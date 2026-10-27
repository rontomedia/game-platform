//! A room: its players, and the game once started. Each room is a task of
//! its own; connections talk to it through `Command`s, so the game needs no
//! locks and the turn timer lives with it.
//!
//! Computer players sit in rooms like people, without a connection. On a
//! bot's turn the room waits a moment, works out its move on a blocking
//! thread (it can take a while on a full table), shows it as a live turn,
//! and plays it.

use std::sync::Arc;
use std::time::Duration;

use engine::{bot_move, within_reach, BotLevel, Game, Move, Rules, Tile};
use protocol::{RoomInfo, RoomOptions, Seat, ServerMessage};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep_until, Instant};

use crate::rooms::Rooms;

pub const MAX_PLAYERS: usize = 4;

/// Names for computer players, the first not taken.
const BOT_NAMES: [&str; 4] = ["Ada", "Bruno", "Cleo", "Dev"];

/// How long things may take.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// A turn; when it runs out the player draws.
    pub turn: Duration,
    /// A turn of a player who has lost their connection.
    pub absent_turn: Duration,
    /// How long a room with nobody connected waits before closing.
    pub abandoned: Duration,
    /// A bot's pause before it moves...
    pub bot_think: Duration,
    /// ...and, in rooms with live turns, how long its play is shown before
    /// the turn ends.
    pub bot_show: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            turn: Duration::from_secs(60),
            absent_turn: Duration::from_secs(5),
            abandoned: Duration::from_secs(120),
            bot_think: Duration::from_millis(1500),
            bot_show: Duration::from_millis(1500),
        }
    }
}

/// A player's identity in a room. Unlike a seat number, it doesn't change
/// when players who left are dropped at the start.
pub type PlayerId = u64;

/// Where a connection receives its messages.
pub type Outbox = mpsc::UnboundedSender<ServerMessage>;

/// A connection, as a room knows it. The id tells a stale connection's
/// `Leave` (after its player came back on a new one) from a current one.
pub struct Connection {
    pub id: u64,
    pub outbox: Outbox,
}

pub enum Command {
    Join { name: String, connection: Connection, reply: oneshot::Sender<Result<PlayerId, String>> },
    Rejoin { token: String, connection: Connection, reply: oneshot::Sender<Result<PlayerId, String>> },
    Leave { player: PlayerId, connection: u64 },
    AddBot { player: PlayerId, level: BotLevel },
    RemoveBot { player: PlayerId, seat: usize },
    Start { player: PlayerId },
    Play { player: PlayerId, table: Vec<Vec<Tile>> },
    Draw { player: PlayerId },
    Draft { player: PlayerId, table: Vec<Vec<Tile>> },
    /// A bot's move, worked out for turn number `turn`.
    BotMove { turn: u64, choice: Move },
}

struct Player {
    id: PlayerId,
    token: String,
    name: String,
    connection: Option<Connection>,
    /// Set for a computer player.
    bot: Option<BotLevel>,
}

impl Player {
    /// Whether they can take their turns: connected, or a bot.
    fn present(&self) -> bool {
        self.connection.is_some() || self.bot.is_some()
    }
}

/// Where a bot is in its turn.
enum BotTurn {
    /// Waiting before it works out its move.
    Thinking,
    /// Working it out.
    Choosing,
    /// Showing this play before making it.
    Showing(Vec<Vec<Tile>>),
}

/// How a room was set up.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub timing: Timing,
    /// Every state tells every player all the racks: for watching how the
    /// bots play, never for real games.
    pub open_racks: bool,
}

pub struct Room {
    code: String,
    config: Config,
    options: RoomOptions,
    /// For bots' moves, worked out elsewhere, to come back as commands. Weak,
    /// so the room still ends when every connection has gone.
    commands: mpsc::WeakSender<Command>,
    /// In seat order.
    players: Vec<Player>,
    next_id: PlayerId,
    game: Option<Game>,
    /// Counts turns, so a bot's move that arrives late is known for one.
    turn: u64,
    /// When the current player's turn runs out.
    turn_ends: Option<Instant>,
    /// The turn's full deadline, while it's cut short because the player
    /// left; given back if they return in time.
    full_turn_ends: Option<Instant>,
    /// When the room closes if nobody comes back.
    closes: Option<Instant>,
    /// The current player's table so far this turn, if they've sent one:
    /// played for them if their time runs out and it's a valid turn, and in
    /// rooms with live turns shown to the others (and to anyone who
    /// reconnects).
    draft: Option<Vec<Vec<Tile>>>,
    /// On a bot's turn: what it's doing, and when it takes its next step.
    bot_turn: Option<(BotTurn, Instant)>,
}

/// Starts a room's task; it removes itself from `rooms` when it closes.
pub fn spawn(code: String, config: Config, options: RoomOptions, rooms: Arc<Rooms>) -> mpsc::Sender<Command> {
    let (sender, commands) = mpsc::channel(64);
    let room = Room {
        code,
        config,
        options,
        commands: sender.downgrade(),
        players: Vec::new(),
        next_id: 0,
        game: None,
        turn: 0,
        turn_ends: None,
        full_turn_ends: None,
        closes: None,
        draft: None,
        bot_turn: None,
    };
    tokio::spawn(room.run(commands, rooms));
    sender
}

impl Room {
    async fn run(mut self, mut commands: mpsc::Receiver<Command>, rooms: Arc<Rooms>) {
        println!("room {} opened", self.code);
        // A room nobody joins closes too.
        self.closes = Some(Instant::now() + self.config.timing.abandoned);
        loop {
            // select! evaluates every branch's future, even a disabled one.
            let never = Instant::now() + Duration::from_secs(365 * 24 * 3600);
            let (turn_ends, closes) = (self.turn_ends, self.closes);
            let bot_step = self.bot_turn.as_ref().filter(|(turn, _)| !matches!(turn, BotTurn::Choosing)).map(|(_, at)| *at);
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.handle(command),
                    None => break,
                },
                _ = sleep_until(turn_ends.unwrap_or(never)), if turn_ends.is_some() => self.time_out(),
                _ = sleep_until(bot_step.unwrap_or(never)), if bot_step.is_some() => self.bot_step(),
                _ = sleep_until(closes.unwrap_or(never)), if closes.is_some() => break,
            }
        }
        rooms.remove(&self.code);
        println!("room {} closed", self.code);
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Join { name, connection, reply } => {
                let _ = reply.send(self.join(name, connection));
            }
            Command::Rejoin { token, connection, reply } => {
                let _ = reply.send(self.rejoin(&token, connection));
            }
            Command::Leave { player, connection } => self.leave(player, connection),
            Command::AddBot { player, level } => self.add_bot(player, level),
            Command::RemoveBot { player, seat } => self.remove_bot(player, seat),
            Command::Start { player } => self.start(player),
            Command::Play { player, table } => self.make_move(player, |game, seat| game.play(seat, table).map(|_| ())),
            Command::Draw { player } => self.make_move(player, |game, seat| game.draw(seat).map(|_| ())),
            Command::Draft { player, table } => self.share_draft(player, table),
            Command::BotMove { turn, choice } => self.bot_chose(turn, choice),
        }
    }

    fn join(&mut self, name: String, connection: Connection) -> Result<PlayerId, String> {
        self.check_seat_free()?;
        let token = format!("{:032x}", rand::random::<u128>());
        let _ = connection.outbox.send(ServerMessage::Joined { code: self.code.clone(), token: token.clone() });
        let id = self.seat_player(name, token, Some(connection), None);
        self.connections_changed();
        Ok(id)
    }

    fn check_seat_free(&self) -> Result<(), String> {
        if self.game.is_some() {
            return Err("the game in this room has started".into());
        }
        if self.players.len() >= MAX_PLAYERS {
            return Err(format!("the room is full ({MAX_PLAYERS} players)"));
        }
        Ok(())
    }

    fn seat_player(&mut self, name: String, token: String, connection: Option<Connection>, bot: Option<BotLevel>) -> PlayerId {
        let id = self.next_id;
        self.next_id += 1;
        self.players.push(Player { id, token, name, connection, bot });
        id
    }

    fn rejoin(&mut self, token: &str, connection: Connection) -> Result<PlayerId, String> {
        let seat = self.players.iter().position(|p| p.bot.is_none() && p.token == token).ok_or("no such seat in this room")?;
        let player = &mut self.players[seat];
        let _ = connection.outbox.send(ServerMessage::Joined { code: self.code.clone(), token: player.token.clone() });
        // Replaces any connection the player still had.
        player.connection = Some(connection);
        let id = player.id;
        if self.game.as_ref().is_some_and(|g| g.current() == seat) {
            if let Some(full) = self.full_turn_ends.take() {
                self.turn_ends = self.turn_ends.map(|t| t.max(full));
            }
        }
        self.connections_changed();
        self.broadcast_state();
        if let Some(table) = self.draft.as_ref().filter(|_| self.options.live_turns) {
            let by = self.game.as_ref().map_or(0, Game::current);
            if by != seat {
                self.send(seat, ServerMessage::Draft { by, table: table.clone() });
            }
        }
        Ok(id)
    }

    fn leave(&mut self, player: PlayerId, connection: u64) {
        let Some(seat) = self.seat(player) else { return };
        if self.players[seat].connection.as_ref().is_none_or(|c| c.id != connection) {
            return;
        }
        self.players[seat].connection = None;
        // Don't keep the others waiting for someone who isn't there.
        if self.game.as_ref().is_some_and(|g| g.current() == seat) {
            let soon = Instant::now() + self.config.timing.absent_turn;
            self.full_turn_ends = self.turn_ends;
            self.turn_ends = self.turn_ends.map(|t| t.min(soon));
            self.broadcast_state();
        }
        self.connections_changed();
    }

    /// Why the host's lobby request from `seat` can't be done, if it can't.
    fn host_refusal(&self, seat: usize) -> Option<&'static str> {
        if self.game.is_some() {
            Some("the game has already started")
        } else if seat != self.host() {
            Some("only the host can do that")
        } else {
            None
        }
    }

    fn add_bot(&mut self, player: PlayerId, level: BotLevel) {
        let Some(seat) = self.seat(player) else { return };
        if let Some(message) = self.host_refusal(seat) {
            return self.send(seat, ServerMessage::Error { message: message.into() });
        }
        if let Err(message) = self.check_seat_free() {
            return self.send(seat, ServerMessage::Error { message });
        }
        let taken = |name: &&str| self.players.iter().any(|p| p.name == **name);
        let name = BOT_NAMES.iter().find(|name| !taken(name)).unwrap_or(&"Bot").to_string();
        self.seat_player(name, format!("{:032x}", rand::random::<u128>()), None, Some(level));
        self.broadcast_room();
    }

    fn remove_bot(&mut self, player: PlayerId, bot: usize) {
        let Some(seat) = self.seat(player) else { return };
        if let Some(message) = self.host_refusal(seat) {
            return self.send(seat, ServerMessage::Error { message: message.into() });
        }
        if self.players.get(bot).is_none_or(|p| p.bot.is_none()) {
            return self.send(seat, ServerMessage::Error { message: "that seat isn't a bot's".into() });
        }
        self.players.remove(bot);
        self.broadcast_room();
    }

    fn start(&mut self, player: PlayerId) {
        let Some(seat) = self.seat(player) else { return };
        let refusal = if self.game.is_some() {
            Some("the game has already started")
        } else if seat != self.host() {
            Some("only the host can start the game")
        } else if self.players.iter().filter(|p| p.present()).count() < 2 {
            Some("a game needs at least 2 players")
        } else {
            None
        };
        if let Some(message) = refusal {
            return self.send(seat, ServerMessage::Error { message: message.into() });
        }
        // Those who left before the start don't get a seat.
        self.players.retain(Player::present);
        self.game = Some(Game::new(self.players.len(), Rules::default(), rand::random()));
        println!("room {}: game started with {} players", self.code, self.players.len());
        self.start_turn();
        self.broadcast_room();
        self.broadcast_state();
    }

    /// A play or a draw by `player`: applied, or rejected to them.
    fn make_move(&mut self, player: PlayerId, apply: impl FnOnce(&mut Game, usize) -> Result<(), engine::TurnError>) {
        let Some(seat) = self.seat(player) else { return };
        let Some(game) = self.game.as_mut() else {
            return self.send(seat, ServerMessage::Error { message: "the game hasn't started".into() });
        };
        if self.players[seat].bot.is_some() {
            return; // only the room moves for bots
        }
        match apply(game, seat) {
            Ok(()) => {
                self.start_turn();
                self.broadcast_state();
            }
            Err(error) => self.send(seat, ServerMessage::Rejected { message: error.to_string(), error }),
        }
    }

    /// The current player's table so far: kept in case their time runs
    /// out, and passed on to the others in rooms with live turns. Only from
    /// the current player, and only tiles they could have: the table's and
    /// their rack's.
    fn share_draft(&mut self, player: PlayerId, table: Vec<Vec<Tile>>) {
        let Some(seat) = self.seat(player) else { return };
        let Some(game) = &self.game else { return };
        if game.current() != seat || game.outcome().is_some() {
            return;
        }
        if !within_reach(game.table(), game.rack(seat), &table) {
            return self.send(seat, ServerMessage::Error { message: "that table has tiles you don't have".into() });
        }
        self.pass_on_draft(seat, table);
    }

    /// Keeps the current player's table so far, shown to the others in rooms
    /// with live turns.
    fn pass_on_draft(&mut self, by: usize, table: Vec<Vec<Tile>>) {
        if self.options.live_turns {
            for other in (0..self.players.len()).filter(|&other| other != by) {
                self.send(other, ServerMessage::Draft { by, table: table.clone() });
            }
        }
        self.draft = Some(table);
    }

    /// A bot's next step in its turn: work out its move, or make the play
    /// it has been showing.
    fn bot_step(&mut self) {
        let Some((step, _)) = self.bot_turn.take() else { return };
        let Some(game) = &self.game else { return };
        let seat = game.current();
        match step {
            BotTurn::Thinking => {
                let Some(level) = self.players[seat].bot else { return };
                let (view, turn) = (game.view(seat), self.turn);
                let Some(commands) = self.commands.upgrade() else { return };
                self.bot_turn = Some((BotTurn::Choosing, Instant::now()));
                tokio::spawn(async move {
                    if let Ok(choice) = tokio::task::spawn_blocking(move || bot_move(&view, level)).await {
                        let _ = commands.send(Command::BotMove { turn, choice }).await;
                    }
                });
            }
            BotTurn::Showing(table) => self.bot_moves(Move::Play(table)),
            BotTurn::Choosing => {}
        }
    }

    /// A bot's move has been worked out: shown first in rooms with live
    /// turns, else made.
    fn bot_chose(&mut self, turn: u64, choice: Move) {
        if turn != self.turn || !matches!(self.bot_turn, Some((BotTurn::Choosing, _))) {
            return; // its turn has passed
        }
        self.bot_turn = None;
        match choice {
            Move::Play(table) if self.options.live_turns => {
                let seat = self.game.as_ref().map_or(0, Game::current);
                self.pass_on_draft(seat, table.clone());
                self.bot_turn = Some((BotTurn::Showing(table), Instant::now() + self.config.timing.bot_show));
            }
            choice => self.bot_moves(choice),
        }
    }

    /// The current player, a bot, makes this move (or draws, should the
    /// game refuse it).
    fn bot_moves(&mut self, choice: Move) {
        let Some(game) = self.game.as_mut() else { return };
        let seat = game.current();
        let played = match choice {
            Move::Play(table) => game.play(seat, table).map_err(|e| eprintln!("room {}: a bot's play: {e}", self.code)),
            Move::Draw => Err(()),
        };
        if played.is_err() {
            let _ = game.draw(seat);
        }
        self.start_turn();
        self.broadcast_state();
    }

    /// The current player ran out of time: the table they left is played
    /// if it's a valid turn, as if they had ended it; else they draw.
    fn time_out(&mut self) {
        let Some(game) = self.game.as_mut() else { return };
        let seat = game.current();
        let played = self.draft.take().is_some_and(|table| game.play(seat, table).is_ok());
        if !played {
            let _ = game.draw(seat);
        }
        self.start_turn();
        self.broadcast_state();
    }

    /// Sets the timer for the turn now beginning (none once the game is
    /// over), and a bot's first step if it's a bot's turn.
    fn start_turn(&mut self) {
        let Some(game) = &self.game else { return };
        self.turn += 1;
        self.full_turn_ends = None;
        self.draft = None;
        self.bot_turn = None;
        let player = &self.players[game.current()];
        let timing = self.config.timing;
        self.turn_ends = game
            .outcome()
            .is_none()
            .then(|| Instant::now() + if player.present() { timing.turn } else { timing.absent_turn });
        if game.outcome().is_none() && player.bot.is_some() {
            self.bot_turn = Some((BotTurn::Thinking, Instant::now() + timing.bot_think));
        }
    }

    /// Someone connected or left: tell everyone, and close the room once
    /// nobody has been here for a while.
    fn connections_changed(&mut self) {
        if self.players.iter().any(|p| p.connection.is_some()) {
            self.closes = None;
        } else if self.closes.is_none() {
            self.closes = Some(Instant::now() + self.config.timing.abandoned);
        }
        self.broadcast_room();
    }

    fn seat(&self, player: PlayerId) -> Option<usize> {
        self.players.iter().position(|p| p.id == player)
    }

    /// The first connected player.
    fn host(&self) -> usize {
        self.players.iter().position(|p| p.connection.is_some()).unwrap_or(0)
    }

    fn send(&self, seat: usize, message: ServerMessage) {
        if let Some(connection) = &self.players[seat].connection {
            let _ = connection.outbox.send(message);
        }
    }

    fn broadcast_room(&self) {
        let seats: Vec<Seat> = self
            .players
            .iter()
            .map(|p| Seat { name: p.name.clone(), connected: p.present(), bot: p.bot })
            .collect();
        for you in 0..self.players.len() {
            let room = RoomInfo {
                code: self.code.clone(),
                seats: seats.clone(),
                you,
                host: self.host(),
                started: self.game.is_some(),
                options: self.options.clone(),
            };
            self.send(you, ServerMessage::Room { room });
        }
    }

    fn send_state(&self, seat: usize) {
        let Some(game) = &self.game else { return };
        let turn_ms = self.turn_ends.map(|t| t.saturating_duration_since(Instant::now()).as_millis() as u32);
        let racks = self.config.open_racks.then(|| (0..game.players()).map(|p| game.rack(p).to_vec()).collect());
        self.send(seat, ServerMessage::State { view: game.view(seat), turn_ms, racks });
    }

    fn broadcast_state(&self) {
        for seat in 0..self.players.len() {
            self.send_state(seat);
        }
    }
}

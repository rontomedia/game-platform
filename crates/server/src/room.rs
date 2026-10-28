//! A room: its players, and the game once started. Each room is a task of
//! its own; connections talk to it through `Command`s, so the game needs no
//! locks and the turn timer lives with it.
//!
//! Computer players sit in rooms like people, without a connection. On a
//! bot's turn the room waits a moment, works out its move on a blocking
//! thread (it can take a while on a full table), shows it as a live turn,
//! and plays it. Bots across the server think a few at a time
//! (`Config::bot_threads`), each for at most `Timing::bot_time`.
//!
//! Others may watch without a seat: they see the table and live turns, and
//! in a game only bots play, every rack too. The host of such a game, who
//! may be watching it, can pause it and step through it a turn at a time.

use std::sync::Arc;
use std::time::Duration;

use engine::{bot_move_with, salvage, within_reach, BotLevel, Game, Lookahead, Move, Rules, Tile};
use protocol::{RoomInfo, RoomOptions, Seat, ServerMessage};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::time::{sleep_until, Instant};

use crate::clock::Bonus;
use crate::rooms::Rooms;

pub const MAX_PLAYERS: usize = 4;
pub const MAX_WATCHERS: usize = 8;

/// Names for computer players, the first not taken.
const BOT_NAMES: [&str; 4] = ["Ada", "Bruno", "Cleo", "Dev"];

/// How long things may take.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// A turn of a minute, the default; rooms that chose another length get
    /// it in proportion. When it runs out, the table the player left is
    /// played if it's a valid turn, else they draw.
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
    /// The longest a bot thinks about a move: Extra Hard plays as well as
    /// it can in that time (the others need far less).
    pub bot_time: Duration,
    /// Each player's reserve for the game, in rooms with a time bank...
    pub bank: Duration,
    /// ...and the most it holds: time left over from turns is added to it.
    pub bank_cap: Duration,
    /// Extra time for tiles, in rooms with `tile_bonus`.
    pub bonus: Bonus,
    /// Extra time a player may be given for tiles they then take back before
    /// they're warned, and before they get no more for the game.
    pub unearned_warning: Duration,
    pub unearned_limit: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            turn: Duration::from_secs(60),
            absent_turn: Duration::from_secs(5),
            abandoned: Duration::from_secs(120),
            bot_think: Duration::from_millis(1500),
            bot_show: Duration::from_millis(1500),
            bot_time: Duration::from_secs(1),
            bank: Duration::from_secs(120),
            bank_cap: Duration::from_secs(180),
            bonus: Bonus { per_tile: Duration::from_secs(15), per_set: Duration::from_secs(20) },
            unearned_warning: Duration::from_secs(60),
            unearned_limit: Duration::from_secs(120),
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
    Join { name: String, watch: bool, connection: Connection, reply: oneshot::Sender<Result<PlayerId, String>> },
    Rejoin { token: String, connection: Connection, reply: oneshot::Sender<Result<PlayerId, String>> },
    Leave { player: PlayerId, connection: u64 },
    Watch { player: PlayerId, watching: bool },
    AddBot { player: PlayerId, level: BotLevel },
    RemoveBot { player: PlayerId, seat: usize },
    Start { player: PlayerId },
    Pause { player: PlayerId, paused: bool },
    Step { player: PlayerId },
    Play { player: PlayerId, table: Vec<Vec<Tile>> },
    Draw { player: PlayerId },
    Draft { player: PlayerId, table: Vec<Vec<Tile>> },
    /// A bot's move, worked out for turn number `turn`.
    BotMove { turn: u64, choice: Move },
}

/// A player in a room, or someone watching (the time fields unused).
struct Player {
    id: PlayerId,
    token: String,
    name: String,
    connection: Option<Connection>,
    /// Set for a computer player.
    bot: Option<BotLevel>,
    /// What's left of their time bank.
    bank: Duration,
    /// Extra time they were given for tiles they then took back...
    unearned: Duration,
    /// ...whether they've been warned about it...
    warned: bool,
    /// ...and whether it's too much: no more extra time this game.
    no_bonus: bool,
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
    /// Making this move, once its play has been shown (in rooms with live
    /// turns).
    Ready(Move),
}

/// How the server's rooms are set up.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub timing: Timing,
    /// Every state tells every player all the racks: for watching how the
    /// bots play, never for real games.
    pub open_racks: bool,
    /// How far Extra Hard bots look ahead.
    pub lookahead: Lookahead,
    /// How many bots may think at once, across all rooms; the others wait
    /// their turn.
    pub bot_threads: usize,
}

pub struct Room {
    code: String,
    config: Config,
    options: RoomOptions,
    /// For bots' moves, worked out elsewhere, to come back as commands. Weak,
    /// so the room still ends when every connection has gone.
    commands: mpsc::WeakSender<Command>,
    /// Shared by every room: a permit for each bot thinking.
    thinking: Arc<Semaphore>,
    /// In seat order.
    players: Vec<Player>,
    /// Those watching, in the order they came.
    watchers: Vec<Player>,
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
    /// Extra time given this turn for tiles: what the best table so far was
    /// worth.
    granted: Duration,
    /// Since when the game has been paused (only bots play it): the clock
    /// and the bots wait.
    paused: Option<Instant>,
    /// Whether to pause again as the next turn begins (a step).
    step: bool,
}

/// Starts a room's task; it removes itself from `rooms` when it closes.
pub fn spawn(code: String, config: Config, options: RoomOptions, rooms: Arc<Rooms>) -> mpsc::Sender<Command> {
    let (sender, commands) = mpsc::channel(64);
    let room = Room {
        code,
        config,
        options,
        commands: sender.downgrade(),
        thinking: rooms.thinking(),
        players: Vec::new(),
        watchers: Vec::new(),
        next_id: 0,
        game: None,
        turn: 0,
        turn_ends: None,
        full_turn_ends: None,
        closes: None,
        draft: None,
        bot_turn: None,
        granted: Duration::ZERO,
        paused: None,
        step: false,
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
            let running = self.paused.is_none();
            let (turn_ends, closes) = (self.deadline().filter(|_| running), self.closes);
            let bot_step = self
                .bot_turn
                .as_ref()
                .filter(|(turn, _)| running && !matches!(turn, BotTurn::Choosing))
                .map(|(_, at)| *at);
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
            Command::Join { name, watch, connection, reply } => {
                let _ = reply.send(self.join(name, watch, connection));
            }
            Command::Rejoin { token, connection, reply } => {
                let _ = reply.send(self.rejoin(&token, connection));
            }
            Command::Leave { player, connection } => self.leave(player, connection),
            Command::Watch { player, watching } => self.watch(player, watching),
            Command::AddBot { player, level } => self.add_bot(player, level),
            Command::RemoveBot { player, seat } => self.remove_bot(player, seat),
            Command::Start { player } => self.start(player),
            Command::Pause { player, paused } => self.pause(player, paused),
            Command::Step { player } => self.step(player),
            Command::Play { player, table } => self.make_move(player, Some(table)),
            Command::Draw { player } => self.make_move(player, None),
            Command::Draft { player, table } => self.share_draft(player, table),
            Command::BotMove { turn, choice } => self.bot_chose(turn, choice),
        }
    }

    fn join(&mut self, name: String, watch: bool, connection: Connection) -> Result<PlayerId, String> {
        if watch {
            self.check_watcher_room()?;
        } else {
            self.check_seat_free()?;
        }
        let token = format!("{:032x}", rand::random::<u128>());
        let _ = connection.outbox.send(ServerMessage::Joined { code: self.code.clone(), token: token.clone() });
        let id = self.seat_player(name, token, Some(connection), None);
        if watch {
            let watcher = self.players.pop().expect("just seated");
            self.watchers.push(watcher);
        }
        self.connections_changed();
        if watch {
            self.send_state_to(id);
            self.send_draft_to(id);
        }
        Ok(id)
    }

    fn check_watcher_room(&self) -> Result<(), String> {
        if self.watchers.len() >= MAX_WATCHERS {
            return Err(format!("enough are watching already ({MAX_WATCHERS})"));
        }
        Ok(())
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
        let bank = self.config.timing.bank;
        self.players.push(Player { id, token, name, connection, bot, bank, unearned: Duration::ZERO, warned: false, no_bonus: false });
        id
    }

    fn rejoin(&mut self, token: &str, connection: Connection) -> Result<PlayerId, String> {
        if let Some(watcher) = self.watchers.iter_mut().find(|w| w.token == token) {
            let _ = connection.outbox.send(ServerMessage::Joined { code: self.code.clone(), token: watcher.token.clone() });
            watcher.connection = Some(connection);
            let id = watcher.id;
            self.connections_changed();
            self.send_state_to(id);
            self.send_draft_to(id);
            return Ok(id);
        }
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
        self.send_draft_to(id);
        Ok(id)
    }

    /// The current player's table so far, for someone who just came, in
    /// rooms with live turns (not for the current player: it's theirs).
    fn send_draft_to(&self, player: PlayerId) {
        let Some(table) = self.draft.as_ref().filter(|_| self.options.live_turns) else { return };
        let by = self.game.as_ref().map_or(0, Game::current);
        if self.seat(player) != Some(by) {
            self.send_to(player, ServerMessage::Draft { by, table: table.clone() });
        }
    }

    fn leave(&mut self, player: PlayerId, connection: u64) {
        if let Some(watcher) = self.watchers.iter_mut().find(|w| w.id == player) {
            if watcher.connection.as_ref().is_some_and(|c| c.id == connection) {
                watcher.connection = None;
                self.connections_changed();
            }
            return;
        }
        let Some(seat) = self.seat(player) else { return };
        if self.players[seat].connection.as_ref().is_none_or(|c| c.id != connection) {
            return;
        }
        self.players[seat].connection = None;
        // Don't keep the others waiting for someone who isn't there.
        if self.game.as_ref().is_some_and(|g| g.current() == seat) {
            // Their bank isn't used while they're away.
            self.settle_bank();
            let soon = Instant::now() + self.config.timing.absent_turn;
            self.full_turn_ends = self.turn_ends;
            self.turn_ends = self.turn_ends.map(|t| t.min(soon));
            self.broadcast_state();
        }
        self.connections_changed();
    }

    /// Watch instead of playing, or take a seat again: before the start.
    fn watch(&mut self, player: PlayerId, watching: bool) {
        if self.game.is_some() {
            return self.send_to(player, ServerMessage::Error { message: "the game has already started".into() });
        }
        let from = if watching { &self.players } else { &self.watchers };
        let Some(at) = from.iter().position(|p| p.id == player) else { return };
        let room = if watching { self.check_watcher_room() } else { self.check_seat_free() };
        if let Err(message) = room {
            return self.send_to(player, ServerMessage::Error { message });
        }
        if watching {
            let moved = self.players.remove(at);
            self.watchers.push(moved);
        } else {
            let moved = self.watchers.remove(at);
            self.players.push(moved);
        }
        self.broadcast_room();
    }

    /// Why the host's lobby request from `player` can't be done, if it can't.
    fn host_refusal(&self, player: PlayerId) -> Option<&'static str> {
        if self.game.is_some() {
            Some("the game has already started")
        } else if self.host() != Some(player) {
            Some("only the host can do that")
        } else {
            None
        }
    }

    fn add_bot(&mut self, player: PlayerId, level: BotLevel) {
        if let Some(message) = self.host_refusal(player) {
            return self.send_to(player, ServerMessage::Error { message: message.into() });
        }
        if let Err(message) = self.check_seat_free() {
            return self.send_to(player, ServerMessage::Error { message });
        }
        let taken = |name: &&str| self.players.iter().any(|p| p.name == **name);
        let name = BOT_NAMES.iter().find(|name| !taken(name)).unwrap_or(&"Bot").to_string();
        self.seat_player(name, format!("{:032x}", rand::random::<u128>()), None, Some(level));
        self.broadcast_room();
    }

    fn remove_bot(&mut self, player: PlayerId, bot: usize) {
        if let Some(message) = self.host_refusal(player) {
            return self.send_to(player, ServerMessage::Error { message: message.into() });
        }
        if self.players.get(bot).is_none_or(|p| p.bot.is_none()) {
            return self.send_to(player, ServerMessage::Error { message: "that seat isn't a bot's".into() });
        }
        self.players.remove(bot);
        self.broadcast_room();
    }

    fn start(&mut self, player: PlayerId) {
        let refusal = if self.game.is_some() {
            Some("the game has already started")
        } else if self.host() != Some(player) {
            Some("only the host can start the game")
        } else if self.players.iter().filter(|p| p.present()).count() < 2 {
            Some("a game needs at least 2 players")
        } else {
            None
        };
        if let Some(message) = refusal {
            return self.send_to(player, ServerMessage::Error { message: message.into() });
        }
        // Those who left before the start don't get a seat.
        self.players.retain(Player::present);
        self.game = Some(Game::new(self.players.len(), Rules::default(), rand::random()));
        println!("room {}: game started with {} players", self.code, self.players.len());
        self.start_turn();
        self.broadcast_room();
        self.broadcast_state();
    }

    /// Why `player` can't pause or step the game, if they can't: only the
    /// host, in a game only bots play.
    fn pause_refusal(&self, player: PlayerId) -> Option<&'static str> {
        if self.game.as_ref().is_none_or(|g| g.outcome().is_some()) {
            Some("there's no game going on")
        } else if self.host() != Some(player) {
            Some("only the host can do that")
        } else if self.players.iter().any(|p| p.bot.is_none()) {
            Some("only a game the bots play alone can be paused")
        } else {
            None
        }
    }

    fn pause(&mut self, player: PlayerId, paused: bool) {
        if let Some(message) = self.pause_refusal(player) {
            return self.send_to(player, ServerMessage::Error { message: message.into() });
        }
        self.step = false;
        if paused {
            self.paused.get_or_insert_with(Instant::now);
        } else {
            self.resume();
        }
        self.broadcast_room();
        self.broadcast_state();
    }

    /// Plays the next turn of a paused game, then pauses again.
    fn step(&mut self, player: PlayerId) {
        if let Some(message) = self.pause_refusal(player) {
            return self.send_to(player, ServerMessage::Error { message: message.into() });
        }
        if self.paused.is_none() {
            return;
        }
        self.resume();
        self.step = true;
        // A bot waiting to think, or to play what it has shown, needn't.
        if let Some((BotTurn::Thinking | BotTurn::Ready(_), at)) = &mut self.bot_turn {
            *at = Instant::now();
        }
        self.broadcast_room();
    }

    /// Restarts the clock and the bots, from where they were when paused.
    fn resume(&mut self) {
        let Some(since) = self.paused.take() else { return };
        let stopped = since.elapsed();
        self.turn_ends = self.turn_ends.map(|t| t + stopped);
        self.full_turn_ends = self.full_turn_ends.map(|t| t + stopped);
        if let Some((_, at)) = &mut self.bot_turn {
            *at += stopped;
        }
    }

    /// A play (the new table) or a draw (`None`) by `player`: applied, or
    /// rejected to them.
    fn make_move(&mut self, player: PlayerId, table: Option<Vec<Vec<Tile>>>) {
        let Some(seat) = self.seat(player) else {
            return self.send_to(player, ServerMessage::Error { message: "you're watching this game, not playing".into() });
        };
        if self.game.is_none() {
            return self.send(seat, ServerMessage::Error { message: "the game hasn't started".into() });
        }
        if self.players[seat].bot.is_some() {
            return; // only the room moves for bots
        }
        let kept = table.as_ref().map_or(Duration::ZERO, |table| self.worth(seat, table));
        // Their turn's time left, should this end it.
        let left = self.turn_ends.map_or(Duration::ZERO, |t| t.saturating_duration_since(Instant::now()));
        if self.game.as_ref().is_some_and(|g| g.current() == seat) {
            self.settle_bank();
        }
        let game = self.game.as_mut().expect("checked above");
        let result = match table {
            Some(table) => game.play(seat, table).map(|_| ()),
            None => game.draw(seat).map(|_| ()),
        };
        match result {
            Ok(()) => self.end_turn(seat, kept, left),
            Err(error) => self.send(seat, ServerMessage::Rejected { message: error.to_string(), error }),
        }
    }

    /// `seat`'s turn has ended with `left` of its time unused, its play
    /// worth `kept` in extra time: banks the time left (not what was given
    /// for tiles then taken back), counts what they were given beyond what
    /// they kept, and begins the next turn.
    fn end_turn(&mut self, seat: usize, kept: Duration, left: Duration) {
        if self.options.time_bank && self.players[seat].bot.is_none() {
            let unearned = self.granted.saturating_sub(kept);
            let cap = self.config.timing.bank_cap;
            let player = &mut self.players[seat];
            player.bank = (player.bank + left.saturating_sub(unearned)).min(cap).max(player.bank);
        }
        let notice = self.count_unearned(seat, kept);
        self.start_turn();
        self.broadcast_state();
        if let Some(message) = notice {
            self.send(seat, ServerMessage::Notice { message });
        }
    }

    /// What `table` would earn `seat` in extra time as their turn (rooms
    /// with `tile_bonus`).
    fn worth(&self, seat: usize, table: &[Vec<Tile>]) -> Duration {
        let Some(game) = self.game.as_ref().filter(|_| self.options.tile_bonus) else { return Duration::ZERO };
        self.config.timing.bonus.worth(game.table(), table, game.rack(seat), game.melded(seat), game.rules())
    }

    /// The current player's table is worth more extra time than any before
    /// it this turn: they get the difference.
    fn give_bonus(&mut self, seat: usize, table: &[Vec<Tile>]) {
        if self.players[seat].no_bonus {
            return;
        }
        let worth = self.worth(seat, table);
        if worth <= self.granted {
            return;
        }
        let added = worth - self.granted;
        self.granted = worth;
        // Into their bank already: the extra time starts from now.
        self.settle_bank();
        self.turn_ends = self.turn_ends.map(|t| t + added);
        self.full_turn_ends = self.full_turn_ends.map(|t| t + added);
        let turn_ms = self.turn_ends.map_or(0, |t| t.saturating_duration_since(Instant::now()).as_millis() as u32);
        self.broadcast(ServerMessage::Clock { by: seat, added_ms: added.as_millis() as u32, turn_ms });
    }

    /// Extra time `seat` was given this turn beyond what their play kept
    /// (`kept`) counts against them: a warning, then no more for the game.
    fn count_unearned(&mut self, seat: usize, kept: Duration) -> Option<String> {
        let granted = std::mem::take(&mut self.granted);
        let timing = self.config.timing;
        let player = &mut self.players[seat];
        if granted <= kept || player.no_bonus {
            return None;
        }
        player.unearned += granted - kept;
        if player.unearned >= timing.unearned_limit {
            player.no_bonus = true;
            return Some(format!(
                "You took back tiles you'd been given {} of extra time for. No more extra time for tiles this game.",
                minutes(player.unearned)
            ));
        }
        if player.unearned >= timing.unearned_warning && !player.warned {
            player.warned = true;
            return Some(format!(
                "Extra time for tiles you then take back counts against you: {} so far. At {} you get no more this game.",
                minutes(player.unearned),
                minutes(timing.unearned_limit)
            ));
        }
        None
    }

    /// Charges the current player's bank for their time past the turn's own
    /// (rooms with a time bank), so the turn's time ends now.
    fn settle_bank(&mut self) {
        let (Some(game), Some(ends)) = (&self.game, self.turn_ends) else { return };
        let now = Instant::now();
        if !self.options.time_bank || now <= ends {
            return;
        }
        let bank = &mut self.players[game.current()].bank;
        *bank = bank.saturating_sub(now - ends);
        self.turn_ends = Some(now);
    }

    /// When the current turn runs out: its own time, then the player's bank
    /// if they're here to use it.
    fn deadline(&self) -> Option<Instant> {
        let ends = self.turn_ends?;
        let player = &self.players[self.game.as_ref()?.current()];
        let banked = self.options.time_bank && player.connection.is_some();
        Some(ends + if banked { player.bank } else { Duration::ZERO })
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
        self.give_bonus(seat, &table);
        self.pass_on_draft(seat, table);
    }

    /// Keeps the current player's table so far, shown to the others in rooms
    /// with live turns.
    fn pass_on_draft(&mut self, by: usize, table: Vec<Vec<Tile>>) {
        if self.options.live_turns {
            for other in (0..self.players.len()).filter(|&other| other != by) {
                self.send(other, ServerMessage::Draft { by, table: table.clone() });
            }
            self.send_watchers(ServerMessage::Draft { by, table: table.clone() });
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
                let (lookahead, limit, thinking) = (self.config.lookahead, self.config.timing.bot_time, self.thinking.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = thinking.acquire_owned().await else { return };
                    let choose = move || {
                        let start = std::time::Instant::now();
                        bot_move_with(&view, level, lookahead, &|| start.elapsed().as_secs_f64() / limit.as_secs_f64())
                    };
                    if let Ok(choice) = tokio::task::spawn_blocking(choose).await {
                        let _ = commands.send(Command::BotMove { turn, choice }).await;
                    }
                });
            }
            BotTurn::Ready(choice) => self.bot_moves(choice),
            BotTurn::Choosing => {}
        }
    }

    /// A bot's move has been worked out: shown first in rooms with live
    /// turns, then made (once the game isn't paused).
    fn bot_chose(&mut self, turn: u64, choice: Move) {
        if turn != self.turn || !matches!(self.bot_turn, Some((BotTurn::Choosing, _))) {
            return; // its turn has passed
        }
        // While paused, from when the pause began: that's shifted on resuming.
        let now = self.paused.unwrap_or_else(Instant::now);
        let at = match &choice {
            Move::Play(table) if self.options.live_turns => {
                let seat = self.game.as_ref().map_or(0, Game::current);
                self.pass_on_draft(seat, table.clone());
                now + self.config.timing.bot_show
            }
            _ => now,
        };
        self.bot_turn = Some((BotTurn::Ready(choice), at));
    }

    /// The current player, a bot, makes this move (or draws, should the
    /// game refuse it).
    fn bot_moves(&mut self, choice: Move) {
        self.settle_bank();
        let Some(game) = self.game.as_mut() else { return };
        let seat = game.current();
        let played = match choice {
            Move::Play(table) => game.play(seat, table).map_err(|e| eprintln!("room {}: a bot's play: {e}", self.code)),
            Move::Draw => Err(()),
        };
        if played.is_err() {
            let _ = game.draw(seat);
        }
        self.end_turn(seat, Duration::ZERO, Duration::ZERO);
    }

    /// The current player ran out of time: as much of the table they left
    /// as makes a valid turn is played (see `engine::salvage`), the rest of
    /// their tiles going back to their rack; if nothing can be, they draw.
    fn time_out(&mut self) {
        self.settle_bank();
        let Some(game) = self.game.as_ref() else { return };
        let seat = game.current();
        let draft = self.draft.take();
        let table = draft
            .as_ref()
            .and_then(|draft| salvage(game.table(), draft, game.rack(seat), game.melded(seat), game.rules()));
        // What they had put down, and what of it is played.
        let size = |table: &[Vec<Tile>]| table.iter().map(Vec::len).sum::<usize>();
        let put_down = draft.as_ref().map_or(0, |d| size(d).saturating_sub(size(game.table())));
        let played = table.as_ref().map_or(0, |t| size(t) - size(game.table()));
        let kept = table.as_ref().map_or(Duration::ZERO, |table| self.worth(seat, table));
        let game = self.game.as_mut().expect("checked above");
        let played = if table.is_some_and(|table| game.play(seat, table).is_ok()) {
            played
        } else {
            let _ = game.draw(seat);
            0
        };
        self.end_turn(seat, kept, Duration::ZERO);
        let message = match (played, put_down.saturating_sub(played)) {
            (0, 0) => None,
            (0, _) => Some("Time ran out and the table wasn't a valid turn, so you drew a tile.".to_string()),
            (played, 0) => Some(format!("Time ran out: your {} played.", tiles(played, "tile was", "tiles were"))),
            (played, back) => Some(format!(
                "Time ran out: {} played, the valid part of your turn; {} back to your rack.",
                tiles(played, "tile", "tiles"),
                tiles(back, "tile went", "tiles went")
            )),
        };
        if let Some(message) = message {
            self.send(seat, ServerMessage::Notice { message });
        }
    }

    /// Sets the timer for the turn now beginning (none once the game is
    /// over), and a bot's first step if it's a bot's turn.
    fn start_turn(&mut self) {
        let Some(game) = &self.game else { return };
        self.turn += 1;
        self.full_turn_ends = None;
        self.draft = None;
        self.bot_turn = None;
        self.granted = Duration::ZERO;
        let player = &self.players[game.current()];
        let timing = self.config.timing;
        let turn = timing.turn * self.options.turn_seconds / 60;
        self.turn_ends =
            game.outcome().is_none().then(|| Instant::now() + if player.present() { turn } else { timing.absent_turn });
        if game.outcome().is_none() && player.bot.is_some() {
            self.bot_turn = Some((BotTurn::Thinking, Instant::now() + timing.bot_think));
        }
        if std::mem::take(&mut self.step) && game.outcome().is_none() {
            self.paused = Some(Instant::now());
            self.broadcast_room();
        }
    }

    /// Someone connected or left: tell everyone, and close the room once
    /// nobody has been here for a while.
    fn connections_changed(&mut self) {
        if self.members().any(|p| p.connection.is_some()) {
            self.closes = None;
        } else if self.closes.is_none() {
            self.closes = Some(Instant::now() + self.config.timing.abandoned);
        }
        self.broadcast_room();
    }

    fn seat(&self, player: PlayerId) -> Option<usize> {
        self.players.iter().position(|p| p.id == player)
    }

    /// Players and those watching.
    fn members(&self) -> impl Iterator<Item = &Player> {
        self.players.iter().chain(&self.watchers)
    }

    /// Of those here, seated or watching, the one who came first (ids count
    /// up); none when nobody is.
    fn host(&self) -> Option<PlayerId> {
        self.members().filter(|p| p.connection.is_some()).map(|p| p.id).min()
    }

    fn send(&self, seat: usize, message: ServerMessage) {
        if let Some(connection) = &self.players[seat].connection {
            let _ = connection.outbox.send(message);
        }
    }

    /// To a member, seated or watching.
    fn send_to(&self, player: PlayerId, message: ServerMessage) {
        if let Some(connection) = self.members().find(|p| p.id == player).and_then(|p| p.connection.as_ref()) {
            let _ = connection.outbox.send(message);
        }
    }

    fn send_watchers(&self, message: ServerMessage) {
        for connection in self.watchers.iter().filter_map(|w| w.connection.as_ref()) {
            let _ = connection.outbox.send(message.clone());
        }
    }

    /// To everyone, seated or watching.
    fn broadcast(&self, message: ServerMessage) {
        for connection in self.members().filter_map(|p| p.connection.as_ref()) {
            let _ = connection.outbox.send(message.clone());
        }
    }

    fn broadcast_room(&self) {
        let host = self.host();
        let seat = |p: &Player| Seat { name: p.name.clone(), connected: p.present(), bot: p.bot, host: host == Some(p.id) };
        let room = RoomInfo {
            code: self.code.clone(),
            seats: self.players.iter().map(seat).collect(),
            watchers: self.watchers.iter().map(seat).collect(),
            you: None,
            watching: None,
            started: self.game.is_some(),
            paused: self.paused.is_some(),
            options: self.options.clone(),
        };
        for you in 0..self.players.len() {
            self.send(you, ServerMessage::Room { room: RoomInfo { you: Some(you), ..room.clone() } });
        }
        for (watching, watcher) in self.watchers.iter().enumerate() {
            if let Some(connection) = &watcher.connection {
                let room = RoomInfo { watching: Some(watching), ..room.clone() };
                let _ = connection.outbox.send(ServerMessage::Room { room });
            }
        }
    }

    /// The state for a member, seated or watching.
    fn send_state_to(&self, player: PlayerId) {
        match self.seat(player) {
            Some(seat) => self.send_state(seat),
            None => {
                if let Some(state) = self.state(None) {
                    self.send_to(player, state);
                }
            }
        }
    }

    fn send_state(&self, seat: usize) {
        if let Some(state) = self.state(Some(seat)) {
            self.send(seat, state);
        }
    }

    /// The game as `seat` may see it, or someone watching (`None`).
    fn state(&self, seat: Option<usize>) -> Option<ServerMessage> {
        let game = self.game.as_ref()?;
        // The clock stands still while the game is paused.
        let now = self.paused.unwrap_or_else(Instant::now);
        let turn_ms = self.turn_ends.map(|t| t.saturating_duration_since(now).as_millis() as u32);
        // Watching a game only bots play, there's nobody to give a rack away to.
        let open = self.config.open_racks || (seat.is_none() && self.players.iter().all(|p| p.bot.is_some()));
        let racks = open.then(|| (0..game.players()).map(|p| game.rack(p).to_vec()).collect());
        // The current player's bank as of now, should they be using it.
        let overdue = self.turn_ends.map_or(Duration::ZERO, |t| now.saturating_duration_since(t));
        let banks = self.options.time_bank.then(|| {
            let current = game.current();
            self.players
                .iter()
                .enumerate()
                .map(|(p, player)| {
                    let left = if p == current { player.bank.saturating_sub(overdue) } else { player.bank };
                    left.as_millis() as u32
                })
                .collect()
        });
        let view = seat.map_or_else(|| game.watcher_view(), |seat| game.view(seat));
        Some(ServerMessage::State { view, turn_ms, banks, racks })
    }

    fn broadcast_state(&self) {
        for seat in 0..self.players.len() {
            self.send_state(seat);
        }
        if let Some(state) = self.state(None) {
            self.send_watchers(state);
        }
    }
}

/// A duration as minutes and seconds: "1:05".
fn minutes(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// "1 tile", "3 tiles", with words for one and for more.
fn tiles(count: usize, one: &str, more: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { more })
}

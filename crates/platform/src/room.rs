//! A room: its players, and the game once started. Each room is a task of
//! its own; connections talk to it through `Command`s, so the game needs no
//! locks and the turn timer lives with it.
//!
//! Computer players sit in rooms like people, without a connection. On a
//! bot's turn the room waits a moment, works out its move on a blocking
//! thread (it can take a while), shows it as a live turn, and plays it.
//! Bots across the server think a few at a time (the permits `Rooms` is
//! given), each for at most `Timing::bot_time`.
//!
//! Others may watch without a seat: they see the game and live turns, and
//! in a game only bots play, every hidden hand too. The host of such a
//! game, who may be watching it, can pause it and step through it a turn
//! at a time.
//!
//! The room knows its game only as a `GameEngine` (`G`): what a move is,
//! whether it ends the turn (Okey's draw doesn't), what a player whose time
//! ran out plays, and what each is sent. Between hands the scores are shown
//! for `Timing::between_hands` before the next deal.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::time::{sleep_until, Instant};

use crate::engine::{ClockInfo, GameEngine, TimedOut};
use crate::rooms::Rooms;
use crate::shared::{Notice, Out, Refusal, RoomInfo, RoomOptions, Seat, SharedMessage};

pub const MAX_WATCHERS: usize = 8;

/// Names for computer players, the first not taken.
const BOT_NAMES: [&str; 4] = ["Ada", "Bruno", "Cleo", "Dev"];

/// How long things may take.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// A turn of a minute, the default; rooms that chose another length get
    /// it in proportion. When it runs out, the game plays something for
    /// the player (`GameEngine::time_out`).
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
    /// Extra time a player may be given for a draft they then take back before
    /// they're warned, and before they get no more for the game.
    pub unearned_warning: Duration,
    pub unearned_limit: Duration,
    /// How long a hand's scores are shown before the next deal.
    pub between_hands: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            turn: Duration::from_secs(60),
            absent_turn: Duration::from_secs(5),
            abandoned: Duration::from_secs(120),
            bot_think: Duration::from_millis(1500),
            // Long enough for its tiles to glide in one by one (web/src/flights.ts).
            bot_show: Duration::from_millis(2800),
            bot_time: Duration::from_secs(1),
            bank: Duration::from_secs(120),
            bank_cap: Duration::from_secs(180),
            unearned_warning: Duration::from_secs(60),
            unearned_limit: Duration::from_secs(120),
            between_hands: Duration::from_secs(12),
        }
    }
}

/// A player's identity in a room. Unlike a seat number, it doesn't change
/// when players who left are dropped at the start.
pub type PlayerId = u64;

/// Where a connection receives its messages.
pub type Outbox<G> = mpsc::UnboundedSender<Out<G>>;

/// A connection, as a room knows it. The id tells a stale connection's
/// `Disconnect` (after its player came back on a new one) from a current one.
pub struct Connection<G: GameEngine> {
    pub id: u64,
    pub outbox: Outbox<G>,
}

/// The answer to joining a room.
pub type Reply = oneshot::Sender<Result<PlayerId, Refusal>>;

pub enum Command<G: GameEngine> {
    Join { name: String, watch: bool, connection: Connection<G>, reply: Reply },
    Rejoin { token: String, connection: Connection<G>, reply: Reply },
    /// A connection has gone; its player may come back.
    Disconnect { player: PlayerId, connection: u64 },
    /// A player leaves for good.
    Leave { player: PlayerId },
    Watch { player: PlayerId, watching: bool },
    AddBot { player: PlayerId, level: G::Level },
    RemoveBot { player: PlayerId, seat: usize },
    Start { player: PlayerId },
    Pause { player: PlayerId, paused: bool },
    Step { player: PlayerId },
    Move { player: PlayerId, mv: G::Move },
    /// The current player's work in progress.
    Draft { player: PlayerId, draft: G::Draft },
    /// Something a seated player tells the game that isn't a move.
    Aside { player: PlayerId, aside: G::Aside },
    /// A bot's move, worked out for turn number `turn`.
    BotMove { turn: u64, mv: G::Move },
}

/// A player in a room, or someone watching (the time fields unused).
struct Player<G: GameEngine> {
    id: PlayerId,
    token: String,
    name: String,
    connection: Option<Connection<G>>,
    /// Set for a computer player.
    bot: Option<G::Level>,
    /// What's left of their time bank.
    bank: Duration,
    /// Extra time they were given for a draft they then took back...
    unearned: Duration,
    /// ...whether they've been warned about it...
    warned: bool,
    /// ...and whether it's too much: no more extra time this game.
    no_bonus: bool,
}

impl<G: GameEngine> Player<G> {
    /// Whether they can take their turns: connected, or a bot.
    fn present(&self) -> bool {
        self.connection.is_some() || self.bot.is_some()
    }
}

/// Where a bot is in its turn.
enum BotTurn<G: GameEngine> {
    /// Waiting before it works out its move.
    Thinking,
    /// Working it out.
    Choosing,
    /// Making this move, once its play has been shown (in rooms with live
    /// turns).
    Ready(G::Move),
}

/// How the server's rooms are set up, whatever their game.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub timing: Timing,
    /// Every state shows every player all the hidden hands (tile rummy:
    /// racks): for watching how the bots play, never for real games.
    pub open_racks: bool,
}

pub struct Room<G: GameEngine> {
    code: String,
    config: Config,
    /// The game's setup for its bots and clock.
    game_config: G::Config,
    options: RoomOptions<G::Rules, G::Level>,
    /// For bots' moves, worked out elsewhere, to come back as commands. Weak,
    /// so the room still ends when every connection has gone.
    commands: mpsc::WeakSender<Command<G>>,
    /// Shared by every room: a permit for each bot thinking.
    thinking: Arc<Semaphore>,
    /// In seat order.
    players: Vec<Player<G>>,
    /// Those watching, in the order they came.
    watchers: Vec<Player<G>>,
    next_id: PlayerId,
    game: Option<G>,
    /// Between hands: when the next is dealt.
    next_hand_at: Option<Instant>,
    /// Counts turns, so a bot's move that arrives late is known for one.
    turn: u64,
    /// When the current player's turn runs out.
    turn_ends: Option<Instant>,
    /// The turn's full deadline, while it's cut short because the player
    /// left; given back if they return in time.
    full_turn_ends: Option<Instant>,
    /// When the room closes if nobody comes back.
    closes: Option<Instant>,
    /// The current player's work so far this turn, if they've sent any:
    /// what's played for them if their time runs out, and in rooms with
    /// live turns shown to the others (and to anyone who reconnects).
    draft: Option<G::Draft>,
    /// On a bot's turn: what it's doing, and when it takes its next step.
    bot_turn: Option<(BotTurn<G>, Instant)>,
    /// Extra time given this turn: what the best draft so far was worth.
    granted: Duration,
    /// Since when the game has been paused (only bots play it): the clock
    /// and the bots wait.
    paused: Option<Instant>,
    /// Whether to pause again as the next turn begins (a step).
    step: bool,
}

/// Starts a room's task; it removes itself from `rooms` when it closes.
pub fn spawn<G: GameEngine>(
    code: String,
    config: Config,
    game_config: G::Config,
    options: RoomOptions<G::Rules, G::Level>,
    rooms: Arc<Rooms<G>>,
) -> mpsc::Sender<Command<G>> {
    let (sender, commands) = mpsc::channel(64);
    let room = Room {
        code,
        config,
        game_config,
        options,
        commands: sender.downgrade(),
        thinking: rooms.thinking(),
        players: Vec::new(),
        watchers: Vec::new(),
        next_id: 0,
        game: None,
        next_hand_at: None,
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

impl<G: GameEngine> Room<G> {
    async fn run(mut self, mut commands: mpsc::Receiver<Command<G>>, rooms: Arc<Rooms<G>>) {
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
            let next_hand = self.next_hand_at.filter(|_| running);
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.handle(command),
                    None => break,
                },
                _ = sleep_until(turn_ends.unwrap_or(never)), if turn_ends.is_some() => self.time_out(),
                _ = sleep_until(bot_step.unwrap_or(never)), if bot_step.is_some() => self.bot_step(),
                _ = sleep_until(next_hand.unwrap_or(never)), if next_hand.is_some() => self.next_hand(),
                _ = sleep_until(closes.unwrap_or(never)), if closes.is_some() => break,
            }
        }
        rooms.remove(&self.code);
        println!("room {} closed", self.code);
    }

    fn handle(&mut self, command: Command<G>) {
        match command {
            Command::Join { name, watch, connection, reply } => {
                let _ = reply.send(self.join(name, watch, connection));
            }
            Command::Rejoin { token, connection, reply } => {
                let _ = reply.send(self.rejoin(&token, connection));
            }
            Command::Disconnect { player, connection } => self.disconnect(player, connection),
            Command::Leave { player } => self.leave(player),
            Command::Watch { player, watching } => self.watch(player, watching),
            Command::AddBot { player, level } => self.add_bot(player, level),
            Command::RemoveBot { player, seat } => self.remove_bot(player, seat),
            Command::Start { player } => self.start(player),
            Command::Pause { player, paused } => self.pause(player, paused),
            Command::Step { player } => self.step(player),
            Command::Move { player, mv } => self.make_move(player, mv),
            Command::Draft { player, draft } => self.share_draft(player, draft),
            Command::Aside { player, aside } => {
                if let (Some(seat), Some(game)) = (self.seat(player), self.game.as_mut()) {
                    game.aside(seat, aside);
                }
            }
            Command::BotMove { turn, mv } => self.bot_chose(turn, mv),
        }
    }

    fn join(&mut self, name: String, watch: bool, connection: Connection<G>) -> Result<PlayerId, Refusal> {
        if watch {
            self.check_watcher_room()?;
        } else {
            self.check_seat_free()?;
        }
        let token = format!("{:032x}", rand::random::<u128>());
        let _ = connection.outbox.send(Out::Shared(SharedMessage::Joined { code: self.code.clone(), token: token.clone() }));
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

    fn check_watcher_room(&self) -> Result<(), Refusal> {
        if self.watchers.len() >= MAX_WATCHERS {
            return Err(Refusal::WatchersFull { max: MAX_WATCHERS as u32 });
        }
        Ok(())
    }

    fn check_seat_free(&self) -> Result<(), Refusal> {
        if self.game.is_some() {
            return Err(Refusal::GameStarted);
        }
        let max = *G::seats(&self.options.game).end();
        if self.players.len() >= max {
            return Err(Refusal::RoomFull { max: max as u32 });
        }
        Ok(())
    }

    fn seat_player(&mut self, name: String, token: String, connection: Option<Connection<G>>, bot: Option<G::Level>) -> PlayerId {
        let id = self.next_id;
        self.next_id += 1;
        let bank = self.config.timing.bank;
        self.players.push(Player { id, token, name, connection, bot, bank, unearned: Duration::ZERO, warned: false, no_bonus: false });
        id
    }

    fn rejoin(&mut self, token: &str, connection: Connection<G>) -> Result<PlayerId, Refusal> {
        if let Some(watcher) = self.watchers.iter_mut().find(|w| w.token == token) {
            let joined = SharedMessage::Joined { code: self.code.clone(), token: watcher.token.clone() };
            let _ = connection.outbox.send(Out::Shared(joined));
            watcher.connection = Some(connection);
            let id = watcher.id;
            self.connections_changed();
            self.send_state_to(id);
            self.send_draft_to(id);
            return Ok(id);
        }
        let seat = self.players.iter().position(|p| p.bot.is_none() && p.token == token).ok_or(Refusal::NoSuchSeat)?;
        let player = &mut self.players[seat];
        let joined = SharedMessage::Joined { code: self.code.clone(), token: player.token.clone() };
        let _ = connection.outbox.send(Out::Shared(joined));
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

    /// The current player's draft so far, for someone who just came, in
    /// rooms with live turns (not for the current player: it's theirs).
    fn send_draft_to(&self, player: PlayerId) {
        let Some(draft) = self.draft.as_ref().filter(|_| self.options.live_turns) else { return };
        let Some(game) = &self.game else { return };
        let by = game.current();
        if self.seat(player) != Some(by) {
            if let Some(message) = game.draft_message(by, draft) {
                self.send_to(player, Out::Game(message));
            }
        }
    }

    fn disconnect(&mut self, player: PlayerId, connection: u64) {
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

    /// `player` leaves for good. Before the start, or watching, they're
    /// gone from the room; during the game, the room's stand-in bot takes
    /// over their seat (taking a turn of theirs afresh). Once the game is
    /// over, their seat stays for the results.
    fn leave(&mut self, player: PlayerId) {
        if let Some(at) = self.watchers.iter().position(|w| w.id == player) {
            self.watchers.remove(at);
            return self.connections_changed();
        }
        let Some(seat) = self.seat(player) else { return };
        match &self.game {
            None => {
                self.players.remove(seat);
            }
            Some(game) if !game.over() => {
                let current = game.current() == seat && game.in_turn();
                let stand_in = &mut self.players[seat];
                stand_in.connection = None;
                stand_in.bot = Some(self.options.stand_in);
                println!("room {}: {} left, a bot plays on for them", self.code, stand_in.name);
                if current {
                    self.start_turn();
                }
                self.broadcast_state();
            }
            Some(_) => self.players[seat].connection = None,
        }
        self.connections_changed();
    }

    /// Watch instead of playing, or take a seat again: before the start.
    fn watch(&mut self, player: PlayerId, watching: bool) {
        if self.game.is_some() {
            return self.refuse(player, Refusal::GameStarted);
        }
        let from = if watching { &self.players } else { &self.watchers };
        let Some(at) = from.iter().position(|p| p.id == player) else { return };
        let room = if watching { self.check_watcher_room() } else { self.check_seat_free() };
        if let Err(refusal) = room {
            return self.refuse(player, refusal);
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
    fn host_refusal(&self, player: PlayerId) -> Option<Refusal> {
        if self.game.is_some() {
            Some(Refusal::GameStarted)
        } else if self.host() != Some(player) {
            Some(Refusal::OnlyHost)
        } else {
            None
        }
    }

    fn add_bot(&mut self, player: PlayerId, level: G::Level) {
        if let Some(refusal) = self.host_refusal(player) {
            return self.refuse(player, refusal);
        }
        if let Err(refusal) = self.check_seat_free() {
            return self.refuse(player, refusal);
        }
        let taken = |name: &&str| self.players.iter().any(|p| p.name == **name);
        let name = BOT_NAMES.iter().find(|name| !taken(name)).unwrap_or(&"Bot").to_string();
        self.seat_player(name, format!("{:032x}", rand::random::<u128>()), None, Some(level));
        self.broadcast_room();
    }

    fn remove_bot(&mut self, player: PlayerId, bot: usize) {
        if let Some(refusal) = self.host_refusal(player) {
            return self.refuse(player, refusal);
        }
        if self.players.get(bot).is_none_or(|p| p.bot.is_none()) {
            return self.refuse(player, Refusal::NotABot);
        }
        self.players.remove(bot);
        self.broadcast_room();
    }

    fn start(&mut self, player: PlayerId) {
        let refusal = if self.game.is_some() {
            Some(Refusal::GameStarted)
        } else if self.host() != Some(player) {
            Some(Refusal::OnlyHost)
        } else if self.players.iter().filter(|p| p.present()).count() < *G::seats(&self.options.game).start() {
            Some(Refusal::TooFewPlayers { min: *G::seats(&self.options.game).start() as u32 })
        } else {
            None
        };
        if let Some(refusal) = refusal {
            return self.refuse(player, refusal);
        }
        // Those who left before the start don't get a seat.
        self.players.retain(Player::present);
        self.game = Some(G::new(&self.options.game, self.players.len(), rand::random()));
        println!("room {}: game started with {} players", self.code, self.players.len());
        self.start_turn();
        self.broadcast_room();
        self.broadcast_state();
    }

    /// Why `player` can't pause or step the game, if they can't: only the
    /// host, in a game only bots play.
    fn pause_refusal(&self, player: PlayerId) -> Option<Refusal> {
        if self.game.as_ref().is_none_or(G::over) {
            Some(Refusal::NoGame)
        } else if self.host() != Some(player) {
            Some(Refusal::OnlyHost)
        } else if self.players.iter().any(|p| p.bot.is_none()) {
            Some(Refusal::NotBotsOnly)
        } else {
            None
        }
    }

    fn pause(&mut self, player: PlayerId, paused: bool) {
        if let Some(refusal) = self.pause_refusal(player) {
            return self.refuse(player, refusal);
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
        if let Some(refusal) = self.pause_refusal(player) {
            return self.refuse(player, refusal);
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
        self.next_hand_at = self.next_hand_at.map(|t| t + stopped);
        self.full_turn_ends = self.full_turn_ends.map(|t| t + stopped);
        if let Some((_, at)) = &mut self.bot_turn {
            *at += stopped;
        }
    }

    /// A move by `player`: applied, or rejected to them.
    fn make_move(&mut self, player: PlayerId, mv: G::Move) {
        let Some(seat) = self.seat(player) else {
            return self.refuse(player, Refusal::Watching);
        };
        let Some(game) = &self.game else {
            return self.refuse(player, Refusal::NotStarted);
        };
        if self.players[seat].bot.is_some() {
            return; // only the room moves for bots
        }
        let kept = G::move_draft(&mv).map_or(Duration::ZERO, |draft| self.worth(seat, draft));
        // Their turn's time left, should this end it.
        let left = self.turn_ends.map_or(Duration::ZERO, |t| t.saturating_duration_since(Instant::now()));
        let ends_turn = game.ends_turn(&mv);
        if ends_turn && game.current() == seat {
            self.settle_bank();
        }
        match self.game.as_mut().expect("checked above").apply(seat, mv) {
            Ok(()) if ends_turn => self.end_turn(seat, kept, left),
            Ok(()) => self.went_on(seat, left),
            Err(message) => self.send(seat, Out::Game(message)),
        }
    }

    /// `seat` has made a move that doesn't end their turn (drawn in Okey):
    /// it goes on, unless the hand has ended (the pool was empty).
    fn went_on(&mut self, seat: usize, left: Duration) {
        if self.game.as_ref().is_some_and(G::in_turn) {
            self.broadcast_state();
        } else {
            self.settle_bank();
            self.end_turn(seat, Duration::ZERO, left);
        }
    }

    /// `seat`'s turn has ended with `left` of its time unused, its play
    /// worth `kept` in extra time: banks the time left (not what was given
    /// for a draft then taken back), counts what they were given beyond what
    /// they kept, and begins the next turn.
    fn end_turn(&mut self, seat: usize, kept: Duration, left: Duration) {
        if self.options.time_bank && self.players[seat].bot.is_none() {
            let unearned = self.granted.saturating_sub(kept);
            let cap = self.config.timing.bank_cap;
            let player = &mut self.players[seat];
            player.bank = (player.bank + left.saturating_sub(unearned)).min(cap).max(player.bank);
        }
        let notice = self.count_unearned(seat, kept);
        if let Some(places) = self.game.as_ref().and_then(G::placings) {
            println!("room {}: game over, places {places:?}", self.code);
        }
        self.start_turn();
        self.broadcast_state();
        if let Some(notice) = notice {
            self.send(seat, Out::Shared(SharedMessage::Notice { notice }));
        }
    }

    /// What `draft` would earn `seat` in extra time as their turn.
    fn worth(&self, seat: usize, draft: &G::Draft) -> Duration {
        self.game.as_ref().map_or(Duration::ZERO, |game| game.worth(&self.game_config, seat, draft))
    }

    /// The current player's draft is worth more extra time than any before
    /// it this turn: they get the difference.
    fn give_bonus(&mut self, seat: usize, draft: &G::Draft) {
        if self.players[seat].no_bonus {
            return;
        }
        let worth = self.worth(seat, draft);
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
        self.broadcast(Out::Shared(SharedMessage::Clock { by: seat, added_ms: added.as_millis() as u32, turn_ms }));
    }

    /// Extra time `seat` was given this turn beyond what their play kept
    /// (`kept`) counts against them: a warning, then no more for the game.
    fn count_unearned(&mut self, seat: usize, kept: Duration) -> Option<Notice> {
        let granted = std::mem::take(&mut self.granted);
        let timing = self.config.timing;
        let player = &mut self.players[seat];
        if granted <= kept || player.no_bonus {
            return None;
        }
        player.unearned += granted - kept;
        if player.unearned >= timing.unearned_limit {
            player.no_bonus = true;
            return Some(Notice::NoMoreBonus { taken_back_ms: player.unearned.as_millis() as u32 });
        }
        if player.unearned >= timing.unearned_warning && !player.warned {
            player.warned = true;
            return Some(Notice::UnearnedWarning {
                so_far_ms: player.unearned.as_millis() as u32,
                limit_ms: timing.unearned_limit.as_millis() as u32,
            });
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

    /// The current player's work so far: kept in case their time runs
    /// out, and passed on to the others in rooms with live turns. Only from
    /// the current player, and only what they could use (tile rummy: the
    /// table's tiles and their rack's).
    fn share_draft(&mut self, player: PlayerId, draft: G::Draft) {
        let Some(seat) = self.seat(player) else { return };
        let Some(game) = &self.game else { return };
        if game.current() != seat || !game.in_turn() {
            return;
        }
        if !game.draft_allowed(&draft) {
            return self.refuse(player, Refusal::NotYourTiles);
        }
        self.give_bonus(seat, &draft);
        self.pass_on_draft(seat, draft);
    }

    /// Keeps the current player's work so far, shown to the others in rooms
    /// with live turns.
    fn pass_on_draft(&mut self, by: usize, draft: G::Draft) {
        let message = self.game.as_ref().and_then(|game| game.draft_message(by, &draft)).map(Out::Game);
        if let Some(message) = message.filter(|_| self.options.live_turns) {
            for other in (0..self.players.len()).filter(|&other| other != by) {
                self.send(other, message.clone());
            }
            self.send_watchers(message);
        }
        self.draft = Some(draft);
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
                let (game, turn) = (game.clone(), self.turn);
                let Some(commands) = self.commands.upgrade() else { return };
                self.bot_turn = Some((BotTurn::Choosing, Instant::now()));
                let (config, limit, thinking) = (self.game_config, self.config.timing.bot_time, self.thinking.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = thinking.acquire_owned().await else { return };
                    let choose = move || {
                        let start = std::time::Instant::now();
                        game.bot_move(level, &config, &|| start.elapsed().as_secs_f64() / limit.as_secs_f64())
                    };
                    if let Ok(mv) = tokio::task::spawn_blocking(choose).await {
                        let _ = commands.send(Command::BotMove { turn, mv }).await;
                    }
                });
            }
            BotTurn::Ready(mv) => self.bot_moves(mv),
            BotTurn::Choosing => {}
        }
    }

    /// A bot's move has been worked out: shown first in rooms with live
    /// turns, then made (once the game isn't paused).
    fn bot_chose(&mut self, turn: u64, mv: G::Move) {
        if turn != self.turn || !matches!(self.bot_turn, Some((BotTurn::Choosing, _))) {
            return; // its turn has passed
        }
        let Some(game) = &self.game else { return };
        let (shown, seat) = (game.shown(&mv), game.current());
        // While paused, from when the pause began: that's shifted on resuming.
        let now = self.paused.unwrap_or_else(Instant::now);
        let at = match shown {
            Some(draft) if self.options.live_turns => {
                self.pass_on_draft(seat, draft);
                now + self.config.timing.bot_show
            }
            _ => now,
        };
        self.bot_turn = Some((BotTurn::Ready(mv), at));
    }

    /// The current player, a bot, makes this move. Should the game refuse
    /// one that would end its turn, it makes some allowed move instead
    /// (`bot_fallback`).
    fn bot_moves(&mut self, mv: G::Move) {
        let Some(game) = self.game.as_mut() else { return };
        let seat = game.current();
        let ends_turn = game.ends_turn(&mv);
        match game.apply(seat, mv) {
            Ok(()) if !ends_turn && game.in_turn() => {
                // It thinks again, about the rest of its turn.
                self.bot_turn = Some((BotTurn::Thinking, Instant::now() + self.config.timing.bot_think));
                self.broadcast_state();
                return;
            }
            Err(message) if ends_turn => {
                eprintln!("room {}: a bot's move: {message:?}", self.code);
                game.bot_fallback();
            }
            _ => {}
        }
        self.settle_bank();
        self.end_turn(seat, Duration::ZERO, Duration::ZERO);
    }

    /// The current player ran out of time: the game plays something for
    /// them, from their draft if they sent one.
    fn time_out(&mut self) {
        self.settle_bank();
        let Some(game) = self.game.as_mut() else { return };
        let seat = game.current();
        let TimedOut { kept, notice } = game.time_out(&self.game_config, self.draft.take());
        self.end_turn(seat, kept, Duration::ZERO);
        if let Some(notice) = notice {
            self.send(seat, Out::Game(notice));
        }
    }

    /// Deals the next hand once the scores have been shown.
    fn next_hand(&mut self) {
        self.next_hand_at = None;
        if self.game.as_mut().is_some_and(G::next_hand) {
            self.start_turn();
            self.broadcast_state();
        }
    }

    /// Sets the timer for the turn now beginning (none once the game is
    /// over, or a hand), and a bot's first step if it's a bot's turn.
    /// After a hand, when the next is dealt.
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
        let in_turn = game.in_turn();
        self.turn_ends = in_turn.then(|| Instant::now() + if player.present() { turn } else { timing.absent_turn });
        self.next_hand_at = (!in_turn && !game.over()).then(|| Instant::now() + timing.between_hands);
        if in_turn && player.bot.is_some() {
            self.bot_turn = Some((BotTurn::Thinking, Instant::now() + timing.bot_think));
        }
        if in_turn && std::mem::take(&mut self.step) {
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
    fn members(&self) -> impl Iterator<Item = &Player<G>> {
        self.players.iter().chain(&self.watchers)
    }

    /// Of those here, seated or watching, the one who came first (ids count
    /// up); none when nobody is.
    fn host(&self) -> Option<PlayerId> {
        self.members().filter(|p| p.connection.is_some()).map(|p| p.id).min()
    }

    fn send(&self, seat: usize, message: Out<G>) {
        if let Some(connection) = &self.players[seat].connection {
            let _ = connection.outbox.send(message);
        }
    }

    /// To a member, seated or watching.
    fn refuse(&self, player: PlayerId, refusal: Refusal) {
        self.send_to(player, Out::Shared(SharedMessage::Refused { refusal }));
    }

    fn send_to(&self, player: PlayerId, message: Out<G>) {
        if let Some(connection) = self.members().find(|p| p.id == player).and_then(|p| p.connection.as_ref()) {
            let _ = connection.outbox.send(message);
        }
    }

    fn send_watchers(&self, message: Out<G>) {
        for connection in self.watchers.iter().filter_map(|w| w.connection.as_ref()) {
            let _ = connection.outbox.send(message.clone());
        }
    }

    /// To everyone, seated or watching.
    fn broadcast(&self, message: Out<G>) {
        for connection in self.members().filter_map(|p| p.connection.as_ref()) {
            let _ = connection.outbox.send(message.clone());
        }
    }

    fn broadcast_room(&self) {
        let host = self.host();
        let seat = |p: &Player<G>| Seat { name: p.name.clone(), connected: p.present(), bot: p.bot, host: host == Some(p.id) };
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
            self.send(you, Out::Shared(SharedMessage::Room { room: RoomInfo { you: Some(you), ..room.clone() } }));
        }
        for (watching, watcher) in self.watchers.iter().enumerate() {
            if let Some(connection) = &watcher.connection {
                let room = RoomInfo { watching: Some(watching), ..room.clone() };
                let _ = connection.outbox.send(Out::Shared(SharedMessage::Room { room }));
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
    fn state(&self, seat: Option<usize>) -> Option<Out<G>> {
        let game = self.game.as_ref()?;
        // The clock stands still while the game is paused.
        let now = self.paused.unwrap_or_else(Instant::now);
        let turn_ms = self.turn_ends.map(|t| t.saturating_duration_since(now).as_millis() as u32);
        let next_hand_ms = self.next_hand_at.map(|t| t.saturating_duration_since(now).as_millis() as u32);
        // Watching a game only bots play, there's nobody to give a hand away to.
        let open = self.config.open_racks || (seat.is_none() && self.players.iter().all(|p| p.bot.is_some()));
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
        Some(Out::Game(game.state(seat, &ClockInfo { turn_ms, banks, next_hand_ms }, open)))
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

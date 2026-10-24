//! A room: its players, and the game once started. Each room is a task of
//! its own; connections talk to it through `Command`s, so the game needs no
//! locks and the turn timer lives with it.

use std::sync::Arc;
use std::time::Duration;

use engine::{Game, Rules, Tile};
use protocol::{RoomInfo, Seat, ServerMessage};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep_until, Instant};

use crate::rooms::Rooms;

pub const MAX_PLAYERS: usize = 4;

/// How long things may take.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// A turn; when it runs out the player draws.
    pub turn: Duration,
    /// A turn of a player who has lost their connection.
    pub absent_turn: Duration,
    /// How long a room with nobody connected waits before closing.
    pub abandoned: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing { turn: Duration::from_secs(60), absent_turn: Duration::from_secs(5), abandoned: Duration::from_secs(120) }
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
    Start { player: PlayerId },
    Play { player: PlayerId, table: Vec<Vec<Tile>> },
    Draw { player: PlayerId },
}

struct Player {
    id: PlayerId,
    token: String,
    name: String,
    connection: Option<Connection>,
}

pub struct Room {
    code: String,
    timing: Timing,
    /// In seat order.
    players: Vec<Player>,
    next_id: PlayerId,
    game: Option<Game>,
    /// When the current player's turn runs out.
    turn_ends: Option<Instant>,
    /// The turn's full deadline, while it's cut short because the player
    /// left; given back if they return in time.
    full_turn_ends: Option<Instant>,
    /// When the room closes if nobody comes back.
    closes: Option<Instant>,
}

/// Starts a room's task; it removes itself from `rooms` when it closes.
pub fn spawn(code: String, timing: Timing, rooms: Arc<Rooms>) -> mpsc::Sender<Command> {
    let (sender, commands) = mpsc::channel(64);
    let room = Room { code, timing, players: Vec::new(), next_id: 0, game: None, turn_ends: None, full_turn_ends: None, closes: None };
    tokio::spawn(room.run(commands, rooms));
    sender
}

impl Room {
    async fn run(mut self, mut commands: mpsc::Receiver<Command>, rooms: Arc<Rooms>) {
        println!("room {} opened", self.code);
        // A room nobody joins closes too.
        self.closes = Some(Instant::now() + self.timing.abandoned);
        loop {
            // select! evaluates every branch's future, even a disabled one.
            let never = Instant::now() + Duration::from_secs(365 * 24 * 3600);
            let (turn_ends, closes) = (self.turn_ends, self.closes);
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => self.handle(command),
                    None => break,
                },
                _ = sleep_until(turn_ends.unwrap_or(never)), if turn_ends.is_some() => self.time_out(),
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
            Command::Start { player } => self.start(player),
            Command::Play { player, table } => self.make_move(player, |game, seat| game.play(seat, table).map(|_| ())),
            Command::Draw { player } => self.make_move(player, |game, seat| game.draw(seat).map(|_| ())),
        }
    }

    fn join(&mut self, name: String, connection: Connection) -> Result<PlayerId, String> {
        if self.game.is_some() {
            return Err("the game in this room has started".into());
        }
        if self.players.len() >= MAX_PLAYERS {
            return Err(format!("the room is full ({MAX_PLAYERS} players)"));
        }
        let id = self.next_id;
        self.next_id += 1;
        let token = format!("{:032x}", rand::random::<u128>());
        let _ = connection.outbox.send(ServerMessage::Joined { code: self.code.clone(), token: token.clone() });
        self.players.push(Player { id, token, name, connection: Some(connection) });
        self.connections_changed();
        Ok(id)
    }

    fn rejoin(&mut self, token: &str, connection: Connection) -> Result<PlayerId, String> {
        let seat = self.players.iter().position(|p| p.token == token).ok_or("no such seat in this room")?;
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
            let soon = Instant::now() + self.timing.absent_turn;
            self.full_turn_ends = self.turn_ends;
            self.turn_ends = self.turn_ends.map(|t| t.min(soon));
            self.broadcast_state();
        }
        self.connections_changed();
    }

    fn start(&mut self, player: PlayerId) {
        let Some(seat) = self.seat(player) else { return };
        let refusal = if self.game.is_some() {
            Some("the game has already started")
        } else if seat != self.host() {
            Some("only the host can start the game")
        } else if self.players.iter().filter(|p| p.connection.is_some()).count() < 2 {
            Some("a game needs at least 2 players")
        } else {
            None
        };
        if let Some(message) = refusal {
            return self.send(seat, ServerMessage::Error { message: message.into() });
        }
        // Those who left before the start don't get a seat.
        self.players.retain(|p| p.connection.is_some());
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
        match apply(game, seat) {
            Ok(()) => {
                self.start_turn();
                self.broadcast_state();
            }
            Err(error) => self.send(seat, ServerMessage::Rejected { message: error.to_string(), error }),
        }
    }

    /// The current player ran out of time: they draw.
    fn time_out(&mut self) {
        let Some(game) = self.game.as_mut() else { return };
        let _ = game.draw(game.current());
        self.start_turn();
        self.broadcast_state();
    }

    /// Sets the timer for the turn now beginning (none once the game is over).
    fn start_turn(&mut self) {
        let Some(game) = &self.game else { return };
        self.full_turn_ends = None;
        self.turn_ends = game.outcome().is_none().then(|| {
            let present = self.players[game.current()].connection.is_some();
            Instant::now() + if present { self.timing.turn } else { self.timing.absent_turn }
        });
    }

    /// Someone connected or left: tell everyone, and close the room once
    /// nobody has been here for a while.
    fn connections_changed(&mut self) {
        if self.players.iter().any(|p| p.connection.is_some()) {
            self.closes = None;
        } else if self.closes.is_none() {
            self.closes = Some(Instant::now() + self.timing.abandoned);
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
        let seats: Vec<Seat> =
            self.players.iter().map(|p| Seat { name: p.name.clone(), connected: p.connection.is_some() }).collect();
        for you in 0..self.players.len() {
            let room = RoomInfo {
                code: self.code.clone(),
                seats: seats.clone(),
                you,
                host: self.host(),
                started: self.game.is_some(),
            };
            self.send(you, ServerMessage::Room { room });
        }
    }

    fn send_state(&self, seat: usize) {
        let Some(game) = &self.game else { return };
        let turn_ms = self.turn_ends.map(|t| t.saturating_duration_since(Instant::now()).as_millis() as u32);
        self.send(seat, ServerMessage::State { view: game.view(seat), turn_ms });
    }

    fn broadcast_state(&self) {
        for seat in 0..self.players.len() {
            self.send_state(seat);
        }
    }
}

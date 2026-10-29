//! What a room says of its own, whatever its game: joining, the room's
//! members and options, the clock, refusals. A connection receives these
//! and its game's messages as `Out`.
//!
//! They serialize as the protocol's messages of the same names (see
//! crates/protocol), the game's part of the options flattened in; the
//! protocol's shared part is to become these types.

use serde::{Deserialize, Serialize};

use crate::engine::GameEngine;

/// A message for a connection: the room's own, or its game's.
pub enum Out<G: GameEngine> {
    Shared(SharedMessage<G::Rules, G::Level>),
    Game(G::Message),
}

impl<G: GameEngine> Clone for Out<G> {
    fn clone(&self) -> Out<G> {
        match self {
            Out::Shared(message) => Out::Shared(message.clone()),
            Out::Game(message) => Out::Game(message.clone()),
        }
    }
}

impl<G: GameEngine> std::fmt::Debug for Out<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Out::Shared(message) => message.fmt(f),
            Out::Game(message) => message.fmt(f),
        }
    }
}

/// The room's own messages, for game rules `R` and bot levels `L`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SharedMessage<R, L> {
    /// First thing on every connection.
    Welcome { protocol: u32 },
    /// This connection has a seat in the room. Keep the token to rejoin.
    Joined { code: String, token: String },
    /// The room's members changed, or the game started.
    Room { room: RoomInfo<R, L> },
    /// The current player was given extra time: their turn now has
    /// `turn_ms` left.
    Clock { by: usize, added_ms: u32, turn_ms: u32 },
    Notice { notice: Notice },
    Refused { refusal: Refusal },
    /// A message the server couldn't read: in English, not for players.
    Error { message: String },
}

/// How a room plays, chosen by whoever opens it: the room's part, and the
/// game's (`game`, rules `R`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomOptions<R, L> {
    /// Seconds in a turn.
    pub turn_seconds: u32,
    /// Whether the others see the current player's draft as it changes.
    pub live_turns: bool,
    /// Each player has a reserve of time for the whole game, used once a
    /// turn's own time runs out.
    pub time_bank: bool,
    /// The bot that takes over the seat of a player who leaves the game.
    pub stand_in: L,
    #[serde(flatten)]
    pub game: R,
}

/// A player in a room, or someone watching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seat<L> {
    pub name: String,
    /// Whether they're here: always, for a bot.
    pub connected: bool,
    /// How well it plays, if this is a computer player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<L>,
    /// Whether they may start the game (and pause one only bots play).
    pub host: bool,
}

/// A room as one of its players, or one of those watching, sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo<R, L> {
    pub code: String,
    /// In turn order once the game has started.
    pub seats: Vec<Seat<L>>,
    pub watchers: Vec<Seat<L>>,
    /// The seat of the player this is sent to; absent for a watcher...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub you: Option<usize>,
    /// ...who is this one of `watchers`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watching: Option<usize>,
    pub started: bool,
    /// Whether the host has paused the game (only bots play it).
    pub paused: bool,
    pub options: RoomOptions<R, L>,
}

/// What the room tells a player. It says what happened, not a sentence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Notice {
    /// Extra time given for a draft they then took back counts against the
    /// player: `so_far_ms` so far, and at `limit_ms` they get no more.
    UnearnedWarning { so_far_ms: u32, limit_ms: u32 },
    /// They took back what they had been given `taken_back_ms` of extra
    /// time for: no more extra time this game.
    NoMoreBonus { taken_back_ms: u32 },
}

/// Why the room, or the server, won't do what a client asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Refusal {
    /// No open room has this code.
    NoSuchRoom { code: String },
    /// The room has closed.
    RoomClosed,
    /// Every seat is taken.
    RoomFull { max: u32 },
    /// As many are watching as may.
    WatchersFull { max: u32 },
    /// The game has started: seats and bots are as they are.
    GameStarted,
    /// A rejoin for a seat this room doesn't have (any more).
    NoSuchSeat,
    /// Only the host may do that.
    OnlyHost,
    /// Removing a seat that isn't a bot's.
    NotABot,
    /// Starting with fewer players than the game needs.
    TooFewPlayers { min: u32 },
    /// Pausing or stepping with no game going on.
    NoGame,
    /// Pausing or stepping a game a person plays in.
    NotBotsOnly,
    /// A move from someone watching.
    Watching,
    /// A move before the game has started.
    NotStarted,
    /// A draft with what the player couldn't use.
    NotYourTiles,
    /// Creating or joining a room while in one.
    AlreadyInRoom,
    /// A request for a room from a connection in none.
    NotInRoom,
}

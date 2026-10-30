//! The messages between the web client and the match server, whatever the
//! game. Each is one WebSocket text frame of JSON, tagged by `type`:
//! `{"type":"start"}`. A game's own messages ride inside as `game`:
//! `{"type":"game","message":{"type":"play","table":[...]}}`, defined by
//! the game's engine crate (`tile_rummy::protocol`, `grid::protocol`).
//!
//! The messages are generic over the game's part of the room options (`R`),
//! its bot levels (`L`) and its messages (`M`). The browser gets them as
//! TypeScript types from each game's WASM build. Fields are never `null`:
//! an absent value is left out, which is what those types expect.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

/// Bumped when a shared message changes in a way an older client can't
/// read; the server says which it speaks in `Welcome`, with the game's own
/// version.
pub const PROTOCOL_VERSION: u32 = 13;

/// The turn lengths a room may choose, in seconds.
pub const TURN_SECONDS: [u32; 5] = [30, 60, 90, 120, 180];

/// How a room plays, chosen by whoever opens it: the room's part, and the
/// game's (`game`). The room's options left out take their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct RoomOptions<R, L> {
    /// Seconds in a turn: one of `TURN_SECONDS`.
    #[serde(default = "a_minute")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub turn_seconds: u32,
    /// Whether the others see the current player's work in progress as it
    /// changes (a draft), as at a real table. Off: only finished turns.
    #[serde(default = "yes")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub live_turns: bool,
    /// Each player has a reserve of time for the whole game, used once a
    /// turn's own time runs out.
    #[serde(default)]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub time_bank: bool,
    /// The bot that takes over the seat of a player who leaves the game.
    pub stand_in: L,
    /// The game's rule set and house rules.
    pub game: R,
}

fn a_minute() -> u32 {
    60
}

fn yes() -> bool {
    true
}

impl<R: Default, L: Default> Default for RoomOptions<R, L> {
    fn default() -> RoomOptions<R, L> {
        RoomOptions { turn_seconds: 60, live_turns: true, time_bank: false, stand_in: L::default(), game: R::default() }
    }
}

impl<R, L> RoomOptions<R, L> {
    /// The turn length kept to `TURN_SECONDS` (the game checks its part).
    pub fn checked_turn(self) -> RoomOptions<R, L> {
        let turn_seconds = if TURN_SECONDS.contains(&self.turn_seconds) { self.turn_seconds } else { 60 };
        RoomOptions { turn_seconds, ..self }
    }
}

/// From a client to the server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ClientMessage<R, L, M> {
    /// Open a new room and join it as its host.
    CreateRoom {
        name: String,
        #[cfg_attr(feature = "wasm", tsify(optional))]
        options: Option<RoomOptions<R, L>>,
    },
    /// Join a room by its code: to play, if it hasn't started, or with
    /// `watch` to watch, at any time.
    JoinRoom {
        code: String,
        name: String,
        #[serde(default)]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        watch: bool,
    },
    /// Take back a seat (or a watcher's place) after a lost connection, with
    /// the token from `Joined`.
    Rejoin { code: String, token: String },
    /// Leave the room for good: before the start, the seat is given up;
    /// during the game, a bot (`RoomOptions::stand_in`) plays it from now
    /// on.
    Leave,
    /// Watch the game instead of playing in it, or take a seat again
    /// (before the start).
    Watch { watching: bool },
    /// Seat a computer player (the host only, before the start).
    AddBot { level: L },
    /// Take a computer player's seat away again (the host only, before the
    /// start).
    RemoveBot { seat: usize },
    /// Deal and play (the host only).
    Start,
    /// Stop or restart the clock and the bots, in a game only bots play
    /// (the host only).
    Pause { paused: bool },
    /// In a paused game only bots play: play the next turn, then pause
    /// again (the host only).
    Step,
    /// A message for the game: a move, a draft, ...
    Game { message: M },
}

/// A player in a room, or someone watching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Seat<L> {
    pub name: String,
    /// Whether they're here: always, for a bot.
    pub connected: bool,
    /// How well it plays, if this is a computer player.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub bot: Option<L>,
    /// Whether they may start the game (and pause one only bots play).
    pub host: bool,
}

/// A room as one of its players, or one of those watching, sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo<R, L> {
    pub code: String,
    /// In turn order once the game has started.
    pub seats: Vec<Seat<L>>,
    /// Those watching, without a seat.
    pub watchers: Vec<Seat<L>>,
    /// The seat of the player this is sent to; absent for a watcher...
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub you: Option<usize>,
    /// ...who is this one of `watchers`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub watching: Option<usize>,
    /// How many may sit at the table: the game's least and most.
    pub min_seats: usize,
    pub max_seats: usize,
    pub started: bool,
    /// Whether the host has paused the game (only bots play it).
    pub paused: bool,
    pub options: RoomOptions<R, L>,
}

/// From the server to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ServerMessage<R, L, M> {
    /// First thing on every connection: the shared protocol's version, and
    /// the game's.
    Welcome { protocol: u32, game: u32 },
    /// This connection has a seat in the room. Keep the token to `Rejoin`.
    Joined { code: String, token: String },
    /// The room's members changed (someone joined, left, came back) or the
    /// game started.
    Room { room: RoomInfo<R, L> },
    /// The current player was given extra time (tile rummy: for tiles):
    /// their turn now has `turn_ms` left.
    Clock { by: usize, added_ms: u32, turn_ms: u32 },
    /// Something this player should know, such as a warning.
    Notice { notice: Notice },
    /// A request the server won't do now (join a full room, start as
    /// someone other than the host, ...).
    Refused { refusal: Refusal },
    /// A message the server couldn't read: a bug in the client, so in
    /// English, not for players.
    Error { message: String },
    /// A message from the game: its state, a rejected move, ...
    Game { message: M },
}

/// What the room tells a player. Like every message, it says what
/// happened, not a sentence: the client words it in the player's language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Notice {
    /// Extra time given for a draft they then took back counts against the
    /// player: `so_far_ms` so far, and at `limit_ms` they get no more.
    UnearnedWarning { so_far_ms: u32, limit_ms: u32 },
    /// They took back what they had been given `taken_back_ms` of extra
    /// time for: no more extra time this game.
    NoMoreBonus { taken_back_ms: u32 },
}

/// Why the server won't do what a client asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
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
    /// A draft with what the player couldn't use (tile rummy: tiles
    /// neither on the table nor on their rack).
    NotYourTiles,
    /// Creating or joining a room while in one.
    AlreadyInRoom,
    /// A request for a room from a connection in none.
    NotInRoom,
}

impl<R: DeserializeOwned, L: DeserializeOwned, M: DeserializeOwned> ClientMessage<R, L, M> {
    pub fn from_json(text: &str) -> Result<ClientMessage<R, L, M>, serde_json::Error> {
        serde_json::from_str(text)
    }
}

impl<R: Serialize, L: Serialize, M: Serialize> ServerMessage<R, L, M> {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("server messages always serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game with a string for its options, numbers for levels, and a
    /// message of its own.
    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "camelCase")]
    enum Say {
        #[default]
        Hello,
        Number { n: u32 },
    }

    type Client = ClientMessage<String, u8, Say>;
    type Server = ServerMessage<String, u8, Say>;

    #[test]
    fn client_messages_as_json() {
        assert_eq!(Client::from_json(r#"{"type":"start"}"#).unwrap(), Client::Start);
        assert_eq!(
            Client::from_json(r#"{"type":"game","message":{"type":"number","n":3}}"#).unwrap(),
            Client::Game { message: Say::Number { n: 3 } }
        );
        assert_eq!(
            Client::from_json(r#"{"type":"createRoom","name":"Ann"}"#).unwrap(),
            Client::CreateRoom { name: "Ann".into(), options: None }
        );
        let ClientMessage::CreateRoom { options, .. } =
            Client::from_json(r#"{"type":"createRoom","name":"Ann","options":{"liveTurns":false,"standIn":0,"game":"x"}}"#).unwrap()
        else {
            panic!()
        };
        assert_eq!(options, Some(RoomOptions { live_turns: false, game: "x".into(), ..RoomOptions::default() }));
        assert_eq!(
            Client::from_json(r#"{"type":"joinRoom","code":"ABCD","name":"Ann"}"#).unwrap(),
            Client::JoinRoom { code: "ABCD".into(), name: "Ann".into(), watch: false }
        );
        assert_eq!(Client::from_json(r#"{"type":"addBot","level":2}"#).unwrap(), Client::AddBot { level: 2 });
        assert_eq!(RoomOptions::<String, u8> { turn_seconds: 45, ..RoomOptions::default() }.checked_turn().turn_seconds, 60);
        assert_eq!(RoomOptions::<String, u8> { turn_seconds: 180, ..RoomOptions::default() }.checked_turn().turn_seconds, 180);
        assert!(Client::from_json(r#"{"type":"cheat"}"#).is_err());
        assert!(Client::from_json(r#"{"type":"game","message":{"type":"cheat"}}"#).is_err());
        assert!(Client::from_json("hello").is_err());
    }

    #[test]
    fn server_messages_as_json() {
        assert_eq!(Server::Welcome { protocol: 13, game: 1 }.to_json(), r#"{"type":"welcome","protocol":13,"game":1}"#);
        let refused = Server::Refused { refusal: Refusal::RoomFull { max: 4 } };
        assert_eq!(refused.to_json(), r#"{"type":"refused","refusal":{"kind":"roomFull","max":4}}"#);
        let notice = Server::Notice { notice: Notice::UnearnedWarning { so_far_ms: 1000, limit_ms: 2000 } };
        assert_eq!(notice.to_json(), r#"{"type":"notice","notice":{"kind":"unearnedWarning","soFarMs":1000,"limitMs":2000}}"#);
        assert_eq!(Server::Game { message: Say::Hello }.to_json(), r#"{"type":"game","message":{"type":"hello"}}"#);
        let seat = Seat { name: "Ann".into(), connected: true, bot: None, host: true };
        let room = RoomInfo {
            code: "ABCD".into(),
            seats: vec![seat],
            watchers: vec![],
            you: Some(0),
            watching: None,
            min_seats: 2,
            max_seats: 4,
            started: false,
            paused: false,
            options: RoomOptions::<String, u8>::default(),
        };
        let json = Server::Room { room }.to_json();
        assert!(!json.contains("null") && !json.contains("watching") && !json.contains("bot"), "{json}");
        assert!(json.ends_with(r#""options":{"turnSeconds":60,"liveTurns":true,"timeBank":false,"standIn":0,"game":""}}}"#), "{json}");
    }
}

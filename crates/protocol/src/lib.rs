//! The messages between the web client and the match server. Each is one
//! WebSocket text frame of JSON, tagged by `type`:
//! `{"type":"play","table":[...]}`.
//!
//! The browser gets these as TypeScript types (`ClientMessage`,
//! `ServerMessage`) from the engine's WASM build. Fields are never `null`:
//! an absent value is left out, which is what those types expect.

use engine::okey101::{MatchView, Okey101Rules, OkeyError};
use engine::{BotLevel, PlayerView, Tile, TurnError};
use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

/// Bumped when a message changes in a way an older client can't read; the
/// server says which it speaks in `Welcome`.
pub const PROTOCOL_VERSION: u32 = 9;

/// How a room plays, chosen by whoever opens it. Options left out take
/// their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(default, rename_all = "camelCase")]
pub struct RoomOptions {
    /// Seconds in a turn: one of `TURN_SECONDS`.
    pub turn_seconds: u32,
    /// Whether the others see the current player's table as they rearrange
    /// it (`Draft`), as at a real table. Off: only finished turns.
    pub live_turns: bool,
    /// Each player has a reserve of time for the whole game, used once a
    /// turn's own time runs out.
    pub time_bank: bool,
    /// Extra time on a turn for tiles put down: given as the table becomes
    /// a valid turn worth more than before (`Clock`).
    pub tile_bonus: bool,
    /// The bot that takes over the seat of a player who leaves the game:
    /// any level but Extra Hard.
    pub stand_in: BotLevel,
    /// The game played: the classic one, or Okey 101 with its settings.
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub rules: RuleSet,
}

/// The rule sets a room can play.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuleSet {
    /// Rearranging a shared table, 30 to open (`engine::Game`).
    #[default]
    Classic,
    /// Draw, meld, discard; 101 to open (`engine::okey101`).
    Okey101(Okey101Rules),
}

impl Default for RoomOptions {
    fn default() -> RoomOptions {
        RoomOptions {
            turn_seconds: 60,
            live_turns: true,
            time_bank: false,
            tile_bonus: false,
            stand_in: BotLevel::Medium,
            rules: RuleSet::Classic,
        }
    }
}

/// The turn lengths a room may choose, in seconds.
pub const TURN_SECONDS: [u32; 5] = [30, 60, 90, 120, 180];

/// The match lengths an Okey 101 room may choose, in hands.
pub const OKEY_HANDS: [u32; 6] = [1, 3, 5, 7, 9, 11];

impl RoomOptions {
    /// The options with anything out of range back at its default. Extra
    /// time for tiles is the classic game's only.
    pub fn checked(self) -> RoomOptions {
        let turn_seconds = if TURN_SECONDS.contains(&self.turn_seconds) { self.turn_seconds } else { 60 };
        let stand_in = if self.stand_in == BotLevel::ExtraHard { BotLevel::Medium } else { self.stand_in };
        let (rules, tile_bonus) = match self.rules {
            RuleSet::Classic => (RuleSet::Classic, self.tile_bonus),
            RuleSet::Okey101(rules) => {
                let hands = if OKEY_HANDS.contains(&rules.hands) { rules.hands } else { Okey101Rules::default().hands };
                (RuleSet::Okey101(Okey101Rules { hands, ..rules }), false)
            }
        };
        RoomOptions { turn_seconds, stand_in, rules, tile_bonus, ..self }
    }
}

/// From a client to the server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ClientMessage {
    /// Open a new room and join it as its host.
    CreateRoom {
        name: String,
        #[serde(default)]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        options: RoomOptions,
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
    AddBot { level: BotLevel },
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
    /// End the turn with the table rearranged to this (see
    /// `engine::check_turn`).
    Play { table: Vec<Vec<Tile>> },
    /// Classic: end the turn by drawing a tile instead. Okey 101: draw
    /// from the pool, and the turn goes on.
    Draw,
    /// Okey 101: end the turn with the table as it now is and this tile
    /// discarded, having taken the previous player's discard instead of
    /// drawing if `take_discard` (see `engine::okey101::Turn`).
    Discard {
        #[serde(default)]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        take_discard: bool,
        table: Vec<Vec<Tile>>,
        discard: Tile,
    },
    /// The table as the current player is rearranging it: played for them
    /// if their time runs out and it's a valid turn, and passed on for the
    /// others to watch in rooms with `live_turns`. Not a move: sets may be
    /// invalid.
    Draft { table: Vec<Vec<Tile>> },
}

/// A player in a room, or someone watching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Seat {
    pub name: String,
    /// Whether they're here: always, for a bot.
    pub connected: bool,
    /// How well it plays, if this is a computer player.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub bot: Option<BotLevel>,
    /// Whether they may start the game (and pause one only bots play).
    pub host: bool,
}

/// A room as one of its players, or one of those watching, sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub code: String,
    /// In turn order once the game has started.
    pub seats: Vec<Seat>,
    /// Those watching, without a seat.
    pub watchers: Vec<Seat>,
    /// The seat of the player this is sent to; absent for a watcher...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub you: Option<usize>,
    /// ...who is this one of `watchers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub watching: Option<usize>,
    pub started: bool,
    /// Whether the host has paused the game (only bots play it).
    pub paused: bool,
    pub options: RoomOptions,
}

/// From the server to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ServerMessage {
    /// First thing on every connection.
    Welcome { protocol: u32 },
    /// This connection has a seat in the room. Keep the token to `Rejoin`.
    Joined { code: String, token: String },
    /// The room's players changed (someone joined, left, came back) or the
    /// game started.
    Room { room: RoomInfo },
    /// The game as this player may see it, after every change (a watcher:
    /// with no rack, and `view.you` absent).
    State {
        view: PlayerView,
        /// Milliseconds left in the current turn; absent once the game is
        /// over.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        turn_ms: Option<u32>,
        /// Milliseconds left in each player's time bank, in seat order (the
        /// current player's as of now): rooms with `time_bank`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        banks: Option<Vec<u32>>,
        /// Every player's rack, in seat order: only from a server started
        /// to show them (`--open-racks`), for judging the bots, and to
        /// watchers of a game only bots play.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        racks: Option<Vec<Vec<Tile>>>,
    },
    /// An Okey 101 match as this player may see it, after every change: as
    /// `State`, for rooms playing Okey 101.
    OkeyState {
        view: MatchView,
        /// Milliseconds left in the current turn; absent between hands and
        /// once the match is over.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        turn_ms: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        banks: Option<Vec<u32>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        racks: Option<Vec<Vec<Tile>>>,
        /// Between hands: milliseconds until the next is dealt.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        next_hand_ms: Option<u32>,
    },
    /// The current player's table so far this turn (see `ClientMessage::Draft`).
    /// Gone with the next `State`.
    Draft { by: usize, table: Vec<Vec<Tile>> },
    /// The current player was given extra time for tiles (`tile_bonus`):
    /// their turn now has `turn_ms` left.
    Clock { by: usize, added_ms: u32, turn_ms: u32 },
    /// Something this player should know, such as a warning.
    Notice { notice: Notice },
    /// This player's move wasn't allowed; if it was their turn, it still is.
    Rejected { error: TurnError },
    /// As `Rejected`, in an Okey 101 room.
    OkeyRejected { error: OkeyError },
    /// A request the server won't do now (join a full room, start as
    /// someone other than the host, ...).
    Refused { refusal: Refusal },
    /// A message the server couldn't read: a bug in the client, so in
    /// English, not for players.
    Error { message: String },
}

/// What the server tells a player. Like every message, it says what
/// happened, not a sentence: the client words it in the player's language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Notice {
    /// The player's time ran out: of the tiles they had put down, `played`
    /// were played (the valid part of their table) and `back` went back to
    /// their rack. None played: they drew a tile.
    TimeRanOut { played: u32, back: u32 },
    /// Extra time given for tiles they then took back counts against the
    /// player: `so_far_ms` so far, and at `limit_ms` they get no more.
    UnearnedWarning { so_far_ms: u32, limit_ms: u32 },
    /// They took back tiles they had been given `taken_back_ms` of extra
    /// time for: no more extra time for tiles this game.
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
    /// Starting with fewer players than a game needs.
    TooFewPlayers { min: u32 },
    /// Pausing or stepping with no game going on.
    NoGame,
    /// Pausing or stepping a game a person plays in.
    NotBotsOnly,
    /// A move from someone watching.
    Watching,
    /// A move before the game has started.
    NotStarted,
    /// A draft with tiles neither on the table nor on the player's rack.
    NotYourTiles,
    /// Creating or joining a room while in one.
    AlreadyInRoom,
    /// A request for a room from a connection in none.
    NotInRoom,
}

impl ClientMessage {
    pub fn from_json(text: &str) -> Result<ClientMessage, serde_json::Error> {
        serde_json::from_str(text)
    }
}

impl ServerMessage {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("server messages always serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{Color, Game, Rules};

    #[test]
    fn client_messages_as_json() {
        let play = r#"{"type":"play","table":[[{"kind":"number","color":"red","value":3},{"kind":"joker"}]]}"#;
        assert_eq!(
            ClientMessage::from_json(play).unwrap(),
            ClientMessage::Play { table: vec![vec![Tile::Number { color: Color::Red, value: 3 }, Tile::Joker]] }
        );
        assert_eq!(ClientMessage::from_json(r#"{"type":"draw"}"#).unwrap(), ClientMessage::Draw);
        assert_eq!(
            ClientMessage::from_json(r#"{"type":"createRoom","name":"Ann"}"#).unwrap(),
            ClientMessage::CreateRoom { name: "Ann".into(), options: RoomOptions::default() },
            "live turns unless asked otherwise"
        );
        assert_eq!(
            ClientMessage::from_json(r#"{"type":"createRoom","name":"Ann","options":{"liveTurns":false}}"#).unwrap(),
            ClientMessage::CreateRoom { name: "Ann".into(), options: RoomOptions { live_turns: false, ..RoomOptions::default() } }
        );
        assert_eq!(
            ClientMessage::from_json(r#"{"type":"joinRoom","code":"ABCD","name":"Ann"}"#).unwrap(),
            ClientMessage::JoinRoom { code: "ABCD".into(), name: "Ann".into(), watch: false }
        );
        assert_eq!(
            ClientMessage::from_json(r#"{"type":"pause","paused":true}"#).unwrap(),
            ClientMessage::Pause { paused: true }
        );
        assert_eq!(
            ClientMessage::from_json(r#"{"type":"addBot","level":"hard"}"#).unwrap(),
            ClientMessage::AddBot { level: BotLevel::Hard }
        );
        assert_eq!(RoomOptions { turn_seconds: 45, ..RoomOptions::default() }.checked().turn_seconds, 60);
        assert_eq!(RoomOptions { turn_seconds: 180, ..RoomOptions::default() }.checked().turn_seconds, 180);
        assert_eq!(RoomOptions { stand_in: BotLevel::ExtraHard, ..RoomOptions::default() }.checked().stand_in, BotLevel::Medium);
        assert_eq!(RoomOptions { stand_in: BotLevel::Hard, ..RoomOptions::default() }.checked().stand_in, BotLevel::Hard);
        assert_eq!(ClientMessage::from_json(r#"{"type":"leave"}"#).unwrap(), ClientMessage::Leave);
        let okey = r#"{"type":"createRoom","name":"Ann","options":{"rules":{"kind":"okey101","hands":3,"progressive":true,"penalties":false}}}"#;
        let ClientMessage::CreateRoom { options, .. } = ClientMessage::from_json(okey).unwrap() else { panic!() };
        assert_eq!(options.rules, RuleSet::Okey101(Okey101Rules { hands: 3, progressive: true, penalties: false }));
        let odd = RoomOptions { tile_bonus: true, rules: RuleSet::Okey101(Okey101Rules { hands: 4, ..Okey101Rules::default() }), ..RoomOptions::default() };
        assert_eq!(odd.clone().checked().rules, RuleSet::Okey101(Okey101Rules::default()), "4 hands isn't offered");
        assert!(!odd.checked().tile_bonus, "classic only");
        let discard = r#"{"type":"discard","table":[],"discard":{"kind":"joker"}}"#;
        assert_eq!(
            ClientMessage::from_json(discard).unwrap(),
            ClientMessage::Discard { take_discard: false, table: vec![], discard: Tile::Joker }
        );
        assert!(ClientMessage::from_json(r#"{"type":"cheat"}"#).is_err());
        assert!(ClientMessage::from_json("hello").is_err());
    }

    #[test]
    fn server_messages_as_json() {
        assert_eq!(ServerMessage::Welcome { protocol: 1 }.to_json(), r#"{"type":"welcome","protocol":1}"#);
        let rejected = ServerMessage::Rejected { error: TurnError::InvalidSet { index: 2 } };
        assert_eq!(rejected.to_json(), r#"{"type":"rejected","error":{"kind":"invalidSet","index":2}}"#);
        let refused = ServerMessage::Refused { refusal: Refusal::RoomFull { max: 4 } };
        assert_eq!(refused.to_json(), r#"{"type":"refused","refusal":{"kind":"roomFull","max":4}}"#);
        let notice = ServerMessage::Notice { notice: Notice::UnearnedWarning { so_far_ms: 1000, limit_ms: 2000 } };
        assert_eq!(notice.to_json(), r#"{"type":"notice","notice":{"kind":"unearnedWarning","soFarMs":1000,"limitMs":2000}}"#);
        let view = Game::new(2, Rules::default(), 1).view(0);
        let state = ServerMessage::State { view: view.clone(), turn_ms: Some(1500), banks: None, racks: None }.to_json();
        assert!(state.starts_with(r#"{"type":"state","view":{"you":0,"rules":{"#), "{state}");
        assert!(state.ends_with(r#""turnMs":1500}"#), "{state}");
        let over = ServerMessage::State { view, turn_ms: None, banks: None, racks: None }.to_json();
        assert!(!over.contains("turnMs") && !over.contains("null"), "{over}");
    }
}

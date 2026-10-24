//! The messages between the web client and the match server. Each is one
//! WebSocket text frame of JSON, tagged by `type`:
//! `{"type":"play","table":[...]}`.
//!
//! The browser gets these as TypeScript types (`ClientMessage`,
//! `ServerMessage`) from the engine's WASM build. Fields are never `null`:
//! an absent value is left out, which is what those types expect.

use engine::{PlayerView, Tile, TurnError};
use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

/// Bumped when a message changes in a way an older client can't read; the
/// server says which it speaks in `Welcome`.
pub const PROTOCOL_VERSION: u32 = 3;

/// How a room plays, chosen by whoever opens it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct RoomOptions {
    /// Whether the others see the current player's table as they rearrange
    /// it (`Draft`), as at a real table. Off: only finished turns.
    pub live_turns: bool,
}

impl Default for RoomOptions {
    fn default() -> RoomOptions {
        RoomOptions { live_turns: true }
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
    /// Join a room that hasn't started, by its code.
    JoinRoom { code: String, name: String },
    /// Take back a seat after a lost connection, with the token from
    /// `Joined`.
    Rejoin { code: String, token: String },
    /// Deal and play (the host only).
    Start,
    /// End the turn with the table rearranged to this (see
    /// `engine::check_turn`).
    Play { table: Vec<Vec<Tile>> },
    /// End the turn by drawing a tile instead.
    Draw,
    /// The table as the current player is rearranging it, for the others
    /// to watch (rooms with `live_turns`). Not a move: sets may be invalid.
    Draft { table: Vec<Vec<Tile>> },
}

/// A player in a room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Seat {
    pub name: String,
    pub connected: bool,
}

/// A room as one of its players sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub code: String,
    /// In turn order once the game has started.
    pub seats: Vec<Seat>,
    /// The seat of the player this is sent to.
    pub you: usize,
    /// The seat that may start the game.
    pub host: usize,
    pub started: bool,
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
    /// The game as this player may see it, after every change.
    State {
        view: PlayerView,
        /// Milliseconds left in the current turn; absent once the game is
        /// over.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        turn_ms: Option<u32>,
    },
    /// The current player's table so far this turn (see `ClientMessage::Draft`).
    /// Gone with the next `State`.
    Draft { by: usize, table: Vec<Vec<Tile>> },
    /// This player's move wasn't allowed; if it was their turn, it still is.
    Rejected { error: TurnError, message: String },
    /// A message the server couldn't act on: malformed, or not possible now.
    Error { message: String },
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
            ClientMessage::CreateRoom { name: "Ann".into(), options: RoomOptions { live_turns: true } },
            "live turns unless asked otherwise"
        );
        assert_eq!(
            ClientMessage::from_json(r#"{"type":"createRoom","name":"Ann","options":{"liveTurns":false}}"#).unwrap(),
            ClientMessage::CreateRoom { name: "Ann".into(), options: RoomOptions { live_turns: false } }
        );
        assert_eq!(
            ClientMessage::from_json(r#"{"type":"joinRoom","code":"ABCD","name":"Ann"}"#).unwrap(),
            ClientMessage::JoinRoom { code: "ABCD".into(), name: "Ann".into() }
        );
        assert!(ClientMessage::from_json(r#"{"type":"cheat"}"#).is_err());
        assert!(ClientMessage::from_json("hello").is_err());
    }

    #[test]
    fn server_messages_as_json() {
        assert_eq!(ServerMessage::Welcome { protocol: 1 }.to_json(), r#"{"type":"welcome","protocol":1}"#);
        let rejected = ServerMessage::Rejected { error: TurnError::InvalidSet { index: 2 }, message: "set 3".into() };
        assert_eq!(
            rejected.to_json(),
            r#"{"type":"rejected","error":{"kind":"invalidSet","index":2},"message":"set 3"}"#
        );
        let view = Game::new(2, Rules::default(), 1).view(0);
        let state = ServerMessage::State { view: view.clone(), turn_ms: Some(1500) }.to_json();
        assert!(state.starts_with(r#"{"type":"state","view":{"you":0,"rules":{"#), "{state}");
        assert!(state.ends_with(r#""turnMs":1500}"#), "{state}");
        let over = ServerMessage::State { view, turn_ms: None }.to_json();
        assert!(!over.contains("turnMs") && !over.contains("null"), "{over}");
    }
}

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
pub const PROTOCOL_VERSION: u32 = 1;

/// From a client to the server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    /// End the turn with the table rearranged to this (see
    /// `engine::check_turn`).
    Play { table: Vec<Vec<Tile>> },
    /// End the turn by drawing a tile instead.
    Draw,
}

/// From the server to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage {
    /// First thing on every connection.
    Welcome { protocol: u32 },
    /// The game as this player may see it, after every change.
    State { view: PlayerView },
    /// This player's play wasn't allowed; it's still their turn.
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
        let state = ServerMessage::State { view: Game::new(2, Rules::default(), 1).view(0) }.to_json();
        assert!(state.starts_with(r#"{"type":"state","view":{"you":0,"rack":["#), "{state}");
        assert!(!state.contains("null"), "{state}");
    }
}

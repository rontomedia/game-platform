//! Grid placement's part of the messages between the client and the match
//! server (crates/protocol has the shared part), inside them as `game`.

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::game::{Cell, GridError, GridView};

/// Bumped when grid placement's messages change in a way an older client
/// can't read (`Welcome { game }`).
pub const GRID_PROTOCOL: u32 = 1;

/// From a player to the game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GridRequest {
    /// Put the player's mark on this cell.
    Place { cell: Cell },
}

/// From the game to a player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GridMessage {
    /// The game as this player (or a watcher) sees it, after every change.
    State {
        view: GridView,
        /// Milliseconds left in the current turn; absent once it's over.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        turn_ms: Option<u32>,
        /// Each player's time bank, in seat order: rooms with `timeBank`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        banks: Option<Vec<u32>>,
    },
    /// This player's mark can't go there; if it was their turn, it still is.
    Rejected { error: GridError },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_json() {
        let place = r#"{"type":"place","cell":{"row":1,"col":2}}"#;
        assert_eq!(serde_json::from_str::<GridRequest>(place).unwrap(), GridRequest::Place { cell: Cell { row: 1, col: 2 } });
        let rejected = GridMessage::Rejected { error: GridError::Occupied };
        assert_eq!(serde_json::to_string(&rejected).unwrap(), r#"{"type":"rejected","error":{"kind":"occupied"}}"#);
    }
}

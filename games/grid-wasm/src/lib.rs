//! The grid engine's API for the browser, as games/tile-rummy-wasm's:
//! engine types cross as `Ts<T>` and are converted inside each function,
//! so bad input from JS is an ordinary error instead of leaking WASM
//! memory.

use grid::{BotLevel, Cell, GridError, GridRules, GridView};
use serde::{Deserialize, Serialize};
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

/// The shared protocol's version this client speaks, to compare with the
/// server's `welcome` (and the protocol's TypeScript types with it).
#[wasm_bindgen(js_name = protocolVersion)]
pub fn protocol_version() -> u32 {
    protocol::PROTOCOL_VERSION
}

/// The version of grid placement's own messages (`welcome`'s `game`).
#[wasm_bindgen(js_name = gameProtocolVersion)]
pub fn game_protocol_version() -> u32 {
    grid::protocol::GRID_PROTOCOL
}

/// `grid::check_place`'s answer.
#[derive(Serialize, Deserialize, Tsify)]
#[serde(tag = "verdict", rename_all = "lowercase")]
pub enum PlaceVerdict {
    Allowed,
    Rejected { error: GridError },
}

/// Checks a mark from the player's view, for instant feedback; the server
/// checks it again.
#[wasm_bindgen(js_name = checkPlace)]
pub fn check_place(view: Ts<GridView>, cell: Ts<Cell>) -> Result<Ts<PlaceVerdict>, JsError> {
    let verdict = match grid::check_place(&view.to_rust()?, cell.to_rust()?) {
        Ok(()) => PlaceVerdict::Allowed,
        Err(error) => PlaceVerdict::Rejected { error },
    };
    Ok(Ts::from_rust(&verdict)?)
}

/// See `grid::bot_move`: a bot's mark for the player whose turn it is, for
/// playing without the server. The game must not be over.
#[wasm_bindgen(js_name = botMove)]
pub fn bot_move(view: Ts<GridView>, level: Ts<BotLevel>) -> Result<Ts<Cell>, JsError> {
    let view = view.to_rust()?;
    if view.outcome.is_some() {
        return Err(JsError::new("the game is over"));
    }
    Ok(Ts::from_rust(&grid::bot_move(&view, level.to_rust()?))?)
}

/// Reads a message from the server, checked against its type: for tests,
/// and so the protocol's TypeScript types are in this build's .d.ts.
#[wasm_bindgen(js_name = readServerMessage, unchecked_return_type = "ServerMessage<GridRules, BotLevel, GridMessage>")]
pub fn read_server_message(text: &str) -> Result<JsValue, JsError> {
    use grid::protocol::GridMessage;
    let message: protocol::ServerMessage<GridRules, BotLevel, GridMessage> = serde_json::from_str(text)?;
    Ok(Ts::from_rust(&message)?.into())
}

/// As `readServerMessage`, for what a client sends.
#[wasm_bindgen(js_name = readClientMessage, unchecked_return_type = "ClientMessage<GridRules, BotLevel, GridRequest>")]
pub fn read_client_message(text: &str) -> Result<JsValue, JsError> {
    use grid::protocol::GridRequest;
    let message: protocol::ClientMessage<GridRules, BotLevel, GridRequest> = serde_json::from_str(text)?;
    Ok(Ts::from_rust(&message)?.into())
}

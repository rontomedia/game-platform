//! The engine's API for the browser. wasm-pack generates the JS glue and
//! TypeScript definitions from it (see web/package.json).
//!
//! Engine types cross as `Ts<T>` and are converted inside each function, so
//! bad input from JS is an ordinary error instead of leaking WASM memory.

use engine::{Rules, Tile, TurnError};
use serde::{Deserialize, Serialize};
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

/// The protocol version this client speaks, to compare with the server's
/// `welcome`. (Using the protocol crate also brings its TypeScript types,
/// `ClientMessage` and `ServerMessage`, into this build's .d.ts.)
#[wasm_bindgen(js_name = protocolVersion)]
pub fn protocol_version() -> u32 {
    protocol::PROTOCOL_VERSION
}

/// See `engine::classify`. `undefined` when the tiles aren't a valid set.
#[wasm_bindgen(js_name = classifySet)]
pub fn classify_set(tiles: Vec<Ts<Tile>>) -> Result<Option<Ts<engine::SetKind>>, JsError> {
    let tiles = tiles.into_iter().map(|tile| tile.to_rust()).collect::<Result<Vec<_>, _>>()?;
    Ok(engine::classify(&tiles).map(|kind| Ts::from_rust(&kind)).transpose()?)
}

/// See `engine::points`. `undefined` when the tiles aren't a valid set.
#[wasm_bindgen(js_name = setPoints)]
pub fn set_points(tiles: Vec<Ts<Tile>>) -> Result<Option<u32>, JsError> {
    let tiles = tiles.into_iter().map(|tile| tile.to_rust()).collect::<Result<Vec<_>, _>>()?;
    Ok(engine::points(&tiles))
}

/// A turn to check, as `engine::check_turn` takes it.
#[derive(Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct TurnInput {
    pub before: Vec<Vec<Tile>>,
    pub after: Vec<Vec<Tile>>,
    pub rack: Vec<Tile>,
    pub melded: bool,
    pub rules: Rules,
}

/// `engine::check_turn`'s answer: the tiles played, or why the turn isn't
/// allowed (worded for the player by the client, in their language).
#[derive(Serialize, Deserialize, Tsify)]
#[serde(tag = "verdict", rename_all = "lowercase")]
pub enum TurnVerdict {
    Allowed { played: Vec<Tile> },
    Rejected { error: TurnError },
}

/// Checks a turn in the browser, for instant feedback; the server checks
/// it again.
#[wasm_bindgen(js_name = checkTurn)]
pub fn check_turn(input: Ts<TurnInput>) -> Result<Ts<TurnVerdict>, JsError> {
    let input = input.to_rust()?;
    let verdict = match engine::check_turn(&input.before, &input.after, &input.rack, input.melded, &input.rules) {
        Ok(played) => TurnVerdict::Allowed { played },
        Err(error) => TurnVerdict::Rejected { error },
    };
    Ok(Ts::from_rust(&verdict)?)
}

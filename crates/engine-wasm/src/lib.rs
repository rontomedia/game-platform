//! The engine's API for the browser. wasm-pack generates the JS glue and
//! TypeScript definitions from it (see web/package.json).
//!
//! Engine types cross as `Ts<T>` and are converted inside each function, so
//! bad input from JS is an ordinary error instead of leaking WASM memory.

use engine::Tile;
use tsify::Ts;
use wasm_bindgen::prelude::*;

/// See `engine::classify`. `undefined` when the tiles aren't a valid set.
#[wasm_bindgen(js_name = classifySet)]
pub fn classify_set(tiles: Vec<Ts<Tile>>) -> Result<Option<Ts<engine::SetKind>>, JsError> {
    let tiles = tiles.into_iter().map(|tile| tile.to_rust()).collect::<Result<Vec<_>, _>>()?;
    Ok(engine::classify(&tiles).map(|kind| Ts::from_rust(&kind)).transpose()?)
}

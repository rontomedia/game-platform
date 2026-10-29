//! The grid engine's API for the browser, as games/tile-rummy-wasm's:
//! engine types cross as `Ts<T>` and are converted inside each function,
//! so bad input from JS is an ordinary error instead of leaking WASM
//! memory.

use grid::{BotLevel, Cell, GridError, GridRules, GridView, Outcome, Square};
use serde::{Deserialize, Serialize};
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

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

/// See `grid::outcome_of`: how a grid as it stands ends the game, if it
/// does. `undefined` while it goes on.
#[wasm_bindgen]
pub fn outcome(rules: Ts<GridRules>, board: Vec<Ts<Square>>) -> Result<Option<Ts<Outcome>>, JsError> {
    let board = board.into_iter().map(|square| square.to_rust()).collect::<Result<Vec<_>, _>>()?;
    let rules = rules.to_rust()?;
    if board.len() != rules.cells() {
        return Err(JsError::new("the board doesn't fit the rules' grid"));
    }
    Ok(grid::outcome_of(&rules, &board).map(|outcome| Ts::from_rust(&outcome)).transpose()?)
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

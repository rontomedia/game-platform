//! The game rules: tiles, sets, checking a turn, and a whole game from deal
//! to scores. Pure: no I/O, clocks or randomness from outside (a game is
//! shuffled from a seed), so the server, the bots and the browser all get
//! the same answers.

mod game;
mod rng;
mod set;
mod tile;
mod turn;

pub use game::{Game, Outcome, PlayerView, Rules};
pub use set::{classify, points, SetKind};
pub use tile::{full_set, Color, Tile, MAX_VALUE};
pub use turn::{check_turn, within_reach, TurnError};

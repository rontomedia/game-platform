//! The game rules: tiles, sets, checking a turn, and a whole game from deal
//! to scores. Pure: no I/O, clocks or randomness from outside (a game is
//! shuffled from a seed), so the server, the bots and the browser all get
//! the same answers.

mod bot;
mod game;
pub mod okey101;
pub mod plain_okey;
pub mod protocol;
mod rng;
mod salvage;
mod set;
mod solve;
mod tile;
mod turn;

pub use bot::{bot_move, bot_move_with, BotLevel, Lookahead, Move};
pub use game::{Game, Outcome, PlayerView, Rules};
pub use salvage::salvage;
pub use set::{classify, points, SetKind};
pub use tile::{full_set, Color, Tile, MAX_VALUE};
pub use turn::{check_turn, within_reach, TurnError};

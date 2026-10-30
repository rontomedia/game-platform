//! Grid placement: two players take turns putting their mark (X, then O)
//! on a free cell of a grid, and the first with `line` marks in a row
//! (across, down or diagonally; more count too) wins. A full grid with no
//! line is a draw. Tic-Tac-Toe and Gomoku are its named rule sets, the
//! grid's size and the line's length house rules.
//!
//! Pure, as `tile_rummy`: no I/O, clock or randomness from outside, so the
//! server, the bots and the browser get the same answers.

mod board;
mod bot;
mod game;
mod notakto;
pub mod protocol;

pub use bot::{bot_move, bot_move_with, BotLevel};
pub use game::{check_place, Cell, Game, GridError, GridRules, GridView, Line, Outcome, RuleSet, Square, MAX_BOARDS};

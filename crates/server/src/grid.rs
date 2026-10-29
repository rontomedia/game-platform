//! Grid placement (games/grid) as a `GameEngine`: a newtype, as the trait
//! and `grid::Game` both come from other crates. No drafts, asides or
//! extra time; when time runs out an Easy bot's mark is put down.
//!
//! Not served yet: no route and no protocol messages until the grid board
//! (migration step 4); `grid_tests.rs` runs it through the room's commands.

use std::ops::RangeInclusive;
use std::time::Duration;

use grid::{bot_move, bot_move_with, BotLevel, Cell, Game, GridError, GridRules, GridView, Square};
use platform::engine::{ClockInfo, GameEngine, TimedOut};
use serde::Serialize;

#[derive(Clone)]
pub struct GridMatch(pub Game);

/// What a grid room sends a player for the game.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GridMessage {
    /// The game as this player (or a watcher) sees it, after every change.
    State {
        view: GridView,
        #[serde(skip_serializing_if = "Option::is_none")]
        turn_ms: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        banks: Option<Vec<u32>>,
    },
    /// This player's mark can't go there; if it was their turn, it still is.
    Rejected { error: GridError },
}

impl GameEngine for GridMatch {
    type Rules = GridRules;
    type Move = Cell;
    type Level = BotLevel;
    type Config = ();
    type Message = GridMessage;
    type Draft = ();
    type Aside = ();

    /// Seat 0 moves first; the seed isn't needed.
    fn new(rules: &GridRules, _players: usize, _seed: u64) -> GridMatch {
        GridMatch(Game::new(*rules))
    }

    fn seats(_rules: &GridRules) -> RangeInclusive<usize> {
        2..=2
    }

    fn players(&self) -> usize {
        self.0.players()
    }

    fn current(&self) -> usize {
        self.0.current()
    }

    fn in_turn(&self) -> bool {
        self.0.outcome().is_none()
    }

    fn over(&self) -> bool {
        self.0.outcome().is_some()
    }

    fn apply(&mut self, seat: usize, cell: Cell) -> Result<(), GridMessage> {
        self.0.place(seat, cell).map_err(|error| GridMessage::Rejected { error })
    }

    /// An Easy bot's mark.
    fn time_out(&mut self, _config: &(), _draft: Option<()>) -> TimedOut<GridMessage> {
        let seat = self.0.current();
        let cell = bot_move(&self.0.view(seat), BotLevel::Easy);
        if let Err(e) = self.0.place(seat, cell) {
            eprintln!("a mark put down when time ran out: {e}");
        }
        TimedOut { kept: Duration::ZERO, notice: None }
    }

    fn bot_move(&self, level: BotLevel, _config: &(), spent: &dyn Fn() -> f64) -> Cell {
        bot_move_with(&self.0.view(self.0.current()), level, spent)
    }

    /// The first free cell.
    fn bot_fallback(&mut self) {
        let seat = self.0.current();
        let rules = *self.0.rules();
        if let Some(free) = self.0.board().iter().position(|&s| s == Square::Empty) {
            let _ = self.0.place(seat, rules.cell(free));
        }
    }

    /// Nothing is hidden, so `open` changes nothing.
    fn state(&self, seat: Option<usize>, clock: &ClockInfo, _open: bool) -> GridMessage {
        let view = seat.map_or_else(|| self.0.watcher_view(), |seat| self.0.view(seat));
        GridMessage::State { view, turn_ms: clock.turn_ms, banks: clock.banks.clone() }
    }

    fn placings(&self) -> Option<Vec<u32>> {
        self.0.placings()
    }
}

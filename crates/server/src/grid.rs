//! Grid placement (games/grid) as a `GameEngine`: a newtype, as the trait
//! and `grid::Game` both come from other crates. No drafts, asides or
//! extra time; when time runs out an Easy bot's mark is put down.
//!
//! Its messages are `grid::protocol`'s; `grid_tests.rs` also runs it
//! through the room's commands.

use std::ops::RangeInclusive;
use std::time::Duration;

use grid::protocol::{GridMessage, GridRequest, GRID_PROTOCOL};
use grid::{bot_move, bot_move_with, BotLevel, Cell, Game, GridRules, Square};
use platform::engine::{ClockInfo, GameEngine, Request, TimedOut};

#[derive(Clone)]
pub struct GridMatch(pub Game);

impl GameEngine for GridMatch {
    type Rules = GridRules;
    type Request = GridRequest;
    type Move = Cell;
    type Level = BotLevel;
    type Config = ();
    type Message = GridMessage;
    type Draft = ();
    type Aside = ();

    const PROTOCOL: u32 = GRID_PROTOCOL;

    fn request(request: GridRequest) -> Request<GridMatch> {
        match request {
            GridRequest::Place { cell } => Request::Move(cell),
        }
    }

    fn checked(rules: GridRules) -> GridRules {
        rules.checked()
    }

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

    /// The first cell that may be marked.
    fn bot_fallback(&mut self) {
        let seat = self.0.current();
        let rules = *self.0.rules();
        let free: Vec<usize> = self.0.board().iter().enumerate().filter(|(_, &s)| s == Square::Empty).map(|(i, _)| i).collect();
        for cell in free {
            if self.0.place(seat, rules.cell(cell)).is_ok() {
                break;
            }
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

//! The rules, and a whole game from the empty grid to its outcome.

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

/// The named rule sets: each a grid and a line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub enum RuleSet {
    /// 3 × 3, three in a row.
    #[default]
    TicTacToe,
    /// 15 × 15, five in a row, or more ("freestyle").
    Gomoku,
    /// Three 3 × 3 boards side by side; both players mark X, and a line
    /// finishes its board: whoever finishes the last one loses.
    Notakto,
}

/// The shortest and longest side a grid may have.
pub const MIN_SIDE: u8 = 3;
pub const MAX_SIDE: u8 = 19;

/// The most boards Notakto is played on.
pub const MAX_BOARDS: u8 = 4;

/// How a game is played: a named rule set, its grid and line possibly
/// changed as house rules (see `checked`).
///
/// Where there are several boards (Notakto), cells are counted across all
/// of them as one grid (`columns` by `rows`), boards side by side, and a
/// line is only ever within one board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct GridRules {
    pub rule_set: RuleSet,
    /// A board's columns and rows.
    pub width: u8,
    pub height: u8,
    /// Marks in a row that win (a longer row wins too), or in Notakto
    /// finish a board.
    pub line: u8,
    /// How many boards: 1, or for Notakto 1 to `MAX_BOARDS`.
    pub boards: u8,
}

impl Default for GridRules {
    fn default() -> GridRules {
        GridRules::named(RuleSet::TicTacToe)
    }
}

impl GridRules {
    /// A rule set as it's played without house rules.
    pub fn named(rule_set: RuleSet) -> GridRules {
        match rule_set {
            RuleSet::TicTacToe => GridRules { rule_set, width: 3, height: 3, line: 3, boards: 1 },
            RuleSet::Gomoku => GridRules { rule_set, width: 15, height: 15, line: 5, boards: 1 },
            RuleSet::Notakto => GridRules { rule_set, width: 3, height: 3, line: 3, boards: 3 },
        }
    }

    /// The rules with anything out of range brought into it: sides of
    /// `MIN_SIDE` to `MAX_SIDE`, a line of 3 up to the shorter side, one
    /// board; Notakto's boards are 3 × 3, 1 to `MAX_BOARDS` of them.
    pub fn checked(self) -> GridRules {
        if self.rule_set == RuleSet::Notakto {
            return GridRules { boards: self.boards.clamp(1, MAX_BOARDS), ..GridRules::named(RuleSet::Notakto) };
        }
        let width = self.width.clamp(MIN_SIDE, MAX_SIDE);
        let height = self.height.clamp(MIN_SIDE, MAX_SIDE);
        let line = self.line.clamp(3, width.min(height));
        GridRules { width, height, line, boards: 1, ..self }
    }

    /// The grid's columns, all boards together.
    pub fn columns(&self) -> u8 {
        self.width * self.boards
    }

    pub fn rows(&self) -> u8 {
        self.height
    }

    pub fn cells(&self) -> usize {
        self.columns() as usize * self.rows() as usize
    }

    /// Where `cell` is in a grid's row-major list of squares, if it's on
    /// the grid.
    pub fn index(&self, cell: Cell) -> Option<usize> {
        (cell.row < self.rows() && cell.col < self.columns()).then(|| cell.row as usize * self.columns() as usize + cell.col as usize)
    }

    pub fn cell(&self, index: usize) -> Cell {
        let columns = self.columns() as usize;
        Cell { row: (index / columns) as u8, col: (index % columns) as u8 }
    }

    /// The board `cell` is on, counted from the left.
    pub fn board_of(&self, cell: Cell) -> usize {
        (cell.col / self.width) as usize
    }

    /// The mark `seat` puts down: their own, or in Notakto X for both.
    pub fn mark(&self, seat: usize) -> Square {
        match self.rule_set {
            RuleSet::Notakto => Square::X,
            _ => Square::of(seat),
        }
    }
}

/// A cell of the grid, counted from the top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct Cell {
    pub row: u8,
    pub col: u8,
}

/// What a cell holds. Seat 0 plays X and moves first, seat 1 plays O.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum Square {
    #[default]
    Empty,
    X,
    O,
}

impl Square {
    /// The mark of the player in `seat`.
    pub fn of(seat: usize) -> Square {
        if seat == 0 {
            Square::X
        } else {
            Square::O
        }
    }

    /// The seat whose mark this is.
    pub fn seat(self) -> Option<usize> {
        match self {
            Square::Empty => None,
            Square::X => Some(0),
            Square::O => Some(1),
        }
    }
}

/// Why a mark can't go there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GridError {
    NotYourTurn,
    /// The cell already has a mark.
    Occupied,
    OffBoard,
    /// The game is over.
    Over,
    /// That board is finished (Notakto): it has a line.
    Closed,
}

impl std::fmt::Display for GridError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let text = match self {
            GridError::NotYourTurn => "not your turn",
            GridError::Occupied => "that cell is taken",
            GridError::OffBoard => "not a cell of the grid",
            GridError::Over => "the game is over",
            GridError::Closed => "that board is finished",
        };
        f.write_str(text)
    }
}

/// A line made in the game, and who made it: in Notakto, each one
/// finishes its board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
pub struct Line {
    pub seat: usize,
    /// End to end.
    pub cells: Vec<Cell>,
}

/// How a game ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// The seat that won; absent for a draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub winner: Option<usize>,
    /// The line that ended the game, end to end (all of it, if it's
    /// longer than needed): the winning line, or in Notakto the loser's
    /// last; empty for a draw.
    pub line: Vec<Cell>,
}

/// What a player, or someone watching, sees: everything, as nothing is
/// hidden.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct GridView {
    /// This player's seat; absent for someone watching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub you: Option<usize>,
    pub rules: GridRules,
    /// Row by row, from the top left.
    pub board: Vec<Square>,
    /// Whose turn it is (or was, once the game is over).
    pub current: usize,
    /// The marks put down so far, in order.
    pub moves: Vec<Cell>,
    /// The lines made so far, in order (Notakto's finished boards).
    pub lines: Vec<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub outcome: Option<Outcome>,
}

#[derive(Clone, Debug)]
pub struct Game {
    rules: GridRules,
    board: Vec<Square>,
    current: usize,
    moves: Vec<Cell>,
    lines: Vec<Line>,
    outcome: Option<Outcome>,
}

/// The directions a line can run in: across, down and both diagonals.
const DIRECTIONS: [(i32, i32); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];

impl Game {
    /// A game for two on an empty grid, house rules brought into range.
    pub fn new(rules: GridRules) -> Game {
        let rules = rules.checked();
        Game { rules, board: vec![Square::Empty; rules.cells()], current: 0, moves: Vec::new(), lines: Vec::new(), outcome: None }
    }

    pub fn rules(&self) -> &GridRules {
        &self.rules
    }

    pub fn players(&self) -> usize {
        2
    }

    /// Whose turn it is, or was once the game is over.
    pub fn current(&self) -> usize {
        self.current
    }

    pub fn board(&self) -> &[Square] {
        &self.board
    }

    pub fn moves(&self) -> &[Cell] {
        &self.moves
    }

    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// `seat` puts their mark on `cell`; a refused move changes nothing.
    pub fn place(&mut self, seat: usize, cell: Cell) -> Result<(), GridError> {
        let index = check(&self.rules, &self.board, &self.lines, self.current, self.outcome.is_some(), Some(seat), cell)?;
        self.board[index] = self.rules.mark(seat);
        self.moves.push(cell);
        let line = line_through(&self.rules, &self.board, cell);
        if self.rules.rule_set == RuleSet::Notakto {
            if !line.is_empty() {
                self.lines.push(Line { seat, cells: line.clone() });
                if self.lines.len() == self.rules.boards as usize {
                    self.outcome = Some(Outcome { winner: Some(1 - seat), line });
                    return Ok(());
                }
            }
            self.current = 1 - seat;
        } else if !line.is_empty() {
            self.lines.push(Line { seat, cells: line.clone() });
            self.outcome = Some(Outcome { winner: Some(seat), line });
        } else if self.board.iter().all(|&s| s != Square::Empty) {
            self.outcome = Some(Outcome { winner: None, line: Vec::new() });
        } else {
            self.current = 1 - seat;
        }
        Ok(())
    }

    pub fn view(&self, seat: usize) -> GridView {
        GridView { you: Some(seat), ..self.watcher_view() }
    }

    pub fn watcher_view(&self) -> GridView {
        GridView {
            you: None,
            rules: self.rules,
            board: self.board.clone(),
            current: self.current,
            moves: self.moves.clone(),
            lines: self.lines.clone(),
            outcome: self.outcome.clone(),
        }
    }

    /// Each seat's place once the game is over, 0 best: the winner 0 and
    /// the other 1, or both 0 for a draw.
    pub fn placings(&self) -> Option<Vec<u32>> {
        let outcome = self.outcome.as_ref()?;
        Some((0..2).map(|seat| outcome.winner.is_some_and(|w| w != seat) as u32).collect())
    }
}

/// Whether the player who sees `view` may put their mark on `cell`, for
/// instant feedback in the browser (the server checks again).
pub fn check_place(view: &GridView, cell: Cell) -> Result<(), GridError> {
    check(&view.rules, &view.board, &view.lines, view.current, view.outcome.is_some(), view.you, cell).map(|_| ())
}

/// The cell's index if `seat` may put their mark there.
fn check(rules: &GridRules, board: &[Square], lines: &[Line], current: usize, over: bool, seat: Option<usize>, cell: Cell) -> Result<usize, GridError> {
    if over {
        return Err(GridError::Over);
    }
    if seat != Some(current) {
        return Err(GridError::NotYourTurn);
    }
    let index = rules.index(cell).ok_or(GridError::OffBoard)?;
    if board[index] != Square::Empty {
        return Err(GridError::Occupied);
    }
    let on = rules.board_of(cell);
    if lines.iter().any(|line| rules.board_of(line.cells[0]) == on) {
        return Err(GridError::Closed);
    }
    Ok(index)
}

/// The longest row of `cell`'s mark through it on its board, if it's a
/// winning line (`rules.line` or more), end to end; else empty.
fn line_through(rules: &GridRules, board: &[Square], cell: Cell) -> Vec<Cell> {
    let Some(index) = rules.index(cell) else { return Vec::new() };
    let mark = board[index];
    if mark == Square::Empty {
        return Vec::new();
    }
    let on = rules.board_of(cell);
    let at = |row: i32, col: i32| {
        let next = Cell { row: u8::try_from(row).ok()?, col: u8::try_from(col).ok()? };
        rules.index(next).filter(|&i| board[i] == mark && rules.board_of(next) == on).map(|_| next)
    };
    let mut best = Vec::new();
    for (dr, dc) in DIRECTIONS {
        let (row, col) = (cell.row as i32, cell.col as i32);
        let back = (1..).take_while(|&k| at(row - k * dr, col - k * dc).is_some()).count() as i32;
        let line: Vec<Cell> = (-back..)
            .map_while(|k| at(row + k * dr, col + k * dc))
            .collect();
        if line.len() > best.len() {
            best = line;
        }
    }
    if best.len() >= rules.line as usize {
        best
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(row: u8, col: u8) -> Cell {
        Cell { row, col }
    }

    /// Plays `cells` in turn from seat 0.
    fn play(rules: GridRules, cells: &[(u8, u8)]) -> Game {
        let mut game = Game::new(rules);
        for &(row, col) in cells {
            let seat = game.current();
            game.place(seat, c(row, col)).unwrap();
        }
        game
    }

    #[test]
    fn named_rule_sets() {
        assert_eq!((GridRules::default().width, GridRules::default().line), (3, 3));
        let gomoku = GridRules::named(RuleSet::Gomoku);
        assert_eq!((gomoku.width, gomoku.height, gomoku.line), (15, 15, 5));
    }

    #[test]
    fn house_rules_are_kept_in_range() {
        let odd = GridRules { rule_set: RuleSet::Gomoku, width: 30, height: 4, line: 9, boards: 2 }.checked();
        assert_eq!((odd.width, odd.height, odd.line, odd.boards), (19, 4, 4, 1));
        let tiny = GridRules { rule_set: RuleSet::TicTacToe, width: 1, height: 0, line: 1, boards: 0 }.checked();
        assert_eq!((tiny.width, tiny.height, tiny.line), (3, 3, 3));
    }

    #[test]
    fn x_moves_first_and_turns_alternate() {
        let mut game = Game::new(GridRules::default());
        assert_eq!(game.place(1, c(0, 0)), Err(GridError::NotYourTurn));
        game.place(0, c(1, 1)).unwrap();
        assert_eq!(game.board()[4], Square::X);
        assert_eq!(game.current(), 1);
        assert_eq!(game.place(1, c(1, 1)), Err(GridError::Occupied));
        assert_eq!(game.place(1, c(3, 0)), Err(GridError::OffBoard));
        assert_eq!(game.place(0, c(0, 0)), Err(GridError::NotYourTurn));
        game.place(1, c(0, 0)).unwrap();
        assert_eq!(game.moves(), [c(1, 1), c(0, 0)]);
    }

    #[test]
    fn lines_in_every_direction_win() {
        let rules = GridRules::default();
        // X across the middle row.
        let game = play(rules, &[(1, 0), (0, 0), (1, 1), (0, 1), (1, 2)]);
        assert_eq!(game.outcome(), Some(&Outcome { winner: Some(0), line: vec![c(1, 0), c(1, 1), c(1, 2)] }));
        // O down the right column.
        let game = play(rules, &[(0, 0), (0, 2), (1, 0), (1, 2), (2, 1), (2, 2)]);
        assert_eq!(game.outcome().unwrap().winner, Some(1));
        // X on both diagonals.
        let game = play(rules, &[(0, 0), (0, 1), (1, 1), (0, 2), (2, 2)]);
        assert_eq!(game.outcome().unwrap().line, [c(0, 0), c(1, 1), c(2, 2)]);
        let game = play(rules, &[(0, 2), (0, 1), (1, 1), (0, 0), (2, 0)]);
        assert_eq!(game.outcome().unwrap().line, [c(0, 2), c(1, 1), c(2, 0)]);
        assert_eq!(game.placings(), Some(vec![0, 1]));
    }

    #[test]
    fn a_full_grid_without_a_line_is_a_draw() {
        // X O X / X O O / O X X
        let game = play(GridRules::default(), &[(0, 0), (0, 1), (0, 2), (1, 1), (1, 0), (1, 2), (2, 1), (2, 0), (2, 2)]);
        assert_eq!(game.outcome(), Some(&Outcome { winner: None, line: vec![] }));
        assert_eq!(game.placings(), Some(vec![0, 0]));
        let mut game = game;
        assert_eq!(game.place(1, c(0, 0)), Err(GridError::Over));
    }

    #[test]
    fn a_longer_line_wins_too() {
        let rules = GridRules::named(RuleSet::Gomoku);
        // X: 7,3 7,4 7,6 7,7 then 7,5 joins them into six.
        let game = play(rules, &[(7, 3), (0, 0), (7, 4), (0, 2), (7, 6), (0, 4), (7, 7), (0, 6), (7, 5)]);
        let outcome = game.outcome().unwrap();
        assert_eq!(outcome.winner, Some(0));
        assert_eq!(outcome.line, (3..=7).map(|col| c(7, col)).collect::<Vec<_>>());
        let game = play(rules, &[(7, 3), (0, 0), (7, 4), (0, 2), (7, 6), (0, 4), (7, 7), (0, 6), (7, 8), (0, 8), (7, 5)]);
        assert_eq!(game.outcome().unwrap().line, (3..=8).map(|col| c(7, col)).collect::<Vec<_>>(), "all six");
        assert!(play(rules, &[(7, 3), (0, 0), (7, 4), (0, 2), (7, 5), (0, 4), (7, 6)]).outcome().is_none(), "four isn't enough");
    }

    #[test]
    fn views_and_checks() {
        let game = play(GridRules::default(), &[(1, 1)]);
        let view = game.view(1);
        assert_eq!((view.you, view.current, view.moves.len()), (Some(1), 1, 1));
        assert_eq!(check_place(&view, c(0, 0)), Ok(()));
        assert_eq!(check_place(&view, c(1, 1)), Err(GridError::Occupied));
        assert_eq!(check_place(&game.view(0), c(0, 0)), Err(GridError::NotYourTurn));
        let watcher = game.watcher_view();
        assert_eq!(watcher.you, None);
        assert_eq!(check_place(&watcher, c(0, 0)), Err(GridError::NotYourTurn));
        assert!(game.placings().is_none());
    }

    #[test]
    fn notakto_boards() {
        let rules = GridRules::named(RuleSet::Notakto);
        assert_eq!((rules.columns(), rules.rows(), rules.cells()), (9, 3, 27));
        let five = GridRules { boards: 9, width: 5, ..rules }.checked();
        assert_eq!((five.width, five.boards), (3, MAX_BOARDS));
        // Both mark X; a line on the first board (0,0 0,1 0,2) finishes it,
        // and the line at 0,2 0,3 crosses boards, so doesn't count.
        let mut game = play(rules, &[(0, 0), (0, 1), (0, 3)]);
        assert!(game.board()[..2].iter().all(|&s| s == Square::X));
        assert!(game.lines().is_empty());
        game.place(1, c(0, 2)).unwrap();
        assert_eq!(game.lines(), [Line { seat: 1, cells: vec![c(0, 0), c(0, 1), c(0, 2)] }]);
        assert_eq!(game.place(0, c(1, 1)), Err(GridError::Closed));
        assert_eq!(check_place(&game.view(0), c(2, 2)), Err(GridError::Closed));
        assert!(game.outcome().is_none());
    }

    #[test]
    fn notakto_whoever_finishes_the_last_board_loses() {
        let rules = GridRules { boards: 2, ..GridRules::named(RuleSet::Notakto) };
        // Board one finished by seat 0 (third mark); board two by seat 1.
        let game = play(rules, &[(0, 0), (1, 0), (2, 0), (0, 3), (2, 3), (1, 3)]);
        let outcome = game.outcome().expect("over");
        assert_eq!(outcome.winner, Some(0), "seat 1 made the last line");
        assert_eq!(outcome.line, [c(0, 3), c(1, 3), c(2, 3)]);
        assert_eq!(game.lines().len(), 2);
        assert_eq!(game.placings(), Some(vec![0, 1]));
    }

    #[test]
    fn as_json() {
        let game = play(GridRules::default(), &[(0, 0), (1, 1), (0, 1), (2, 2), (0, 2)]);
        let json = serde_json::to_string(&game.view(0)).unwrap();
        assert!(json.starts_with(r#"{"you":0,"rules":{"ruleSet":"ticTacToe","width":3,"height":3,"line":3,"boards":1},"board":["x","x","x","empty","o""#), "{json}");
        assert!(json.ends_with(r#""lines":[{"seat":0,"cells":[{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2}]}],"outcome":{"winner":0,"line":[{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2}]}}"#), "{json}");
        let watcher = serde_json::to_string(&Game::new(GridRules::default()).watcher_view()).unwrap();
        assert!(!watcher.contains("null") && !watcher.contains("you") && !watcher.contains("outcome"), "{watcher}");
        assert_eq!(serde_json::to_string(&GridError::NotYourTurn).unwrap(), r#"{"kind":"notYourTurn"}"#);
    }
}

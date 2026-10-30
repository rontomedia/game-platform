//! Ultimate Tic-Tac-Toe's bots: negamax with alpha-beta, as for the grid,
//! over a position of 81 cells and 9 boards' states. A position is worth
//! the boards won towards a line of three (more for two of a line, none
//! for a line the other has a board in), and on the open boards the
//! marks towards a line, weighted by how much that board counts.
//! - `Easy` looks one move ahead, with noise (a hash of the position).
//! - `Medium` three moves ahead.
//! - `Hard` five.
//! - `ExtraHard` deeper and deeper until its time is up.

use crate::board::mix;
use crate::bot::BotLevel;
use crate::game::{Cell, GridView, Square, META_LINES};

/// A won game: more than any position's worth.
const WIN: i32 = 1 << 24;

/// How deep Extra Hard goes at most.
const MAX_DEPTH: usize = 14;

/// Extra Hard starts another, deeper search only while it has used less
/// than this share of its time.
const DEEPER: f64 = 0.3;

/// How much a board counts: by the meta lines through it (the middle in
/// four, corners in three, edges in two).
const BOARD_WEIGHT: [i32; 9] = [3, 2, 3, 2, 4, 2, 3, 2, 3];

/// A board's state: still open, won by X or O, or full with no line.
const OPEN: u8 = 0;
const DRAWN: u8 = 3;

#[derive(Clone)]
struct Position {
    /// 0 empty, 1 X, 2 O; by board, then the board's cells row by row.
    cells: [u8; 81],
    /// `OPEN`, 1 or 2 (won by X or O), or `DRAWN`.
    boards: [u8; 9],
    /// The board to play in, or 9 for any open one.
    next: usize,
}

/// A local cell's lines, by its index in the board.
const LINES: [[usize; 3]; 8] = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];

impl Position {
    fn from_view(view: &GridView) -> Position {
        let rules = &view.rules;
        let mut cells = [0; 81];
        for (i, square) in view.board.iter().enumerate() {
            let cell = rules.cell(i);
            cells[at(cell)] = match square {
                Square::X => 1,
                Square::O => 2,
                _ => 0,
            };
        }
        let mut position = Position { cells, boards: [OPEN; 9], next: view.next_board.map_or(9, |b| b as usize) };
        for b in 0..9 {
            position.boards[b] = position.state(b);
        }
        position
    }

    fn board(&self, b: usize) -> &[u8] {
        &self.cells[b * 9..b * 9 + 9]
    }

    /// Board `b`'s state from its cells.
    fn state(&self, b: usize) -> u8 {
        let board = self.board(b);
        for line in LINES {
            let m = board[line[0]];
            if m != 0 && board[line[1]] == m && board[line[2]] == m {
                return m;
            }
        }
        if board.iter().all(|&c| c != 0) {
            DRAWN
        } else {
            OPEN
        }
    }

    /// Whether `mark` has three boards in a row.
    fn won(&self, mark: u8) -> bool {
        META_LINES.iter().any(|line| line.iter().all(|&b| self.boards[b] == mark))
    }

    fn over(&self) -> bool {
        self.boards.iter().all(|&b| b != OPEN)
    }

    /// The cells that may be marked: in the board sent to, or any open one.
    fn moves(&self) -> Vec<usize> {
        let boards: Vec<usize> = if self.next < 9 { vec![self.next] } else { (0..9).filter(|&b| self.boards[b] == OPEN).collect() };
        boards.into_iter().flat_map(|b| (b * 9..b * 9 + 9).filter(|&i| self.cells[i] == 0)).collect()
    }

    /// Marks `i` for `mark`; what to put back to undo it.
    fn play(&mut self, i: usize, mark: u8) -> (u8, usize) {
        let undo = (self.boards[i / 9], self.next);
        self.cells[i] = mark;
        let b = i / 9;
        self.boards[b] = self.state(b);
        let target = i % 9;
        self.next = if self.boards[target] == OPEN { target } else { 9 };
        undo
    }

    fn undo(&mut self, i: usize, (board, next): (u8, usize)) {
        self.cells[i] = 0;
        self.boards[i / 9] = board;
        self.next = next;
    }

    /// The position's worth to `mark`.
    fn eval(&self, mark: u8) -> i32 {
        let other = 3 - mark;
        let mut score = 0;
        for line in META_LINES {
            let (mut mine, mut theirs, mut drawn) = (0, 0, false);
            for &b in &line {
                match self.boards[b] {
                    m if m == mark => mine += 1,
                    m if m == other => theirs += 1,
                    DRAWN => drawn = true,
                    _ => {}
                }
            }
            if drawn {
                continue;
            }
            match (mine, theirs) {
                (1, 0) => score += 30,
                (2, 0) => score += 120,
                (0, 1) => score -= 30,
                (0, 2) => score -= 120,
                _ => {}
            }
        }
        for (b, &weight) in BOARD_WEIGHT.iter().enumerate() {
            match self.boards[b] {
                OPEN => {
                    let board = self.board(b);
                    let mut local = 0;
                    for line in LINES {
                        let [m, t] = line.iter().fold([0, 0], |[m, t], &k| [m + (board[k] == mark) as i32, t + (board[k] == other) as i32]);
                        local += match (m, t) {
                            (1, 0) => 1,
                            (2, 0) => 4,
                            (0, 1) => -1,
                            (0, 2) => -4,
                            _ => 0,
                        };
                    }
                    score += local * weight;
                }
                m if m == mark => score += 10 * weight,
                m if m == other => score -= 10 * weight,
                _ => {}
            }
        }
        score
    }

    fn key(&self) -> u64 {
        self.cells.iter().enumerate().fold(self.next as u64, |key, (i, &c)| if c == 0 { key } else { mix(key ^ (i as u64 * 3 + c as u64)) })
    }
}

/// Where a grid cell is in a `Position`: its board, then within it.
fn at(cell: Cell) -> usize {
    let (row, col) = (cell.row as usize, cell.col as usize);
    (row / 3 * 3 + col / 3) * 9 + row % 3 * 3 + col % 3
}

/// The grid cell of a `Position`'s index.
fn cell_of(i: usize) -> Cell {
    let (b, k) = (i / 9, i % 9);
    Cell { row: (b / 3 * 3 + k / 3) as u8, col: (b % 3 * 3 + k % 3) as u8 }
}

struct Search<'a> {
    position: Position,
    spent: &'a dyn Fn() -> f64,
    nodes: u64,
    stopped: bool,
}

impl Search<'_> {
    /// The value to `mark`, who moves, `depth` moves on.
    fn negamax(&mut self, mark: u8, depth: usize, mut alpha: i32, beta: i32) -> i32 {
        self.nodes += 1;
        if self.nodes.is_multiple_of(512) && (self.spent)() >= 1.0 {
            self.stopped = true;
        }
        if self.stopped {
            return 0;
        }
        if self.position.over() {
            return 0;
        }
        if depth == 0 {
            return self.position.eval(mark);
        }
        let mut best = -WIN * 2;
        for i in self.ordered(mark) {
            let undo = self.position.play(i, mark);
            let value = if self.position.won(mark) { WIN + depth as i32 } else { -self.negamax(3 - mark, depth - 1, -beta, -alpha) };
            self.position.undo(i, undo);
            if self.stopped {
                return 0;
            }
            best = best.max(value);
            alpha = alpha.max(value);
            if alpha >= beta {
                break;
            }
        }
        best
    }

    /// The moves, those that win a board first.
    fn ordered(&mut self, mark: u8) -> Vec<usize> {
        let mut moves: Vec<(bool, usize)> = self
            .position
            .moves()
            .into_iter()
            .map(|i| {
                let undo = self.position.play(i, mark);
                let wins = self.position.boards[i / 9] == mark;
                self.position.undo(i, undo);
                (!wins, i)
            })
            .collect();
        moves.sort_unstable();
        moves.into_iter().map(|(_, i)| i).collect()
    }

    /// The best first move `depth` deep, unless time ran out; between equal
    /// ones, a hash picks. `noise` is added to each move's value.
    fn root(&mut self, mark: u8, depth: usize, noise: i32, salt: u64) -> Option<(usize, i32)> {
        let key = self.position.key();
        let mut best: Option<(i32, u64, usize)> = None;
        for i in self.ordered(mark) {
            let undo = self.position.play(i, mark);
            let value = if self.position.won(mark) { WIN + depth as i32 } else { -self.negamax(3 - mark, depth - 1, -WIN * 2, WIN * 2) };
            self.position.undo(i, undo);
            if self.stopped {
                return None;
            }
            let hash = mix(key ^ mix(i as u64) ^ salt);
            let value = if noise > 0 { value + (hash % (2 * noise as u64 + 1)) as i32 - noise } else { value };
            if best.is_none_or(|(v, h, _)| (value, hash) > (v, h)) {
                best = Some((value, hash, i));
            }
        }
        best.map(|(value, _, i)| (i, value))
    }
}

/// The cell a bot of `level` marks in `view` (an Ultimate game not over).
pub(crate) fn choose(view: &GridView, level: BotLevel, spent: &dyn Fn() -> f64) -> Cell {
    let position = Position::from_view(view);
    let mark = view.current as u8 + 1;
    let first = position.moves()[0];
    let mut search = Search { position, spent, nodes: 0, stopped: false };
    let salt = level as u64;
    let best = match level {
        BotLevel::Easy => search.root(mark, 1, 40, salt),
        BotLevel::Medium => search.root(mark, 3, 0, salt),
        BotLevel::Hard => search.root(mark, 5, 0, salt),
        BotLevel::ExtraHard => {
            let mut best = None;
            for depth in 1..=MAX_DEPTH {
                if depth > 3 && (search.spent)() >= DEEPER {
                    break;
                }
                match search.root(mark, depth, 0, salt) {
                    Some(found) => {
                        best = Some(found);
                        if found.1 >= WIN {
                            break;
                        }
                    }
                    None => break,
                }
            }
            best
        }
    };
    cell_of(best.map_or(first, |(i, _)| i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Game, GridRules, RuleSet};
    use BotLevel::*;

    fn play_out(levels: [BotLevel; 2], budget: f64) -> Game {
        let mut game = Game::new(GridRules::named(RuleSet::Ultimate));
        while game.outcome().is_none() {
            let seat = game.current();
            let start = std::time::Instant::now();
            let cell = choose(&game.view(seat), levels[seat], &|| start.elapsed().as_secs_f64() / budget);
            game.place(seat, cell).unwrap();
        }
        game
    }

    #[test]
    fn cells_and_indices() {
        for row in 0..9 {
            for col in 0..9 {
                let cell = Cell { row, col };
                assert_eq!(cell_of(at(cell)), cell);
            }
        }
        assert_eq!(at(Cell { row: 4, col: 5 }), 4 * 9 + 5);
    }

    #[test]
    fn every_level_marks_where_it_may() {
        // A game some moves in, wherever they sent each player.
        let mut game = Game::new(GridRules::named(RuleSet::Ultimate));
        let moves = [
            (0, 0), (0, 3), (0, 1), (0, 4), (1, 3), (3, 0), (0, 2), (2, 6), (8, 2), (7, 8),
        ];
        for &(row, col) in &moves {
            let seat = game.current();
            if game.place(seat, Cell { row, col }).is_err() {
                break;
            }
        }
        // Whatever the position, every level answers with a legal mark.
        for level in [Easy, Medium, Hard, ExtraHard] {
            if game.outcome().is_some() {
                break;
            }
            let seat = game.current();
            let cell = choose(&game.view(seat), level, &|| 0.5);
            assert!(crate::game::check_place(&game.view(seat), cell.into()).is_ok(), "{level:?}");
        }
    }

    #[test]
    fn hard_beats_easy() {
        let mut wins = 0;
        for levels in [[Hard, Easy], [Easy, Hard]] {
            let game = play_out(levels, 0.2);
            let winner = game.outcome().unwrap().winner;
            wins += (winner == Some(levels.iter().position(|&l| l == Hard).unwrap())) as u32;
        }
        assert!(wins >= 1, "Hard won {wins} of 2");
    }
}

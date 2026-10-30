//! Notakto's bots. A board's marks are a 9-bit set (all X); a position is
//! the open boards' sets, each turned to the least of its 8 symmetries
//! and sorted, as boards' order and turns don't matter. There are only 46
//! such boards, so a position is searched to the end:
//! - `Hard` and `ExtraHard` play perfectly: a winning move when there is
//!   one, else one that keeps every board open, hoping for a slip.
//! - `Medium` plays as Hard once fewer than `MEDIUM_SEES` cells are safe
//!   (finish no board), and before that any safe cell.
//! - `Easy` any safe cell, else any.
//!
//! Between equal cells a hash of the position picks, so games vary.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::board::mix;
use crate::bot::BotLevel;
use crate::game::{Cell, GridView, Square};

/// A board's lines, by cell (row by row).
const LINES: [[usize; 3]; 8] = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];

/// Medium plays perfectly once fewer cells than this are safe.
const MEDIUM_SEES: usize = 12;

fn has_line(marks: u16) -> bool {
    LINES.iter().any(|line| line.iter().all(|&k| marks >> k & 1 == 1))
}

/// The 8 ways to turn and flip a board: where each cell goes.
fn symmetries() -> [[usize; 9]; 8] {
    let turn = |p: [usize; 9]| std::array::from_fn(|k| p[[6, 3, 0, 7, 4, 1, 8, 5, 2][k]]);
    let flip = |p: [usize; 9]| std::array::from_fn(|k| p[[2, 1, 0, 5, 4, 3, 8, 7, 6][k]]);
    let mut all = [[0; 9]; 8];
    let mut p: [usize; 9] = std::array::from_fn(|k| k);
    for i in 0..4 {
        all[2 * i] = p;
        all[2 * i + 1] = flip(p);
        p = turn(p);
    }
    all
}

/// Each board's least symmetric form, by its marks.
fn canon_table() -> &'static [u16; 512] {
    static TABLE: OnceLock<[u16; 512]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let symmetries = symmetries();
        std::array::from_fn(|marks| {
            symmetries
                .iter()
                .map(|s| (0..9).fold(0, |to, k| to | ((marks as u16 >> s[k] & 1) << k)))
                .min()
                .expect("symmetries")
        })
    })
}

/// A position as a key: its boards sorted, 10 bits each.
fn key(open: &[u16]) -> u64 {
    open.iter().fold(0, |key, &m| key << 10 | (m as u64 + 1))
}

struct Solver {
    canon: &'static [u16; 512],
    known: HashMap<u64, bool>,
}

impl Solver {
    fn new() -> Solver {
        Solver { canon: canon_table(), known: HashMap::new() }
    }

    fn canon(&self, marks: u16) -> u16 {
        self.canon[marks as usize]
    }

    /// The position after a mark on cell `k` of board `i` of `open`.
    fn after(&self, open: &[u16], i: usize, k: usize) -> Vec<u16> {
        let marks = open[i] | 1 << k;
        let mut next: Vec<u16> = open.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &m)| self.canon(m)).collect();
        if !has_line(marks) {
            next.push(self.canon(marks));
        }
        next.sort_unstable();
        next
    }

    /// Whether the player to move wins with `open` boards left (none: the
    /// other just finished the last), each in its least form, sorted.
    fn wins(&mut self, open: Vec<u16>) -> bool {
        self.solve(&open)
    }

    fn solve(&mut self, open: &[u16]) -> bool {
        if open.is_empty() {
            return true;
        }
        let key = key(open);
        if let Some(&known) = self.known.get(&key) {
            return known;
        }
        let mut win = false;
        'boards: for i in 0..open.len() {
            if i > 0 && open[i] == open[i - 1] {
                continue;
            }
            let marks = open[i];
            for k in (0..9).filter(|&k| marks >> k & 1 == 0) {
                let next = marks | 1 << k;
                let mut rest = open.to_vec();
                if has_line(next) {
                    rest.remove(i);
                } else {
                    rest[i] = self.canon(next);
                    rest.sort_unstable();
                }
                if !self.solve(&rest) {
                    win = true;
                    break 'boards;
                }
            }
        }
        self.known.insert(key, win);
        win
    }
}

/// The cell a bot of `level` marks in `view` (a Notakto game not over).
pub(crate) fn choose(view: &GridView, level: BotLevel) -> Cell {
    let rules = &view.rules;
    let columns = rules.columns() as usize;
    // Each open board's marks, and the cells of the grid its bits stand for.
    let mut open = Vec::new();
    let mut cells = Vec::new();
    for board in 0..rules.boards as usize {
        if view.lines.iter().any(|line| rules.board_of(line.cells[0]) == board) {
            continue;
        }
        let at: [usize; 9] = std::array::from_fn(|k| (k / 3) * columns + board * 3 + k % 3);
        open.push(at.iter().enumerate().fold(0u16, |m, (k, &i)| m | ((view.board[i] != Square::Empty) as u16) << k));
        cells.push(at);
    }
    // Every move: board, cell, and whether it's safe (finishes no board).
    let moves: Vec<(usize, usize, bool)> = open
        .iter()
        .enumerate()
        .flat_map(|(i, &m)| (0..9).filter(move |&k| m >> k & 1 == 0).map(move |k| (i, k, !has_line(m | 1 << k))))
        .collect();
    let safe: Vec<_> = moves.iter().copied().filter(|m| m.2).collect();
    let perfect = match level {
        BotLevel::Easy => false,
        BotLevel::Medium => safe.len() < MEDIUM_SEES,
        BotLevel::Hard | BotLevel::ExtraHard => true,
    };
    let picks = if perfect {
        let mut solver = Solver::new();
        let winning: Vec<_> = moves.iter().copied().filter(|&(i, k, _)| !solver.wins(solver.after(&open, i, k))).collect();
        if winning.is_empty() { safe.clone() } else { winning }
    } else {
        safe.clone()
    };
    let picks = if picks.is_empty() { moves } else { picks };
    let key = open.iter().fold(level as u64, |h, &m| mix(h ^ m as u64));
    let (i, k, _) = picks[(key % picks.len() as u64) as usize];
    rules.cell(cells[i][k])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Game, GridRules, RuleSet};
    use BotLevel::*;

    fn play_out(rules: GridRules, levels: [BotLevel; 2]) -> Game {
        let mut game = Game::new(rules);
        while game.outcome().is_none() {
            let seat = game.current();
            let cell = choose(&game.view(seat), levels[seat]);
            game.place(seat, cell).unwrap();
        }
        game
    }

    #[test]
    fn forty_six_boards() {
        let solver = Solver::new();
        let boards: std::collections::HashSet<u16> = (0..512).filter(|&m| !has_line(m)).map(|m| solver.canon(m)).collect();
        assert_eq!(boards.len(), 46);
    }

    #[test]
    fn who_wins_from_the_start() {
        // One board: the first player wins (the middle); two: the second; three: the first.
        let mut solver = Solver::new();
        assert!(solver.wins(vec![0]));
        assert!(!solver.wins(vec![0, 0]));
        assert!(solver.wins(vec![0, 0, 0]));
    }

    #[test]
    fn hard_wins_what_can_be_won() {
        for boards in 1..=3 {
            let rules = GridRules { boards, ..GridRules::named(RuleSet::Notakto) };
            let winner = if boards == 2 { 1 } else { 0 };
            for other in [Easy, Medium, Hard] {
                let mut levels = [other, other];
                levels[winner] = Hard;
                assert_eq!(play_out(rules, levels).outcome().unwrap().winner, Some(winner), "{boards} boards, Hard against {other:?}");
            }
        }
    }

    #[test]
    fn easy_keeps_boards_open() {
        let game = Game::new(GridRules::named(RuleSet::Notakto));
        let mut game = game;
        for cell in [(0, 0), (0, 1)] {
            let seat = game.current();
            game.place(seat, Cell { row: cell.0, col: cell.1 }).unwrap();
        }
        for _ in 0..4 {
            let seat = game.current();
            let cell = choose(&game.view(seat), Easy);
            assert_ne!(cell, Cell { row: 0, col: 2 }, "a safe cell");
            game.place(seat, cell).unwrap();
            if !game.lines().is_empty() {
                break;
            }
        }
    }

    #[test]
    fn four_boards_in_time() {
        let start = std::time::Instant::now();
        let rules = GridRules { boards: 4, ..GridRules::named(RuleSet::Notakto) };
        let game = play_out(rules, [Hard, Medium]);
        assert!(game.outcome().is_some());
        assert!(start.elapsed().as_secs() < 5, "{:?}", start.elapsed());
    }
}

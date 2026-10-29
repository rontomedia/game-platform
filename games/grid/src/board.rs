//! The grid as the bots search it: marks put down and taken back in place,
//! with what each line-long window of cells holds kept up to date, so a
//! position's score, a move's worth and a win cost a few window updates.

use crate::game::{GridRules, GridView};

/// What a window of `line` cells with `k` of one player's marks and none
/// of the other's is worth to that player, for `k` below `line`.
fn weight(k: usize) -> i64 {
    match k {
        0 => 0,
        k => 10i64.pow(k as u32 - 1),
    }
}

/// Cells this far from a mark (in rows or columns) are worth trying.
const NEAR: i32 = 2;

pub(crate) struct Board {
    pub width: usize,
    pub height: usize,
    pub line: usize,
    /// 0 empty, 1 X, 2 O.
    cells: Vec<u8>,
    /// The windows each cell is in.
    in_windows: Vec<Vec<usize>>,
    /// Marks of X and of O in each window.
    counts: Vec<[u8; 2]>,
    /// The position's worth to X: its windows', X's less O's.
    score: i64,
    /// Windows a mark short of a line, with none of the other's: a line
    /// next move for X, and for O.
    threats: [u32; 2],
    /// Marks within `NEAR` of each cell.
    near: Vec<u16>,
    pub marks: usize,
    /// Zobrist hash of the marks.
    pub key: u64,
    zobrist: Vec<[u64; 2]>,
}

impl Board {
    pub fn new(rules: &GridRules) -> Board {
        let (width, height, line) = (rules.width as usize, rules.height as usize, rules.line as usize);
        let mut windows = 0;
        let mut in_windows = vec![Vec::new(); width * height];
        for row in 0..height as i32 {
            for col in 0..width as i32 {
                for (dr, dc) in [(0, 1), (1, 0), (1, 1), (1, -1)] {
                    let end = (row + dr * (line as i32 - 1), col + dc * (line as i32 - 1));
                    if end.0 < 0 || end.0 >= height as i32 || end.1 < 0 || end.1 >= width as i32 {
                        continue;
                    }
                    for k in 0..line as i32 {
                        in_windows[((row + dr * k) * width as i32 + col + dc * k) as usize].push(windows);
                    }
                    windows += 1;
                }
            }
        }
        let zobrist = (0..width * height)
            .map(|i| [mix(i as u64 * 2 + 1), mix(i as u64 * 2 + 2)])
            .collect();
        Board {
            width,
            height,
            line,
            cells: vec![0; width * height],
            in_windows,
            counts: vec![[0; 2]; windows],
            score: 0,
            threats: [0; 2],
            near: vec![0; width * height],
            marks: 0,
            key: 0,
            zobrist,
        }
    }

    /// The board in a view.
    pub fn from_view(view: &GridView) -> Board {
        let mut board = Board::new(&view.rules);
        for (cell, square) in view.board.iter().enumerate() {
            if let Some(seat) = square.seat() {
                board.place(cell, seat);
            }
        }
        board
    }

    pub fn cells(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self, cell: usize) -> bool {
        self.cells[cell] == 0
    }

    pub fn full(&self) -> bool {
        self.marks == self.cells.len()
    }

    fn value(&self, window: usize) -> i64 {
        match self.counts[window] {
            [0, 0] => 0,
            [x, 0] => weight(x as usize),
            [0, o] => -weight(o as usize),
            _ => 0,
        }
    }

    /// Which players the window is a line next move for.
    fn threat(&self, window: usize) -> [u32; 2] {
        let [x, o] = self.counts[window];
        let short = self.line as u8 - 1;
        [(x == short && o == 0) as u32, (o == short && x == 0) as u32]
    }

    /// Takes the window out of the totals, changes it, and puts it back.
    fn change(&mut self, window: usize, seat: usize, by: i8) {
        let [x, o] = self.threat(window);
        self.threats[0] -= x;
        self.threats[1] -= o;
        self.score -= self.value(window);
        self.counts[window][seat] = (self.counts[window][seat] as i8 + by) as u8;
        self.score += self.value(window);
        let [x, o] = self.threat(window);
        self.threats[0] += x;
        self.threats[1] += o;
    }

    /// Puts `seat`'s mark on the empty `cell`; whether it makes a line.
    pub fn place(&mut self, cell: usize, seat: usize) -> bool {
        let mut won = false;
        for i in 0..self.in_windows[cell].len() {
            let window = self.in_windows[cell][i];
            self.change(window, seat, 1);
            won |= self.counts[window][seat] as usize == self.line;
        }
        self.cells[cell] = seat as u8 + 1;
        self.marks += 1;
        self.key ^= self.zobrist[cell][seat];
        self.mark_near(cell, 1);
        won
    }

    /// Takes the mark on `cell` back.
    pub fn undo(&mut self, cell: usize) {
        let seat = self.cells[cell] as usize - 1;
        for i in 0..self.in_windows[cell].len() {
            self.change(self.in_windows[cell][i], seat, -1);
        }
        self.cells[cell] = 0;
        self.marks -= 1;
        self.key ^= self.zobrist[cell][seat];
        self.mark_near(cell, -1);
    }

    fn mark_near(&mut self, cell: usize, by: i32) {
        let (row, col) = ((cell / self.width) as i32, (cell % self.width) as i32);
        for r in (row - NEAR).max(0)..=(row + NEAR).min(self.height as i32 - 1) {
            for c in (col - NEAR).max(0)..=(col + NEAR).min(self.width as i32 - 1) {
                let near = &mut self.near[r as usize * self.width + c as usize];
                *near = (*near as i32 + by) as u16;
            }
        }
    }

    /// Whether `seat` would make a line on `cell`.
    pub fn wins_at(&self, cell: usize, seat: usize) -> bool {
        self.in_windows[cell].iter().any(|&w| self.counts[w][seat] as usize == self.line - 1 && self.counts[w][1 - seat] == 0)
    }

    /// Whether `seat` can make a line with their next mark.
    pub fn threatens(&self, seat: usize) -> bool {
        self.threats[seat] > 0
    }

    /// The position's worth to `seat`.
    pub fn eval(&self, seat: usize) -> i64 {
        if seat == 0 {
            self.score
        } else {
            -self.score
        }
    }

    /// How much a mark on `cell` would do: for whoever moves, and against
    /// the other (what it takes from their windows).
    pub fn worth(&self, cell: usize, seat: usize) -> i64 {
        let mut worth = 0;
        for &w in &self.in_windows[cell] {
            let [mine, theirs] = [self.counts[w][seat] as usize, self.counts[w][1 - seat] as usize];
            if theirs == 0 {
                worth += weight(mine + 1) - weight(mine);
            }
            if mine == 0 {
                // Blocking counts a little less than building.
                worth += (weight(theirs + 1) - weight(theirs)) * 9 / 10;
            }
        }
        worth
    }

    /// The empty cells worth trying, best first: near a mark (the middle
    /// on an empty board), or all of them on a small grid.
    pub fn candidates(&self, seat: usize) -> Vec<usize> {
        if self.marks == 0 {
            return vec![(self.height / 2) * self.width + self.width / 2];
        }
        let all = self.cells.len() <= 25;
        let mut cells: Vec<(i64, usize)> = (0..self.cells.len())
            .filter(|&c| self.cells[c] == 0 && (all || self.near[c] > 0))
            .map(|c| (self.worth(c, seat), c))
            .collect();
        cells.sort_by_key(|&(worth, cell)| (std::cmp::Reverse(worth), cell));
        cells.into_iter().map(|(_, c)| c).collect()
    }

    /// The board's cells as squares (for tests).
    #[cfg(test)]
    pub fn squares(&self) -> Vec<crate::game::Square> {
        use crate::game::Square;
        self.cells.iter().map(|&c| [Square::Empty, Square::X, Square::O][c as usize]).collect()
    }

    #[cfg(test)]
    pub fn window_count(&self) -> usize {
        self.counts.len()
    }
}

/// SplitMix64's finaliser: a well-spread hash of `x`.
pub(crate) fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{RuleSet, Square};

    #[test]
    fn windows_and_updates() {
        let board = Board::new(&GridRules::default());
        assert_eq!(board.window_count(), 8, "3 rows, 3 columns, 2 diagonals");
        let mut board = Board::new(&GridRules::named(RuleSet::Gomoku));
        assert_eq!(board.window_count(), 11 * 15 * 2 + 11 * 11 * 2);
        let (key, score) = (board.key, board.score);
        assert!(!board.place(7 * 15 + 7, 0));
        assert!(board.score > 0 && board.key != key);
        board.undo(7 * 15 + 7);
        assert_eq!((board.key, board.score, board.marks), (key, score, 0));
        assert!(board.squares().iter().all(|&s| s == Square::Empty));
    }

    #[test]
    fn a_line_is_seen() {
        let mut board = Board::new(&GridRules::default());
        board.place(0, 0);
        board.place(1, 0);
        assert!(board.wins_at(2, 0) && !board.wins_at(2, 1));
        assert!(board.threatens(0) && !board.threatens(1));
        assert!(board.place(2, 0));
    }
}

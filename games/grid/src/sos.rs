//! S.O.S's bots. Writing a letter that makes an S-O-S scores and plays
//! again, so a turn is a chain of scoring letters ended by one that
//! scores nothing; that last letter is what matters: it may leave an
//! S-O-S for the other to take (an S _ S, an S O _), and late in the game
//! every letter does.
//! - `Easy` takes an S-O-S on most turns, else writes anything.
//! - `Medium` takes the most it can, else writes a safe letter (leaving
//!   none to take), else anything.
//! - `Hard` as Medium, but when nothing is safe, the letter that gives the
//!   other the least: what they would take after it, greedily.
//! - `ExtraHard` as Hard, and plays the last `ENDGAME` cells perfectly
//!   (searched to the end, positions remembered).
//!
//! Between equal moves a hash of the position picks, so games vary.

use std::collections::HashMap;

use crate::board::mix;
use crate::bot::BotLevel;
use crate::game::{GridView, Letter, Placing, Square};

/// Extra Hard searches to the end from this many empty cells.
const ENDGAME: usize = 10;

/// The 8 ways from a cell.
const STEPS: [(i32, i32); 8] = [(0, 1), (1, 0), (1, 1), (1, -1), (0, -1), (-1, 0), (-1, -1), (-1, 1)];

const S: u8 = 1;
const O: u8 = 2;

#[derive(Clone)]
struct Grid {
    width: i32,
    height: i32,
    /// 0 empty, `S` or `O`.
    cells: Vec<u8>,
}

impl Grid {
    fn at(&self, row: i32, col: i32) -> u8 {
        if row < 0 || col < 0 || row >= self.height || col >= self.width {
            return 0;
        }
        self.cells[(row * self.width + col) as usize]
    }

    /// The S-O-Ss `letter` on the empty cell `i` would make.
    fn gain(&self, i: usize, letter: u8) -> u32 {
        let (row, col) = (i as i32 / self.width, i as i32 % self.width);
        if letter == O {
            STEPS[..4].iter().filter(|&&(dr, dc)| self.at(row - dr, col - dc) == S && self.at(row + dr, col + dc) == S).count() as u32
        } else {
            STEPS.iter().filter(|&&(dr, dc)| self.at(row + dr, col + dc) == O && self.at(row + 2 * dr, col + 2 * dc) == S).count() as u32
        }
    }

    fn moves(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        (0..self.cells.len()).filter(|&i| self.cells[i] == 0).flat_map(|i| [(i, S), (i, O)])
    }

    /// The scoring move that scores most, if any.
    fn best_gain(&self) -> Option<(u32, usize, u8)> {
        self.moves().map(|(i, l)| (self.gain(i, l), i, l)).filter(|m| m.0 > 0).max_by_key(|m| (m.0, std::cmp::Reverse(m.1)))
    }

    /// What the player to move takes by scoring greedily until they can't.
    fn chain(&mut self) -> u32 {
        let mut taken = Vec::new();
        let mut total = 0;
        while let Some((gain, i, letter)) = self.best_gain() {
            self.cells[i] = letter;
            taken.push(i);
            total += gain;
        }
        for i in taken {
            self.cells[i] = 0;
        }
        total
    }

    fn empty(&self) -> usize {
        self.cells.iter().filter(|&&c| c == 0).count()
    }

    fn key(&self) -> u64 {
        self.cells.iter().fold(0, |key, &c| mix(key ^ c as u64))
    }
}

/// The letter a bot of `level` writes in `view` (an S.O.S game not over).
pub(crate) fn choose(view: &GridView, level: BotLevel) -> Placing {
    let rules = &view.rules;
    let cells = view.board.iter().map(|s| match s {
        Square::S => S,
        Square::Empty => 0,
        _ => O,
    });
    let mut grid = Grid { width: rules.columns() as i32, height: rules.rows() as i32, cells: cells.collect() };
    let key = mix(grid.key() ^ level as u64);
    let (i, letter) = pick(&mut grid, level, key);
    Placing { cell: rules.cell(i), letter: Some(if letter == S { Letter::S } else { Letter::O }) }
}

fn pick(grid: &mut Grid, level: BotLevel, key: u64) -> (usize, u8) {
    let any = |moves: &[(usize, u8)]| moves[(key % moves.len() as u64) as usize];
    let all: Vec<(usize, u8)> = grid.moves().collect();
    if level == BotLevel::ExtraHard && grid.empty() <= ENDGAME {
        return endgame(grid, key);
    }
    if let Some((_, i, letter)) = grid.best_gain() {
        if level != BotLevel::Easy || key % 10 < 7 {
            return (i, letter);
        }
    }
    if level == BotLevel::Easy {
        return any(&all);
    }
    // What each move leaves the other to take.
    let mut given: Vec<(u32, usize, u8)> = Vec::new();
    for &(i, letter) in &all {
        grid.cells[i] = letter;
        let gives = if level == BotLevel::Medium { grid.best_gain().is_some() as u32 } else { grid.chain() };
        grid.cells[i] = 0;
        given.push((gives, i, letter));
    }
    let least = given.iter().map(|g| g.0).min().expect("a free cell");
    if level == BotLevel::Medium && least > 0 {
        return any(&all);
    }
    let best: Vec<(usize, u8)> = given.into_iter().filter(|g| g.0 == least).map(|g| (g.1, g.2)).collect();
    any(&best)
}

/// The best move searched to the end.
fn endgame(grid: &mut Grid, key: u64) -> (usize, u8) {
    let mut known = HashMap::new();
    let moves: Vec<(usize, u8)> = grid.moves().collect();
    let mut best = (i32::MIN, 0, moves[0]);
    for (n, &(i, letter)) in moves.iter().enumerate() {
        let value = play(grid, i, letter, &mut known);
        let tie = mix(key ^ n as u64);
        if (value, tie) > (best.0, best.1) {
            best = (value, tie, (i, letter));
        }
    }
    best.2
}

/// Writes `letter` on `i` and returns the mover's points from here on,
/// less the other's, with best play.
fn play(grid: &mut Grid, i: usize, letter: u8, known: &mut HashMap<u64, i32>) -> i32 {
    let gain = grid.gain(i, letter) as i32;
    grid.cells[i] = letter;
    let rest = value(grid, known);
    grid.cells[i] = 0;
    if gain > 0 {
        gain + rest
    } else {
        -rest
    }
}

/// The value of the grid to whoever moves: their points to come less the
/// other's.
fn value(grid: &mut Grid, known: &mut HashMap<u64, i32>) -> i32 {
    let key = grid.key();
    if let Some(&value) = known.get(&key) {
        return value;
    }
    let moves: Vec<(usize, u8)> = grid.moves().collect();
    let best = moves.into_iter().map(|(i, letter)| play(grid, i, letter, known)).max().unwrap_or(0);
    known.insert(key, best);
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Cell, Game, GridRules, RuleSet};
    use BotLevel::*;

    fn c(row: u8, col: u8) -> Cell {
        Cell { row, col }
    }

    fn sos(width: u8, height: u8, letters: &[(u8, u8, Letter)]) -> Game {
        let mut game = Game::new(GridRules { width, height, ..GridRules::named(RuleSet::Sos) });
        for &(row, col, letter) in letters {
            let seat = game.current();
            game.play(seat, Placing { cell: c(row, col), letter: Some(letter) }).unwrap();
        }
        game
    }

    fn play_out(game: &mut Game, levels: [BotLevel; 2]) {
        while game.outcome().is_none() {
            let seat = game.current();
            let placing = choose(&game.view(seat), levels[seat]);
            game.play(seat, placing).unwrap();
        }
    }

    #[test]
    fn takes_an_sos() {
        // S _ S across the top: the O between scores.
        let game = sos(5, 5, &[(0, 0, Letter::S), (0, 2, Letter::S)]);
        for level in [Medium, Hard, ExtraHard] {
            assert_eq!(choose(&game.view(0), level), Placing { cell: c(0, 1), letter: Some(Letter::O) }, "{level:?}");
        }
    }

    #[test]
    fn leaves_nothing_to_take() {
        // One S in the middle of a 5 × 5: an O beside it, or an S two away, would give an S-O-S.
        let game = sos(5, 5, &[(2, 2, Letter::S)]);
        for level in [Medium, Hard] {
            let mut next = game.clone();
            let seat = next.current();
            next.play(seat, choose(&game.view(seat), level)).unwrap();
            let left = next.view(1 - seat);
            let mut grid = Grid { width: 5, height: 5, cells: left.board.iter().map(|s| (*s == Square::S) as u8 + 2 * (*s == Square::O) as u8).collect() };
            assert_eq!(grid.chain(), 0, "{level:?} left an S-O-S");
        }
    }

    #[test]
    fn extra_hard_beats_hard_at_the_end_or_ties() {
        let mut wins = [0; 2];
        for first in 0..6u8 {
            let mut game = sos(4, 4, &[(first / 4, first % 4, Letter::S)]);
            play_out(&mut game, [ExtraHard, Hard]);
            if let Some(w) = game.outcome().unwrap().winner {
                wins[w] += 1;
            }
        }
        assert!(wins[0] >= wins[1], "{wins:?}");
    }

    #[test]
    fn bots_finish_games() {
        let mut game = Game::new(GridRules::named(RuleSet::Sos));
        let start = std::time::Instant::now();
        play_out(&mut game, [Easy, Hard]);
        let mut game = Game::new(GridRules::named(RuleSet::Sos));
        play_out(&mut game, [Medium, ExtraHard]);
        assert!(game.outcome().is_some());
        assert!(start.elapsed().as_secs() < 30, "{:?}", start.elapsed());
    }
}

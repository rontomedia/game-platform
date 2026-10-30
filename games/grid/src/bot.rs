//! Computer players: the cell a bot puts its mark on, from what it sees.
//!
//! Every level makes a line when it can. Then:
//! - `Easy` blocks a line the other would make next, else plays what
//!   looks best one move ahead, with noise (from a hash of the position,
//!   so the engine stays pure): it misses most threats further off.
//! - `Medium` looks two moves ahead (its own and the reply).
//! - `Hard` looks four ahead on a big grid; a small one (Tic-Tac-Toe) it
//!   searches to the end, so it never loses there.
//! - `ExtraHard` plays as Hard on a small grid, and on a big one looks
//!   deeper and deeper (iterative deepening) until its time is up.
//!
//! The search is negamax with alpha-beta over the cells near the marks,
//! the most promising first (`Board::candidates`), scored by what each
//! line-long window of cells holds.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::board::{mix, Board};
use crate::game::{GridView, Placing, RuleSet};
use crate::{notakto, sos};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum BotLevel {
    Easy,
    #[default]
    Medium,
    Hard,
    #[serde(rename = "extraHard")]
    ExtraHard,
}

/// Grids of at most this many cells are searched to the end.
const SOLVED: usize = 12;

/// How many of a position's cells are tried, below the first move...
const WIDTH: usize = 10;
/// ...and for the first.
const ROOT_WIDTH: usize = 16;

/// The deepest Extra Hard looks.
const MAX_DEPTH: usize = 12;

/// Extra Hard starts another, deeper search only while it has used less
/// than this share of its time (each takes several times the last).
const DEEPER: f64 = 0.3;

/// A line: more than any position's score.
const WIN: i64 = 1 << 40;

/// The move a bot of `level` makes for the player whose turn it is in
/// `view` (the game must not be over).
pub fn bot_move(view: &GridView, level: BotLevel) -> Placing {
    bot_move_with(view, level, &|| 0.0)
}

/// As `bot_move`, with `spent` the share of its time used so far (the
/// engine has no clock): Hard and Extra Hard stop looking when it
/// reaches 1, and play the best they had found.
pub fn bot_move_with(view: &GridView, level: BotLevel, spent: &dyn Fn() -> f64) -> Placing {
    match view.rules.rule_set {
        RuleSet::Notakto => return notakto::choose(view, level).into(),
        RuleSet::Sos => return sos::choose(view, level),
        RuleSet::TicTacToe | RuleSet::Gomoku => {}
    }
    let mut board = Board::from_view(view);
    let cell = choose(&mut board, view.current, level, spent);
    view.rules.cell(cell).into()
}

fn choose(board: &mut Board, seat: usize, level: BotLevel, spent: &dyn Fn() -> f64) -> usize {
    let moves = board.candidates(seat);
    let Some(&first) = moves.first() else { return 0 };
    if let Some(&win) = moves.iter().find(|&&c| board.wins_at(c, seat)) {
        return win;
    }
    let salt = level as u64;
    match level {
        BotLevel::Easy => easy(board, seat, &moves),
        BotLevel::Medium => Search::new(board, spent).deepening(seat, 2, salt).unwrap_or(first),
        BotLevel::Hard | BotLevel::ExtraHard if board.cells() <= SOLVED => solved(board, seat, salt),
        BotLevel::Hard => Search::new(board, spent).deepening(seat, 4, salt).unwrap_or(first),
        BotLevel::ExtraHard => Search::new(board, spent).deepening(seat, MAX_DEPTH, salt).unwrap_or(first),
    }
}

/// Easy: blocks the other's line, else one move ahead with noise.
fn easy(board: &mut Board, seat: usize, moves: &[usize]) -> usize {
    if let Some(&block) = moves.iter().find(|&&c| board.wins_at(c, 1 - seat)) {
        return block;
    }
    // About an open three's worth on Gomoku's grid.
    let noise = 5 * 10i64.pow(board.line.saturating_sub(3) as u32) as u64;
    let key = board.key;
    let mut best = (i64::MIN, moves[0]);
    for &cell in moves {
        board.place(cell, seat);
        let worth = board.eval(seat) + (mix(key ^ mix(cell as u64)) % (2 * noise + 1)) as i64 - noise as i64;
        board.undo(cell);
        if worth > best.0 {
            best = (worth, cell);
        }
    }
    best.1
}

/// A small grid searched to the end: of the best cells, one chosen by a
/// hash of the position.
fn solved(board: &mut Board, seat: usize, salt: u64) -> usize {
    let mut known = HashMap::new();
    let mut values = Vec::new();
    let free: Vec<usize> = (0..board.cells()).filter(|&c| board.is_empty(c)).collect();
    for cell in free {
        let value = if board.place(cell, seat) { win_value(board) } else { -solve(board, 1 - seat, &mut known) };
        board.undo(cell);
        values.push((value, cell));
    }
    let best = values.iter().map(|&(v, _)| v).max().expect("a free cell");
    let best: Vec<usize> = values.into_iter().filter(|&(v, _)| v == best).map(|(_, c)| c).collect();
    best[(mix(board.key ^ salt) % best.len() as u64) as usize]
}

/// A line just made, worth more the sooner.
fn win_value(board: &Board) -> i32 {
    100 - board.marks as i32
}

/// The position's value to `seat`, who moves: a win (sooner better), 0 a
/// draw, a loss.
fn solve(board: &mut Board, seat: usize, known: &mut HashMap<u64, i32>) -> i32 {
    if let Some(&value) = known.get(&board.key) {
        return value;
    }
    if board.full() {
        return 0;
    }
    let mut best = i32::MIN;
    for cell in 0..board.cells() {
        if !board.is_empty(cell) {
            continue;
        }
        let value = if board.place(cell, seat) { win_value(board) } else { -solve(board, 1 - seat, known) };
        board.undo(cell);
        best = best.max(value);
    }
    known.insert(board.key, best);
    best
}

struct Search<'a> {
    board: &'a mut Board,
    spent: &'a dyn Fn() -> f64,
    nodes: u64,
    stopped: bool,
}

impl<'a> Search<'a> {
    fn new(board: &'a mut Board, spent: &'a dyn Fn() -> f64) -> Search<'a> {
        Search { board, spent, nodes: 0, stopped: false }
    }

    /// The best cell from searches 1, 2, ... `depth` moves deep, the
    /// deepest finished in time; none if not even the first was.
    fn deepening(&mut self, seat: usize, depth: usize, salt: u64) -> Option<usize> {
        let mut best = None;
        for depth in 1..=depth {
            if depth > 2 && (self.spent)() >= DEEPER {
                break;
            }
            match self.root(seat, depth, salt) {
                Some((cell, value)) => {
                    best = Some(cell);
                    if value >= WIN {
                        break;
                    }
                }
                None => break,
            }
        }
        best
    }

    /// The cells worth trying for `seat`: only those that stop the other's
    /// line next move, if there is one.
    fn moves(&self, seat: usize) -> Vec<usize> {
        let moves = self.board.candidates(seat);
        if self.board.threatens(1 - seat) {
            let blocks: Vec<usize> = moves.iter().copied().filter(|&c| self.board.wins_at(c, 1 - seat)).collect();
            if !blocks.is_empty() {
                return blocks;
            }
        }
        moves
    }

    /// The first move: its cell and value, unless time ran out. Between
    /// cells of equal value, a hash of the position picks, so games vary.
    fn root(&mut self, seat: usize, depth: usize, salt: u64) -> Option<(usize, i64)> {
        let moves = self.moves(seat);
        let mut best: Option<(i64, u64, usize)> = None;
        let mut alpha = -WIN * 2;
        for &cell in moves.iter().take(ROOT_WIDTH) {
            let value = if self.board.place(cell, seat) {
                WIN + depth as i64
            } else {
                -self.negamax(1 - seat, depth - 1, -WIN * 2, -alpha + 1)
            };
            self.board.undo(cell);
            if self.stopped {
                return None;
            }
            let tie = mix(self.board.key ^ mix(cell as u64) ^ salt);
            if best.is_none_or(|(v, t, _)| (value, tie) > (v, t)) {
                best = Some((value, tie, cell));
                alpha = alpha.max(value);
            }
        }
        best.map(|(value, _, cell)| (cell, value))
    }

    /// The position's value to `seat`, who moves, looking `depth` moves on.
    fn negamax(&mut self, seat: usize, depth: usize, mut alpha: i64, beta: i64) -> i64 {
        self.nodes += 1;
        if self.nodes.is_multiple_of(256) && (self.spent)() >= 1.0 {
            self.stopped = true;
        }
        if self.stopped {
            return 0;
        }
        if self.board.threatens(seat) {
            return WIN + depth as i64;
        }
        if self.board.full() {
            return 0;
        }
        if depth == 0 {
            return self.board.eval(seat);
        }
        let mut best = -WIN * 2;
        for cell in self.moves(seat).into_iter().take(WIDTH) {
            self.board.place(cell, seat);
            let value = -self.negamax(1 - seat, depth - 1, -beta, -alpha);
            self.board.undo(cell);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Cell, Game, GridRules};
    use BotLevel::*;

    const LEVELS: [BotLevel; 4] = [Easy, Medium, Hard, ExtraHard];

    fn c(row: u8, col: u8) -> Cell {
        Cell { row, col }
    }

    /// A game with `cells` played in turn from seat 0.
    fn game(rules: GridRules, cells: &[(u8, u8)]) -> Game {
        let mut game = Game::new(rules);
        for &(row, col) in cells {
            let seat = game.current();
            game.place(seat, c(row, col)).unwrap();
        }
        game
    }

    /// Plays `game` out between `levels` (by seat).
    fn play_out(game: &mut Game, levels: [BotLevel; 2]) {
        while game.outcome().is_none() {
            let seat = game.current();
            let placing = bot_move(&game.view(seat), levels[seat]);
            game.play(seat, placing).unwrap();
        }
    }

    /// Every way the other can play against `level` in `seat` from
    /// `game`: whether `level` ever loses.
    fn ever_loses(game: &Game, level: BotLevel, seat: usize) -> bool {
        if let Some(outcome) = game.outcome() {
            return outcome.winner == Some(1 - seat);
        }
        if game.current() == seat {
            let mut next = game.clone();
            next.play(seat, bot_move(&game.view(seat), level)).unwrap();
            return ever_loses(&next, level, seat);
        }
        let rules = *game.rules();
        (0..rules.cells()).map(|i| rules.cell(i)).any(|cell| {
            let mut next = game.clone();
            next.place(1 - seat, cell).is_ok() && ever_loses(&next, level, seat)
        })
    }

    #[test]
    fn hard_never_loses_tic_tac_toe() {
        let start = Game::new(GridRules::default());
        for level in [Hard, ExtraHard] {
            assert!(!ever_loses(&start, level, 0), "{level:?} as X");
            assert!(!ever_loses(&start, level, 1), "{level:?} as O, from any first move");
        }
    }

    #[test]
    fn perfect_players_draw() {
        for first in 0..9 {
            let rules = GridRules::default();
            let cell = rules.cell(first);
            let mut game = game(rules, &[(cell.row, cell.col)]);
            play_out(&mut game, [Hard, ExtraHard]);
            assert_eq!(game.outcome().unwrap().winner, None, "after X on {cell:?}");
        }
    }

    #[test]
    fn every_level_takes_a_line_and_blocks_one() {
        // X X . / O O . / . . . with X to move: X makes its line. With X's
        // second mark in a corner instead, X must stop O's.
        let rules = GridRules::default();
        let win = game(rules, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let block = game(rules, &[(0, 0), (1, 0), (2, 2), (1, 1)]);
        for level in LEVELS {
            assert_eq!(bot_move(&win.view(0), level).cell, c(0, 2), "{level:?} wins");
            assert_eq!(bot_move(&block.view(0), level).cell, c(1, 2), "{level:?} blocks");
        }
    }

    /// A Gomoku game after `moves`, X's and O's in turn.
    fn gomoku(moves: &[(u8, u8)]) -> Game {
        game(GridRules::named(RuleSet::Gomoku), moves)
    }

    #[test]
    fn gomoku_bots_make_five_and_stop_a_four() {
        // X has four across row 7 (open at both ends); O to block can't, X wins.
        let five = gomoku(&[(7, 5), (8, 5), (7, 6), (8, 6), (7, 7), (8, 7), (7, 8), (0, 0)]);
        // O has four down column 3 (X at the top), X to move must take the bottom.
        let four = gomoku(&[(1, 3), (2, 3), (9, 9), (3, 3), (9, 11), (4, 3), (12, 12), (5, 3)]);
        let start = std::time::Instant::now();
        let spent = || start.elapsed().as_secs_f64() / 2.0;
        for level in LEVELS {
            let cell = bot_move_with(&five.view(0), level, &spent).cell;
            assert!([c(7, 4), c(7, 9)].contains(&cell), "{level:?} makes five, not {cell:?}");
            assert_eq!(bot_move_with(&four.view(0), level, &spent).cell, c(6, 3), "{level:?} blocks the four");
        }
    }

    #[test]
    fn medium_and_up_stop_an_open_three() {
        // O has an open three down column 5; X, with nothing of its own, blocks an end.
        let three = gomoku(&[(0, 0), (6, 5), (0, 14), (7, 5), (14, 0), (8, 5)]);
        for level in [Medium, Hard, ExtraHard] {
            let start = std::time::Instant::now();
            let cell = bot_move_with(&three.view(0), level, &|| start.elapsed().as_secs_f64() / 2.0).cell;
            assert!([c(5, 5), c(9, 5), c(4, 5), c(10, 5)].contains(&cell), "{level:?} played {cell:?}");
        }
    }

    #[test]
    fn out_of_time_still_moves() {
        let mut game = gomoku(&[(7, 7), (7, 8)]);
        let placing = bot_move_with(&game.view(0), ExtraHard, &|| 1.0);
        assert!(game.play(0, placing).is_ok());
        // An empty grid: the middle.
        assert_eq!(bot_move(&gomoku(&[]).view(0), Hard).cell, c(7, 7));
    }

    #[test]
    fn bots_finish_games() {
        let mut game = gomoku(&[]);
        play_out(&mut game, [Easy, Medium]);
        assert!(game.outcome().is_some());
        let mut game = Game::new(GridRules { width: 4, height: 4, line: 3, ..GridRules::default() });
        play_out(&mut game, [Hard, Easy]);
        assert!(game.outcome().is_some());
    }

    /// How the levels fare against each other on each rule set, and how
    /// long a move takes:
    /// `cargo test -p grid --release -- --ignored --nocapture grid_strength`.
    /// `GRID_GAMES` games a pair (default 20), Extra Hard thinking up to
    /// `GRID_MS` a move (default 1000).
    #[test]
    #[ignore]
    fn grid_strength() {
        let var = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
        let (games, budget) = (var("GRID_GAMES", 20), var("GRID_MS", 1000) as f64 / 1000.0);
        for rule_set in [RuleSet::TicTacToe, RuleSet::Gomoku, RuleSet::Notakto, RuleSet::Sos] {
            let rules = GridRules::named(rule_set);
            for (a, b) in [(Medium, Easy), (Hard, Easy), (Hard, Medium), (ExtraHard, Hard)] {
                let (mut wins, mut draws) = ([0; 2], 0);
                let mut time = [(std::time::Duration::ZERO, std::time::Duration::ZERO, 0u32); 2];
                for g in 0..games {
                    // Each sits first in half the games, X's first mark from the game's number.
                    let levels = if g % 2 == 0 { [a, b] } else { [b, a] };
                    let first = rules.cell(mix(g) as usize % rules.cells());
                    let mut game = Game::new(rules);
                    let letter = rules.letters().then_some(crate::game::Letter::S);
                    game.play(0, Placing { cell: first, letter }).unwrap();
                    while game.outcome().is_none() {
                        let seat = game.current();
                        let start = std::time::Instant::now();
                        let placing = bot_move_with(&game.view(seat), levels[seat], &|| start.elapsed().as_secs_f64() / budget);
                        let took = start.elapsed();
                        let who = (levels[seat] == b) as usize;
                        time[who] = (time[who].0 + took, time[who].1.max(took), time[who].2 + 1);
                        game.play(seat, placing).unwrap();
                    }
                    match game.outcome().unwrap().winner {
                        Some(w) => wins[(levels[w] == b) as usize] += 1,
                        None => draws += 1,
                    }
                }
                println!("{rule_set:?}, {a:?} vs {b:?} over {games} games: {} to {}, {draws} drawn", wins[0], wins[1]);
                for (i, level) in [a, b].iter().enumerate() {
                    let (total, slowest, moves) = time[i];
                    println!("  {level:?}: {moves} moves, average {:?}, slowest {slowest:?}", total / moves.max(1));
                }
            }
        }
    }
}

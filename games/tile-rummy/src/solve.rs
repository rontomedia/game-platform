//! The solver behind the bots: which of a player's tiles to add to the
//! table, and how to arrange the table into valid sets so that every tile
//! already on it is still used.
//!
//! Dynamic programming over the numbers 1 to 13, after J. N. van Rijn,
//! F. W. Takes and J. K. Vis, "The complexity of Rummikub problems" (2015).
//! Going up the numbers, the state is how long each color's unfinished runs
//! are (none, 1, 2, or 3 and more, which may end), so the work grows with
//! the numbers rather than with the tiles. Each number's tiles extend a
//! run, start one, or join a group of that number; jokers stand in for any
//! of them. Within a number the colors are decided one at a time, counting
//! what went to groups, and the groups are formed last.
//!
//! Limits: at most two runs of a color side by side, and two groups of a
//! number. With two of each tile, a third needs jokers, which is rare; a
//! table that needs one has no solution here, and the caller falls back.

use std::collections::HashMap;

use crate::tile::{Color, Counts, Tile, MAX_VALUE};

const VALUES: usize = MAX_VALUE as usize;
const COLORS: usize = Color::ALL.len();

/// What a solution is worth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Objective {
    /// The points of the optional tiles played, a joker counting as the
    /// number it stands for: for the initial meld, when nothing is required.
    Points,
    /// The number of optional tiles played, then their numbers. Each
    /// optional joker played adds `joker`: below zero to keep jokers unless
    /// they let more tiles go down.
    Tiles { joker: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Solution {
    /// Every required tile and the optional ones played, as valid sets.
    pub sets: Vec<Vec<Tile>>,
    /// The optional tiles played.
    pub played: Vec<Tile>,
    pub score: i32,
}

/// The best arrangement of all of `required` and any of `optional` into
/// valid sets, or `None` if `required` can't be arranged (see the limits
/// above).
pub(crate) fn solve(required: &[Tile], optional: &[Tile], objective: Objective) -> Option<Solution> {
    let problem = Problem::new(required, optional, objective);
    let mut solver = Solver { problem: &problem, memo: HashMap::new() };
    let start = Node { value: 0, color: 0, runs: 0, jokers: 0, ones: 0, twos: 0 };
    let score = solver.best(start)?;
    let sets = solver.sets(start);
    let played = Counts::of(sets.iter().flatten())
        .minus(&Counts::of(required))
        .expect("the sets hold every required tile")
        .tiles();
    Some(Solution { sets, played, score })
}

struct Problem {
    /// Tiles of each color and number that must be used...
    required: [[u8; VALUES]; COLORS],
    /// ...and that may be, required ones included.
    available: [[u8; VALUES]; COLORS],
    required_jokers: u8,
    jokers: u8,
    objective: Objective,
}

impl Problem {
    fn new(required: &[Tile], optional: &[Tile], objective: Objective) -> Problem {
        let mut problem = Problem {
            required: [[0; VALUES]; COLORS],
            available: [[0; VALUES]; COLORS],
            required_jokers: 0,
            jokers: 0,
            objective,
        };
        let tiles = required.iter().map(|&t| (t, true)).chain(optional.iter().map(|&t| (t, false)));
        for (tile, is_required) in tiles {
            match tile {
                Tile::Number { color, value } => {
                    let (c, v) = (color as usize, value as usize - 1);
                    problem.available[c][v] += 1;
                    problem.required[c][v] += is_required as u8;
                }
                Tile::Joker => {
                    problem.jokers += 1;
                    problem.required_jokers += is_required as u8;
                }
            }
        }
        problem
    }

    /// What an optional tile with the number at `value` (0-based) adds.
    fn number_weight(&self, value: usize) -> i32 {
        match self.objective {
            Objective::Points => value as i32 + 1,
            Objective::Tiles { .. } => 100 + value as i32 + 1,
        }
    }

    /// What a joker standing for a number at `value` adds.
    fn joker_weight(&self, value: usize) -> i32 {
        match self.objective {
            Objective::Points => value as i32 + 1,
            Objective::Tiles { .. } => 0,
        }
    }

    /// The score of the last step, past 13: `None` if a run is too short
    /// or a required joker unused.
    fn finish(&self, node: Node) -> Option<i32> {
        let runs_done = (0..COLORS).all(|c| lengths(node.runs, c).iter().all(|&l| l == 0 || l == 3));
        if !runs_done || node.jokers < self.required_jokers {
            return None;
        }
        Some(match self.objective {
            Objective::Points => 0,
            Objective::Tiles { joker } => (node.jokers - self.required_jokers) as i32 * joker,
        })
    }
}

/// A point in the search: deciding `color` at `value` (both 0-based;
/// `color == COLORS` is forming that number's groups).
#[derive(Clone, Copy, Debug)]
struct Node {
    value: usize,
    color: usize,
    /// For each color, the lengths of its two unfinished runs, shorter
    /// first (see `lengths`).
    runs: u16,
    /// Jokers used so far.
    jokers: u8,
    /// Colors at this number with one tile, and with two, in groups.
    ones: u8,
    twos: u8,
}

impl Node {
    fn key(self) -> u64 {
        (self.value as u64) << 40
            | (self.color as u64) << 32
            | (self.runs as u64) << 16
            | (self.jokers as u64) << 8
            | (self.ones as u64) << 4
            | self.twos as u64
    }
}

/// The lengths of a color's two runs: 0 (none), 1, 2, or 3 for three and
/// more.
fn lengths(runs: u16, color: usize) -> [u8; 2] {
    let bits = (runs >> (4 * color)) & 0xF;
    [(bits >> 2) as u8, (bits & 3) as u8]
}

fn with_lengths(runs: u16, color: usize, a: u8, b: u8) -> u16 {
    let (short, long) = if a <= b { (a, b) } else { (b, a) };
    runs & !(0xF << (4 * color)) | ((short << 2 | long) as u16) << (4 * color)
}

/// What a run slot gets at a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    /// Nothing: a run there ends, or no run starts.
    Empty,
    Number,
    Joker,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::Empty, Slot::Number, Slot::Joker];

    /// A run's length after this, or `None` if it would end too short.
    fn extend(self, length: u8) -> Option<u8> {
        match (self, length) {
            (Slot::Empty, 1 | 2) => None,
            (Slot::Empty, _) => Some(0),
            (_, length) => Some((length + 1).min(3)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Choice {
    /// A color at a number: what its two run slots get, and how many of its
    /// tiles go to groups.
    Color { slots: [Slot; 2], grouped: u8 },
    /// The number's groups, with this many jokers.
    Groups { jokers: u8 },
}

/// How groups form from `ones` colors with one tile, `twos` with two, and
/// `jokers`: how many groups, and how many of the ones and of the jokers go
/// to the first. `None` if they can't.
fn groups(ones: u8, twos: u8, jokers: u8) -> Option<(usize, u8, u8)> {
    let fits = |size: u8| (3..=4).contains(&size);
    if ones + twos + jokers == 0 {
        return Some((0, 0, 0));
    }
    if twos == 0 && ones > 0 && fits(ones + jokers) {
        return Some((1, ones, jokers));
    }
    for first_ones in 0..=ones {
        for first_jokers in 0..=jokers {
            if fits(twos + first_ones + first_jokers) && fits(twos + ones - first_ones + jokers - first_jokers) {
                return Some((2, first_ones, first_jokers));
            }
        }
    }
    None
}

struct Solver<'a> {
    problem: &'a Problem,
    memo: HashMap<u64, Option<i32>>,
}

impl Solver<'_> {
    /// The best score from `node` on, or `None` if nothing valid follows.
    fn best(&mut self, node: Node) -> Option<i32> {
        if node.value == VALUES {
            return self.problem.finish(node);
        }
        if let Some(&score) = self.memo.get(&node.key()) {
            return score;
        }
        let mut best = None;
        for (gain, next, _) in self.options(node) {
            if let Some(score) = self.best(next) {
                best = best.max(Some(gain + score));
            }
        }
        self.memo.insert(node.key(), best);
        best
    }

    /// Each way on from `node`: what it adds, where it leads, and what it
    /// is.
    fn options(&self, node: Node) -> Vec<(i32, Node, Choice)> {
        let p = self.problem;
        let mut options = Vec::new();
        if node.color == COLORS {
            for jokers in 0..=p.jokers - node.jokers {
                if groups(node.ones, node.twos, jokers).is_some() {
                    let next = Node {
                        value: node.value + 1,
                        color: 0,
                        jokers: node.jokers + jokers,
                        ones: 0,
                        twos: 0,
                        ..node
                    };
                    options.push((jokers as i32 * p.joker_weight(node.value), next, Choice::Groups { jokers }));
                }
            }
            return options;
        }
        let (c, v) = (node.color, node.value);
        let runs = lengths(node.runs, c);
        for a in Slot::ALL {
            for b in Slot::ALL {
                if runs[0] == runs[1] && a > b {
                    continue; // the same as the other way round
                }
                let (Some(la), Some(lb)) = (a.extend(runs[0]), b.extend(runs[1])) else {
                    continue;
                };
                let count = |slot| [a, b].iter().filter(|&&s| s == slot).count() as u8;
                let jokers = count(Slot::Joker);
                if node.jokers + jokers > p.jokers {
                    continue;
                }
                for grouped in 0..=2 {
                    let numbers = count(Slot::Number) + grouped;
                    if numbers < p.required[c][v] || numbers > p.available[c][v] {
                        continue;
                    }
                    let next = Node {
                        color: c + 1,
                        runs: with_lengths(node.runs, c, la, lb),
                        jokers: node.jokers + jokers,
                        ones: node.ones + (grouped == 1) as u8,
                        twos: node.twos + (grouped == 2) as u8,
                        ..node
                    };
                    let gain =
                        (numbers - p.required[c][v]) as i32 * p.number_weight(v) + jokers as i32 * p.joker_weight(v);
                    options.push((gain, next, Choice::Color { slots: [a, b], grouped }));
                }
            }
        }
        options
    }

    /// The sets of a best solution from `start`, which must have one.
    fn sets(&mut self, start: Node) -> Vec<Vec<Tile>> {
        let mut sets = Vec::new();
        // Each color's unfinished runs, in the order of `lengths`.
        let mut runs: [[Vec<Tile>; 2]; COLORS] = Default::default();
        let mut grouped = [0; COLORS];
        let mut node = start;
        while node.value < VALUES {
            let target = self.best(node);
            let (_, next, choice) = self
                .options(node)
                .into_iter()
                .find(|&(gain, next, _)| self.best(next).map(|s| gain + s) == target)
                .expect("the best score comes from one of the options");
            let value = node.value as u8 + 1;
            match choice {
                Choice::Color { slots, grouped: g } => {
                    let c = node.color;
                    let color = Color::ALL[c];
                    let before = lengths(node.runs, c);
                    for (i, slot) in slots.into_iter().enumerate() {
                        match slot {
                            Slot::Empty if before[i] > 0 => sets.push(std::mem::take(&mut runs[c][i])),
                            Slot::Empty => {}
                            Slot::Number => runs[c][i].push(Tile::Number { color, value }),
                            Slot::Joker => runs[c][i].push(Tile::Joker),
                        }
                    }
                    // Keep the runs in the order the state keeps their lengths.
                    let after = [slots[0].extend(before[0]), slots[1].extend(before[1])];
                    if after[0] > after[1] {
                        runs[c].swap(0, 1);
                    }
                    grouped[c] = g;
                }
                Choice::Groups { jokers } => {
                    let ones = grouped.iter().filter(|&&g| g == 1).count() as u8;
                    let twos = grouped.iter().filter(|&&g| g == 2).count() as u8;
                    let (count, first_ones, first_jokers) = groups(ones, twos, jokers).expect("only feasible groups");
                    let mut made = vec![Vec::new(); count];
                    let mut seen = 0;
                    for (c, &g) in grouped.iter().enumerate() {
                        let tile = Tile::Number { color: Color::ALL[c], value };
                        if g == 2 {
                            made[0].push(tile);
                            made[1].push(tile);
                        } else if g == 1 {
                            made[(seen >= first_ones) as usize].push(tile);
                            seen += 1;
                        }
                    }
                    for j in 0..jokers {
                        made[(j >= first_jokers) as usize].push(Tile::Joker);
                    }
                    sets.extend(made);
                }
            }
            node = next;
        }
        sets.extend(runs.into_iter().flatten().filter(|run| !run.is_empty()));
        sets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set::classify;
    use crate::tile::Color::*;

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    const J: Tile = Tile::Joker;
    const KEEP_JOKERS: Objective = Objective::Tiles { joker: -1 };

    fn sorted(mut tiles: Vec<Tile>) -> Vec<Tile> {
        tiles.sort_by_key(|t| format!("{t:?}"));
        tiles
    }

    /// Checks the solution is valid sets of all required tiles and the
    /// played ones; returns it.
    fn check(required: &[Tile], optional: &[Tile], objective: Objective) -> Solution {
        let solution = solve(required, optional, objective).expect("a solution");
        assert!(solution.sets.iter().all(|s| classify(s).is_some()), "{:?}", solution.sets);
        let all: Vec<Tile> = required.iter().chain(&solution.played).copied().collect();
        assert_eq!(sorted(solution.sets.concat()), sorted(all));
        assert!(Counts::of(optional).minus(&Counts::of(&solution.played)).is_some());
        solution
    }

    #[test]
    fn nothing_to_play() {
        let solution = check(&[], &[n(Red, 1), n(Blue, 5)], KEEP_JOKERS);
        assert!(solution.sets.is_empty());
        assert_eq!(solution.score, 0);
    }

    #[test]
    fn a_whole_run_and_two_groups() {
        let mut rack: Vec<Tile> = (1..=13).map(|v| n(Black, v)).collect();
        rack.extend([n(Red, 7), n(Blue, 7), n(Orange, 7), n(Red, 9), n(Blue, 9), n(Orange, 9)]);
        rack.push(n(Blue, 1)); // goes nowhere
        let solution = check(&[], &rack, KEEP_JOKERS);
        assert_eq!(solution.played.len(), rack.len() - 1);
    }

    #[test]
    fn two_of_each_in_parallel_runs_and_double_groups() {
        let rack = [n(Red, 3), n(Red, 3), n(Red, 4), n(Red, 4), n(Red, 5), n(Red, 5)];
        assert_eq!(check(&[], &rack, KEEP_JOKERS).played.len(), 6);
        let eights: Vec<Tile> = Color::ALL.iter().flat_map(|&c| [n(c, 8), n(c, 8)]).collect();
        assert_eq!(check(&[], &eights, KEEP_JOKERS).sets.len(), 2);
    }

    #[test]
    fn jokers_fill_gaps_only_when_that_helps() {
        let rack = [n(Red, 3), n(Red, 5), J];
        assert_eq!(check(&[], &rack, KEEP_JOKERS).played.len(), 3);
        // A run already there: the joker stays on the rack.
        let rack = [n(Red, 3), n(Red, 4), n(Red, 5), J];
        assert_eq!(sorted(check(&[], &rack, KEEP_JOKERS).played), sorted(rack[..3].to_vec()));
        // Unless it's worth playing it.
        assert_eq!(check(&[], &rack, Objective::Tiles { joker: 1000 }).played.len(), 4);
    }

    #[test]
    fn points_count_what_jokers_stand_for() {
        let rack = [n(Blue, 10), n(Blue, 11), J, n(Red, 1), n(Black, 1)];
        let solution = check(&[], &rack, Objective::Points);
        // 10-11-12 (or 9-10-11 and the like) beats 1-1-joker.
        assert_eq!(solution.score, 33);
    }

    #[test]
    fn rearranging_the_table() {
        // A red 5 on the rack: 3-4-5-6-7 splits into 3-4-5 and 5-6-7.
        let table: Vec<Tile> = (3..=7).map(|v| n(Red, v)).collect();
        assert_eq!(check(&table, &[n(Red, 5)], KEEP_JOKERS).played, vec![n(Red, 5)]);
        // A joker taken from a run, replaced by the real tile, goes into a group.
        let table = [n(Red, 3), J, n(Red, 5)];
        let rack = [n(Red, 4), n(Blue, 9), n(Black, 9)];
        assert_eq!(check(&table, &rack, KEEP_JOKERS).played.len(), 3);
    }

    #[test]
    fn required_tiles_must_all_be_used() {
        assert_eq!(solve(&[n(Red, 3), n(Red, 4)], &[n(Blue, 9)], KEEP_JOKERS), None);
        // With the rack's red 5 they can.
        assert_eq!(check(&[n(Red, 3), n(Red, 4)], &[n(Red, 5)], KEEP_JOKERS).played, vec![n(Red, 5)]);
        // A table joker counts as required.
        assert_eq!(check(&[n(Red, 3), J, n(Red, 5)], &[], KEEP_JOKERS).score, 0);
    }

    #[test]
    fn a_full_table() {
        // Every tile but the jokers lies on the table as runs 1-13; the rack
        // has both jokers.
        let table: Vec<Tile> =
            Color::ALL.iter().flat_map(|&c| (1..=13).flat_map(move |v| [n(c, v), n(c, v)])).collect();
        let solution = check(&table, &[J, J], KEEP_JOKERS);
        assert!(solution.played.is_empty());
        assert_eq!(check(&table, &[J, J], Objective::Tiles { joker: 1000 }).played, vec![J, J]);
    }
}

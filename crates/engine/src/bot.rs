//! Computer players: the move a bot makes, from what its player sees.
//!
//! Every level makes the initial meld as soon as its rack allows and,
//! once melded, plays as many tiles as it can find a place for. They
//! differ in how much of the table they look at, as players do:
//! - `Easy` puts down new sets from its rack, then adds single tiles to
//!   the ends of runs and to groups. Now and then (`EASY_SPOTS`) it also
//!   spots a rearrangement of one of the table's sets, as Medium would.
//! - `Medium` also makes small rearrangements, as a good player spots
//!   them: it changes at most `MEDIUM_REACH` of the table's sets (splitting
//!   a run, moving a tile from one set to another, freeing a joker).
//! - `Hard` rearranges the whole table (see `solve`), leaving alone each
//!   set it doesn't need to change.
//!
//! None holds tiles back on purpose. A joker is played only when it lets
//! more tiles go down, or to go out.

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::game::PlayerView;
use crate::set::{classify, points};
use crate::solve::{solve, Objective, Solution};
use crate::tile::{Counts, Tile};
use crate::turn::check_turn;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum BotLevel {
    Easy,
    Medium,
    Hard,
}

/// A turn: the new table, or drawing a tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Move {
    Play(Vec<Vec<Tile>>),
    Draw,
}

/// Keeps a joker on the rack unless it lets more tiles go down.
const KEEP_JOKERS: Objective = Objective::Tiles { joker: -1 };
/// Plays jokers too: only taken when it empties the rack.
const GO_OUT: Objective = Objective::Tiles { joker: 1000 };
/// How many of the table's sets a Medium bot may change in a turn.
const MEDIUM_REACH: usize = 2;
/// On how many turns in `EASY_SPOTS.1` an Easy bot looks for a change to
/// one of the table's sets.
const EASY_SPOTS: (u64, u64) = (1, 2);

/// The bot's move for the player whose view this is, on their turn.
pub fn bot_move(view: &PlayerView, level: BotLevel) -> Move {
    let melded = view.melded[view.you];
    let table = if !melded {
        initial_meld(view)
    } else {
        match level {
            BotLevel::Easy if spots(view) => rearrange_a_little(view, 1),
            BotLevel::Easy => add_to_table(view),
            BotLevel::Medium => rearrange_a_little(view, MEDIUM_REACH),
            BotLevel::Hard => rearrange(view).or_else(|| add_to_table(view)),
        }
    };
    let valid = |table: &Vec<Vec<Tile>>| {
        let ok = check_turn(&view.table, table, &view.rack, melded, &view.rules).is_ok();
        debug_assert!(ok, "a bot's play doesn't check: {table:?}");
        ok
    };
    match table.filter(valid) {
        Some(table) => Move::Play(table),
        None => Move::Draw,
    }
}

/// New sets from the rack worth the initial meld: the most tiles if that's
/// enough, else the most points.
fn initial_meld(view: &PlayerView) -> Option<Vec<Vec<Tile>>> {
    let worth = |sets: &[Vec<Tile>]| sets.iter().map(|set| points(set).unwrap_or(0)).sum::<u32>();
    [KEEP_JOKERS, Objective::Points]
        .into_iter()
        .filter_map(|objective| solve(&[], &view.rack, objective))
        .find(|solution| worth(&solution.sets) >= view.rules.initial_meld)
        .map(|solution| [view.table.clone(), solution.sets].concat())
}

/// Easy, once melded: new sets from the rack, then single tiles added to
/// the table's sets. `None` if nothing fits.
fn add_to_table(view: &PlayerView) -> Option<Vec<Vec<Tile>>> {
    let (_, solution) = most_tiles(&[], &view.rack)?;
    let rest = without(&view.rack, &solution.played);
    let table = with_singles([view.table.clone(), solution.sets].concat(), rest);
    (played(view, &table) > 0).then_some(table)
}

/// Whether Easy looks further this turn: on `EASY_SPOTS` of positions,
/// chosen by a hash of what it sees, so the same position gets the same
/// move (the engine has no randomness of its own).
fn spots(view: &PlayerView) -> bool {
    // FNV-1a, the same on every platform and Rust version.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let byte = |tile: &Tile| match *tile {
        Tile::Number { color, value } => color as u8 * 16 + value,
        Tile::Joker => 255,
    };
    let mut rack = view.rack.clone();
    rack.sort_by_key(byte);
    let table = view.table.iter().flatten().map(byte);
    for b in rack.iter().map(byte).chain([254]).chain(table).chain([view.pool as u8]) {
        hash = (hash ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    hash % EASY_SPOTS.1 < EASY_SPOTS.0
}

/// Medium, once melded (and Easy now and then): the best play that changes
/// at most `reach` of the table's sets (the rest left as they are), then
/// single tiles added; at least as good as Easy's. `None` if nothing fits.
fn rearrange_a_little(view: &PlayerView, reach: usize) -> Option<Vec<Vec<Tile>>> {
    let mut best = add_to_table(view);
    let mut most = best.as_ref().map_or(0, |table| played(view, table));
    for chosen in choices(view.table.len(), reach) {
        let loose: Vec<Tile> = chosen.iter().flat_map(|&i| view.table[i].iter().copied()).collect();
        let Some((_, solution)) = most_tiles(&loose, &view.rack) else { continue };
        let kept = view.table.iter().enumerate().filter(|(i, _)| !chosen.contains(i)).map(|(_, set)| set.clone());
        let rest = without(&view.rack, &solution.played);
        let table = with_singles(kept.chain(solution.sets).collect(), rest);
        let count = played(view, &table);
        if count > most {
            (best, most) = (Some(table), count);
        }
    }
    best
}

/// Each choice of 1 to `reach` of `n` sets, fewest first (so on a tie the
/// fewer win).
fn choices(n: usize, reach: usize) -> Vec<Vec<usize>> {
    let mut all = Vec::new();
    let mut layer: Vec<Vec<usize>> = vec![Vec::new()];
    for _ in 0..reach {
        layer = layer
            .iter()
            .flat_map(|chosen| {
                let from = chosen.last().map_or(0, |&last| last + 1);
                (from..n).map(move |i| [chosen.as_slice(), &[i]].concat())
            })
            .collect();
        all.extend(layer.iter().cloned());
    }
    all
}

/// `table` with single tiles of `rest` added to its sets; jokers only if
/// that empties the rack.
fn with_singles(mut table: Vec<Vec<Tile>>, rest: Vec<Tile>) -> Vec<Vec<Tile>> {
    let (jokers, numbers): (Vec<Tile>, Vec<Tile>) = rest.into_iter().partition(|&t| t == Tile::Joker);
    let numbers = add_singles(&mut table, numbers);
    if numbers.is_empty() && !jokers.is_empty() {
        let mut out = table.clone();
        if add_singles(&mut out, jokers).is_empty() {
            return out;
        }
    }
    table
}

/// Tiles `table` has beyond the view's table: those played from the rack.
fn played(view: &PlayerView, table: &[Vec<Tile>]) -> usize {
    let size = |table: &[Vec<Tile>]| table.iter().map(Vec::len).sum::<usize>();
    size(table).saturating_sub(size(&view.table))
}

/// Hard, once melded: the whole table rearranged to play the most tiles.
/// `None` if nothing fits, or the table is beyond the solver.
fn rearrange(view: &PlayerView) -> Option<Vec<Vec<Tile>>> {
    let all: Vec<Tile> = view.table.iter().flatten().copied().collect();
    let (objective, mut best) = most_tiles(&all, &view.rack)?;
    if best.played.is_empty() {
        return None;
    }
    // Keep each of the table's sets as it is if the same is possible
    // without touching it, so the others see only what changed.
    let mut kept = vec![false; view.table.len()];
    for i in 0..view.table.len() {
        let loose: Vec<Tile> =
            (0..view.table.len()).filter(|&j| j != i && !kept[j]).flat_map(|j| view.table[j].iter().copied()).collect();
        if let Some(solution) = solve(&loose, &view.rack, objective).filter(|s| s.score == best.score) {
            kept[i] = true;
            best = solution;
        }
    }
    let unchanged = view.table.iter().zip(&kept).filter(|(_, &k)| k).map(|(set, _)| set.clone());
    Some(unchanged.chain(best.sets).collect())
}

/// The solution playing the most of `rack`, keeping jokers unless they help
/// or the rack can then be emptied; with the objective it was found with.
fn most_tiles(required: &[Tile], rack: &[Tile]) -> Option<(Objective, Solution)> {
    let keep = solve(required, rack, KEEP_JOKERS)?;
    let rest = without(rack, &keep.played);
    if !rest.is_empty() && rest.iter().all(|&t| t == Tile::Joker) {
        if let Some(out) = solve(required, rack, GO_OUT).filter(|s| s.played.len() == rack.len()) {
            return Some((GO_OUT, out));
        }
    }
    Some((KEEP_JOKERS, keep))
}

/// Adds each of `tiles` it can to an end of a run or to a group on
/// `table`, until no more fit; returns the rest.
fn add_singles(table: &mut [Vec<Tile>], mut tiles: Vec<Tile>) -> Vec<Tile> {
    loop {
        let before = tiles.len();
        tiles.retain(|&tile| !table.iter_mut().any(|set| add_single(set, tile)));
        if tiles.len() == before {
            return tiles;
        }
    }
}

fn add_single(set: &mut Vec<Tile>, tile: Tile) -> bool {
    for at in [set.len(), 0] {
        set.insert(at, tile);
        if classify(set).is_some() {
            return true;
        }
        set.remove(at);
    }
    false
}

/// `tiles` without `taken`, which must be among them.
fn without(tiles: &[Tile], taken: &[Tile]) -> Vec<Tile> {
    Counts::of(tiles).minus(&Counts::of(taken)).expect("taken from the tiles").tiles()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Game, Rules};
    use crate::tile::Color::{self, *};

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    const J: Tile = Tile::Joker;

    fn view(table: Vec<Vec<Tile>>, rack: Vec<Tile>, melded: bool) -> PlayerView {
        PlayerView {
            you: 0,
            rules: Rules::default(),
            rack_sizes: vec![rack.len(), 14],
            rack,
            table,
            melded: vec![melded, true],
            pool: 40,
            current: 0,
            outcome: None,
        }
    }

    fn played(table: &[Vec<Tile>], m: &Move) -> usize {
        match m {
            Move::Play(after) => after.iter().map(Vec::len).sum::<usize>() - table.iter().map(Vec::len).sum::<usize>(),
            Move::Draw => 0,
        }
    }

    #[test]
    fn initial_meld_keeps_the_joker_when_it_can() {
        let rack = vec![n(Blue, 10), n(Blue, 11), n(Blue, 12), J, n(Red, 1)];
        let m = bot_move(&view(vec![], rack, false), BotLevel::Easy);
        assert_eq!(m, Move::Play(vec![vec![n(Blue, 10), n(Blue, 11), n(Blue, 12)]]));
    }

    #[test]
    fn initial_meld_uses_the_joker_when_it_must() {
        let rack = vec![n(Blue, 9), n(Blue, 11), J, n(Red, 1)];
        let m = bot_move(&view(vec![], rack, false), BotLevel::Easy);
        assert_eq!(m, Move::Play(vec![vec![n(Blue, 9), J, n(Blue, 11)]]));
        // Not enough even with it.
        let rack = vec![n(Blue, 1), n(Blue, 2), J];
        assert_eq!(bot_move(&view(vec![], rack, false), BotLevel::Hard), Move::Draw);
    }

    #[test]
    fn initial_meld_leaves_the_table_alone() {
        let table = vec![vec![n(Red, 3), n(Red, 4), n(Red, 5)]];
        let rack = vec![n(Red, 6), n(Red, 7), n(Black, 10), n(Blue, 10), n(Orange, 10)];
        let m = bot_move(&view(table.clone(), rack, false), BotLevel::Hard);
        let new = vec![n(Black, 10), n(Blue, 10), n(Orange, 10)];
        assert_eq!(m, Move::Play(vec![table[0].clone(), new]));
    }

    #[test]
    fn easy_adds_single_tiles_to_runs_and_groups() {
        let table = vec![vec![n(Red, 3), n(Red, 4), n(Red, 5)], vec![n(Red, 9), n(Blue, 9), n(Black, 9)]];
        let rack = vec![n(Red, 7), n(Red, 6), n(Red, 2), n(Orange, 9), n(Blue, 1)];
        let m = bot_move(&view(table.clone(), rack, true), BotLevel::Easy);
        let run: Vec<Tile> = (2..=7).map(|v| n(Red, v)).collect();
        assert_eq!(m, Move::Play(vec![run, vec![n(Red, 9), n(Blue, 9), n(Black, 9), n(Orange, 9)]]));
    }

    #[test]
    fn hard_rearranges_where_easy_cannot() {
        // A red 5 splits 3-4-5-6-7 into 3-4-5 and 5-6-7.
        let run: Vec<Tile> = (3..=7).map(|v| n(Red, v)).collect();
        let v = view(vec![run], vec![n(Red, 5), n(Blue, 2)], true);
        assert!(!spots(&v), "a position where Easy doesn't look further");
        assert_eq!(bot_move(&v, BotLevel::Easy), Move::Draw);
        assert_eq!(played(&v.table, &bot_move(&v, BotLevel::Hard)), 1);
    }

    #[test]
    fn easy_spots_a_split_now_and_then() {
        // The same split, from racks differing in a tile that can't be
        // played: Easy finds it on some of them, about `EASY_SPOTS`.
        let run: Vec<Tile> = (3..=7).map(|v| n(Red, v)).collect();
        let racks = [Black, Blue, Orange].into_iter().flat_map(|c| (1..=13).map(move |v| vec![n(Red, 5), n(c, v)]));
        let mut found = 0;
        for rack in racks {
            let v = view(vec![run.clone()], rack.clone(), true);
            let count = played(&v.table, &bot_move(&v, BotLevel::Easy));
            assert_eq!(count, spots(&v) as usize, "{rack:?}");
            found += count;
        }
        assert!((10..30).contains(&found), "{found} of 39");
    }

    #[test]
    fn hard_leaves_sets_it_needs_not_change() {
        let untouched = vec![vec![n(Blue, 1), n(Blue, 2), n(Blue, 3)], vec![n(Black, 11), n(Blue, 11), n(Red, 11)]];
        let run: Vec<Tile> = (3..=7).map(|v| n(Red, v)).collect();
        let table = [untouched.clone(), vec![run]].concat();
        let Move::Play(after) = bot_move(&view(table, vec![n(Red, 5)], true), BotLevel::Hard) else { panic!("a play") };
        assert_eq!(after[..2], untouched[..], "kept as they were, in their order");
        assert_eq!(after.len(), 4);
    }

    #[test]
    fn jokers_go_down_to_go_out() {
        let table = vec![vec![n(Red, 3), n(Red, 4), n(Red, 5)]];
        for level in [BotLevel::Easy, BotLevel::Hard] {
            let m = bot_move(&view(table.clone(), vec![n(Red, 6), J], true), level);
            assert_eq!(played(&table, &m), 2, "{level:?}");
            // Not otherwise.
            let m = bot_move(&view(table.clone(), vec![n(Red, 6), J, n(Blue, 1)], true), level);
            assert_eq!(played(&table, &m), 1, "{level:?}");
        }
    }

    fn play(game: &mut Game, levels: &[BotLevel]) {
        let player = game.current();
        match bot_move(&game.view(player), levels[player]) {
            Move::Play(table) => game.play(player, table).expect("bots only make valid plays"),
            Move::Draw => {
                game.draw(player).unwrap();
            }
        }
    }

    #[test]
    fn bots_play_whole_games() {
        let mut gone_out = 0;
        for seed in 0..12 {
            let players = 2 + seed as usize % 3;
            let levels: Vec<BotLevel> = (0..players)
                .map(|p| if (p + seed as usize).is_multiple_of(2) { BotLevel::Easy } else { BotLevel::Hard })
                .collect();
            let mut game = Game::new(players, Rules::default(), seed);
            let mut turns = 0;
            while game.outcome().is_none() {
                play(&mut game, &levels);
                turns += 1;
                assert!(turns < 1000, "seed {seed} doesn't end");
            }
            gone_out += (0..players).any(|p| game.rack(p).is_empty()) as usize;
        }
        assert!(gone_out > 6, "most games end with someone going out: {gone_out} of 12");
    }

    #[test]
    fn medium_makes_small_rearrangements() {
        // A red 5 splits 3-4-5-6-7 into 3-4-5 and 5-6-7: one set changed.
        let run: Vec<Tile> = (3..=7).map(|v| n(Red, v)).collect();
        let v = view(vec![run.clone()], vec![n(Red, 5), n(Blue, 1)], true);
        assert_eq!(played(&v.table, &bot_move(&v, BotLevel::Medium)), 1);
        // An orange 1 makes a group with the 1s taken from two runs: two
        // sets changed, as far as Medium looks.
        let runs = vec![(1..=4).map(|v| n(Red, v)).collect(), (1..=4).map(|v| n(Blue, v)).collect()];
        let v = view(runs, vec![n(Orange, 1)], true);
        assert_eq!(played(&v.table, &bot_move(&v, BotLevel::Medium)), 1);
    }

    #[test]
    fn medium_sits_between_easy_and_hard() {
        // On every turn of some whole games: Easy plays no more than Medium,
        // Medium no more than Hard, and each is beaten sometimes.
        let (mut medium_better, mut hard_better) = (0, 0);
        for seed in 0..3 {
            let mut game = Game::new(3, Rules::default(), seed);
            let levels = [BotLevel::Hard; 3];
            while game.outcome().is_none() {
                let view = game.view(game.current());
                let count = |level| played(&view.table, &bot_move(&view, level));
                let (easy, medium, hard) = (count(BotLevel::Easy), count(BotLevel::Medium), count(BotLevel::Hard));
                assert!(easy <= medium && medium <= hard, "seed {seed}: {easy}, {medium}, {hard}");
                medium_better += (medium > easy) as usize;
                hard_better += (hard > medium) as usize;
                play(&mut game, &levels);
            }
        }
        assert!(medium_better > 0 && hard_better > 0, "{medium_better}, {hard_better}");
    }

    /// How the levels fare against each other, and how long a move takes:
    /// `cargo test -p engine --release -- --ignored --nocapture strength`.
    #[test]
    #[ignore]
    fn strength() {
        use BotLevel::*;
        let games = 200;
        for (a, b) in [(Hard, Easy), (Medium, Easy), (Hard, Medium)] {
            let mut wins = [0.0; 2];
            let mut slowest = [std::time::Duration::ZERO; 2];
            let mut total = [std::time::Duration::ZERO; 2];
            let mut moves = [0u32; 2];
            for seed in 0..games {
                // Each sits first in half the games.
                let levels = if seed % 2 == 0 { [a, b] } else { [b, a] };
                let mut game = Game::new(2, Rules::default(), seed);
                while game.outcome().is_none() {
                    let who = (levels[game.current()] == b) as usize;
                    let start = std::time::Instant::now();
                    play(&mut game, &levels);
                    let took = start.elapsed();
                    slowest[who] = slowest[who].max(took);
                    total[who] += took;
                    moves[who] += 1;
                }
                let winners = &game.outcome().unwrap().winners;
                for &w in winners {
                    wins[(levels[w] == b) as usize] += 1.0 / winners.len() as f64;
                }
            }
            println!("{a:?} vs {b:?} over {games} two-player games: {:.0} to {:.0}", wins[0], wins[1]);
            for (i, level) in [a, b].iter().enumerate() {
                println!("  {level:?}: {} moves, average {:?}, slowest {:?}", moves[i], total[i] / moves[i], slowest[i]);
            }
        }
    }
}

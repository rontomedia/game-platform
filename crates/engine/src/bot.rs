//! Computer players: the move a bot makes, from what its player sees.
//!
//! Both levels make the initial meld as soon as their rack allows and,
//! once melded, play as many tiles as they can find a place for:
//! - `Easy` doesn't rearrange the table: it puts down new sets from its
//!   rack, then adds single tiles to the ends of runs and to groups.
//! - `Hard` rearranges the whole table (see `solve`), leaving alone each
//!   set it doesn't need to change.
//!
//! Neither holds tiles back on purpose. A joker is played only when it lets
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

/// The bot's move for the player whose view this is, on their turn.
pub fn bot_move(view: &PlayerView, level: BotLevel) -> Move {
    let melded = view.melded[view.you];
    let table = if !melded {
        initial_meld(view)
    } else if level == BotLevel::Hard {
        rearrange(view).or_else(|| add_to_table(view))
    } else {
        add_to_table(view)
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
    let mut table = [view.table.clone(), solution.sets].concat();
    let rest = without(&view.rack, &solution.played);
    let (jokers, numbers): (Vec<Tile>, Vec<Tile>) = rest.into_iter().partition(|&t| t == Tile::Joker);
    let numbers = add_singles(&mut table, numbers);
    if numbers.is_empty() && !jokers.is_empty() {
        // Only jokers left: play them too if that empties the rack.
        let mut out = table.clone();
        if add_singles(&mut out, jokers).is_empty() {
            table = out;
        }
    }
    let size = |table: &[Vec<Tile>]| table.iter().map(Vec::len).sum::<usize>();
    (size(&table) > size(&view.table)).then_some(table)
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
        let v = view(vec![run], vec![n(Red, 5), n(Blue, 1)], true);
        assert_eq!(bot_move(&v, BotLevel::Easy), Move::Draw);
        assert_eq!(played(&v.table, &bot_move(&v, BotLevel::Hard)), 1);
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

    /// How the levels fare against each other, and how long a move takes:
    /// `cargo test -p engine --release -- --ignored --nocapture strength`.
    #[test]
    #[ignore]
    fn strength() {
        let games = 200;
        let mut wins = [0.0; 2];
        let mut slowest = std::time::Duration::ZERO;
        let mut total = std::time::Duration::ZERO;
        let mut moves = 0;
        for seed in 0..games {
            // Hard sits first in half the games.
            let levels =
                if seed % 2 == 0 { [BotLevel::Hard, BotLevel::Easy] } else { [BotLevel::Easy, BotLevel::Hard] };
            let mut game = Game::new(2, Rules::default(), seed);
            while game.outcome().is_none() {
                let start = std::time::Instant::now();
                play(&mut game, &levels);
                let took = start.elapsed();
                slowest = slowest.max(took);
                total += took;
                moves += 1;
            }
            let winners = &game.outcome().unwrap().winners;
            for &w in winners {
                wins[(levels[w] == BotLevel::Easy) as usize] += 1.0 / winners.len() as f64;
            }
        }
        println!("Hard vs Easy over {games} two-player games: Hard won {:.0}, Easy {:.0}", wins[0], wins[1]);
        println!("moves: {moves}, average {:?}, slowest {slowest:?}", total / moves);
    }
}

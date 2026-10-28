//! Extra Hard: Hard's solver, plus a look past the turn.
//!
//! Hard plays the most tiles it can, whatever that leaves on its rack.
//! Extra Hard also weighs the rack a play leaves: tiles that wait on each
//! other (5-6 of a colour, two 8s of different colours) are worth keeping
//! if the tiles they wait for can still come, which it knows by counting
//! the tiles it has seen (`unseen`). Its options are Hard's play, the
//! plays that keep back one of the tiles Hard would play, and drawing.
//!
//! When another player is down to their last tile or two (`DANGER`), it
//! also weighs how likely each option is to let them go out next turn:
//! it imagines racks they could hold (unseen tiles that couldn't all go on
//! the table as they left it, or they'd have gone out) and counts how many
//! could all go on the table it would leave.

use super::{add_to_table, position_hash, rearrange_with, without};
use crate::game::PlayerView;
use crate::rng::Rng;
use crate::solve::{solve, Objective};
use crate::tile::{full_set, Color, Counts, Tile, MAX_VALUE};

/// What a play is worth, in points per thing.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Weights {
    /// Each tile played.
    pub tile: i32,
    /// Each tile still to come that would make a set with two on the rack.
    pub wait: i32,
    /// Each point left on the rack (what it costs if someone goes out).
    pub point: i32,
    /// Letting a player in danger go out, for certain (scaled by how
    /// likely it is).
    pub risk: i32,
    /// Each point left on the rack while someone is in danger.
    pub danger_point: i32,
}

pub(crate) const WEIGHTS: Weights = Weights { tile: 100, wait: 1, point: 1, risk: 20000, danger_point: 3 };

/// Another player with this many tiles or fewer may go out next turn.
const DANGER: usize = 2;
/// Racks imagined for each player in danger.
const SAMPLES: usize = 40;

/// Extra Hard, once melded: the best of its options; `None` to draw.
pub(super) fn extra_hard(view: &PlayerView) -> Option<Vec<Vec<Tile>>> {
    extra_hard_with(view, WEIGHTS)
}

pub(crate) fn extra_hard_with(view: &PlayerView, weights: Weights) -> Option<Vec<Vec<Tile>>> {
    // Beyond the solver (rare): as Easy.
    let Some(hard) = rearrange_with(&view.table, &view.rack) else { return add_to_table(view) };
    let rest = left(view, &hard);
    if rest.is_empty() {
        return Some(hard); // going out
    }
    let unseen = unseen(view);
    let threats = threats(view, &unseen);
    let mut kinds = without(&view.rack, &rest);
    kinds.dedup(); // in a fixed order, so equal tiles are together
    let held = kinds.iter().filter_map(|&tile| rearrange_with(&view.table, &without(&view.rack, &[tile])));
    let options = std::iter::once(Some(hard)).chain(held.map(Some)).chain(std::iter::once(None));
    let mut best: Option<(f64, Option<Vec<Vec<Tile>>>)> = None;
    for option in options {
        let table = option.as_deref().unwrap_or(&view.table);
        let value = worth(view, table, &unseen, &threats, weights);
        if best.as_ref().is_none_or(|(most, _)| value > *most) {
            best = Some((value, option));
        }
    }
    best.and_then(|(_, option)| option)
}

/// What leaving `table` is worth (`view.table`: drawing).
fn worth(view: &PlayerView, table: &[Vec<Tile>], unseen: &Counts, threats: &[Vec<Vec<Tile>>], weights: Weights) -> f64 {
    let rest = left(view, table);
    let played = view.rack.len() - rest.len();
    let points: u32 = rest.iter().map(|t| t.penalty(view.rules.joker_penalty)).sum();
    let point = if threats.is_empty() { weights.point } else { weights.danger_point };
    let value = weights.tile * played as i32 + weights.wait * waits(&rest, unseen) as i32 - point * points as i32;
    value as f64 - weights.risk as f64 * threats.iter().map(|racks| risk(table, racks)).sum::<f64>()
}

/// For each other player in danger, racks they could hold.
fn threats(view: &PlayerView, unseen: &Counts) -> Vec<Vec<Vec<Tile>>> {
    let others = (0..view.rack_sizes.len()).filter(|&p| Some(p) != view.you);
    let near = others.filter(|&p| view.melded[p] && (1..=DANGER).contains(&view.rack_sizes[p]));
    let mut rng = Rng::new(position_hash(view));
    let pool = unseen.tiles();
    near.map(|p| racks(&view.table, &pool, view.rack_sizes[p], &mut rng)).filter(|racks| !racks.is_empty()).collect()
}

/// Up to `SAMPLES` racks of `size` from `pool` that couldn't all go on
/// `table` (or their holder would have gone out).
fn racks(table: &[Vec<Tile>], pool: &[Tile], size: usize, rng: &mut Rng) -> Vec<Vec<Tile>> {
    let mut racks = Vec::new();
    for _ in 0..SAMPLES * 4 {
        if racks.len() == SAMPLES || pool.len() < size {
            break;
        }
        let mut pool = pool.to_vec();
        let rack: Vec<Tile> = (0..size).map(|_| pool.swap_remove(rng.below(pool.len()))).collect();
        if !goes_out(table, &rack) {
            racks.push(rack);
        }
    }
    racks
}

/// The share of `racks` that could all go on `table`.
fn risk(table: &[Vec<Tile>], racks: &[Vec<Tile>]) -> f64 {
    racks.iter().filter(|rack| goes_out(table, rack)).count() as f64 / racks.len() as f64
}

/// Whether all of `rack` can go on `table`, rearranged as needed.
fn goes_out(table: &[Vec<Tile>], rack: &[Tile]) -> bool {
    let all: Vec<Tile> = table.iter().flatten().chain(rack).copied().collect();
    solve(&all, &[], Objective::Tiles { joker: 0 }).is_some()
}

/// The rack after playing `table`.
fn left(view: &PlayerView, table: &[Vec<Tile>]) -> Vec<Tile> {
    let played = Counts::of(table.iter().flatten()).minus(&Counts::of(view.table.iter().flatten())).expect("a play adds tiles");
    Counts::of(&view.rack).minus(&played).expect("played from the rack").tiles()
}

/// Tiles this player hasn't seen: in the pool or on the others' racks.
fn unseen(view: &PlayerView) -> Counts {
    let seen = Counts::of(view.table.iter().flatten().chain(&view.rack));
    Counts::of(&full_set()).minus(&seen).expect("every tile seen is one of the set")
}

/// How many unseen tiles would each make a set with two tiles of `rack`:
/// the ends of two in a row of a colour, the middle of a gap, the other
/// colours of a pair of numbers. Counted for each pair, so a tile two
/// pairs wait for counts twice.
fn waits(rack: &[Tile], unseen: &Counts) -> u32 {
    let mut numbers: Vec<(Color, u8)> = rack
        .iter()
        .filter_map(|&t| match t {
            Tile::Number { color, value } => Some((color, value)),
            Tile::Joker => None,
        })
        .collect();
    numbers.sort_by_key(|&(color, value)| (color as u8, value));
    numbers.dedup();
    let count = |color: Color, value: i32| {
        if (1..=MAX_VALUE as i32).contains(&value) {
            unseen.count(Tile::Number { color, value: value as u8 })
        } else {
            0
        }
    };
    let mut total = 0;
    for (i, &(c1, v1)) in numbers.iter().enumerate() {
        for &(c2, v2) in &numbers[i + 1..] {
            let (lo, hi) = (v1.min(v2) as i32, v1.max(v2) as i32);
            if c1 == c2 && hi - lo == 1 {
                total += count(c1, lo - 1) + count(c1, hi + 1);
            } else if c1 == c2 && hi - lo == 2 {
                total += count(c1, lo + 1);
            } else if c1 != c2 && v1 == v2 {
                total += Color::ALL.iter().filter(|&&c| c != c1 && c != c2).map(|&c| count(c, v1 as i32)).sum::<u32>();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::super::{bot_move, initial_meld, BotLevel, Move};
    use super::*;
    use crate::game::{Game, Rules};
    use crate::tile::Color::*;

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }

    fn view(table: Vec<Vec<Tile>>, rack: Vec<Tile>) -> PlayerView {
        PlayerView {
            you: Some(0),
            rules: Rules::default(),
            rack_sizes: vec![rack.len(), 14],
            rack,
            table,
            melded: vec![true, true],
            pool: 40,
            current: 0,
            outcome: None,
        }
    }

    #[test]
    fn waiting_tiles_count_only_while_they_can_come() {
        let rack = [n(Red, 5), n(Red, 6), n(Blue, 9)];
        let all = Counts::of(&full_set());
        // A red 4 or 7, two of each.
        assert_eq!(waits(&rack, &all), 4);
        // Both red 4s seen: only the 7s are left.
        let seen = Counts::of(&[n(Red, 4), n(Red, 4)]);
        assert_eq!(waits(&rack, &all.minus(&seen).unwrap()), 2);
    }

    #[test]
    fn of_two_plays_as_big_the_one_leaving_tiles_that_wait_is_worth_more() {
        let v = view(vec![vec![n(Red, 7), n(Red, 8), n(Red, 9)]], vec![n(Red, 6), n(Red, 10), n(Red, 11), n(Blue, 1)]);
        let unseen = unseen(&v);
        // Keeps 10-11, waiting for the last red 9 and two red 12s...
        let low = [vec![n(Red, 6), n(Red, 7), n(Red, 8), n(Red, 9)]];
        // ...or 6 and 11, waiting for nothing.
        let high = [vec![n(Red, 7), n(Red, 8), n(Red, 9), n(Red, 10)]];
        assert_eq!(waits(&left(&v, &low), &unseen), 3);
        // (Points left on the rack aside: the low play leaves 3 more.)
        let weights = Weights { point: 0, ..WEIGHTS };
        assert!(worth(&v, &low, &unseen, &[], weights) > worth(&v, &high, &unseen, &[], weights));
    }

    #[test]
    fn it_plays_whole_games() {
        for seed in 0..2 {
            let mut game = Game::new(3, Rules::default(), seed);
            let levels = [BotLevel::ExtraHard, BotLevel::Hard, BotLevel::ExtraHard];
            while game.outcome().is_none() {
                let who = game.current();
                match bot_move(&game.view(who), levels[who]) {
                    Move::Play(table) => game.play(who, table).expect("only valid plays"),
                    Move::Draw => {
                        game.draw(who).unwrap();
                    }
                }
            }
        }
    }

    /// Extra Hard with `weights`, as `bot_move` plays it.
    fn extra_move(view: &PlayerView, weights: Weights) -> Move {
        let table = if !view.you.is_some_and(|you| view.melded[you]) {
            initial_meld(view)
        } else {
            extra_hard_with(view, weights)
        };
        table.map_or(Move::Draw, Move::Play)
    }

    #[test]
    fn it_does_not_open_the_table_for_a_player_about_to_go_out() {
        // The other player has one tile left, which can't go on the black
        // 1-2-3 (or they'd have gone out). A blue 4-5-6 on the table would
        // let a blue 3 or 7 go out; drawing opens nothing.
        let table = vec![vec![n(Black, 1), n(Black, 2), n(Black, 3)]];
        let rack = vec![n(Blue, 4), n(Blue, 5), n(Blue, 6), n(Red, 9), n(Orange, 11)];
        let mut v = view(table, rack);
        v.rack_sizes = vec![5, 1];
        let threats = threats(&v, &unseen(&v));
        assert_eq!(threats.len(), 1);
        assert!(threats[0].iter().all(|rack| rack.len() == 1 && rack[0] != n(Black, 4)), "a black 4 would have gone out");
        let with_run = [v.table.clone(), vec![vec![n(Blue, 4), n(Blue, 5), n(Blue, 6)]]].concat();
        assert!(risk(&with_run, &threats[0]) > 0.0, "a blue 3 or 7 now goes out");
        assert_eq!(risk(&v.table, &threats[0]), 0.0, "drawing opens nothing");
    }

    /// How a player choosing by `policy` fares against Hard players: its
    /// share of the wins (ties split) over `games` games of `players`, its
    /// seat changing each game, and its average time a move. Games run on
    /// every core.
    fn arena(games: u64, players: usize, policy: impl Fn(&PlayerView) -> Move + Sync) -> (f64, f64, std::time::Duration) {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u64;
        let results: Vec<(f64, i64, std::time::Duration, u32)> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|thread| {
                    let policy = &policy;
                    scope.spawn(move || {
                        let (mut wins, mut score, mut time, mut moves) = (0.0, 0, std::time::Duration::ZERO, 0);
                        for seed in (thread..games).step_by(threads as usize) {
                            let seat = seed as usize % players;
                            let mut game = Game::new(players, Rules::default(), 1_000_000 + seed);
                            while game.outcome().is_none() {
                                let who = game.current();
                                let view = game.view(who);
                                let choice = if who == seat {
                                    let start = std::time::Instant::now();
                                    let choice = policy(&view);
                                    time += start.elapsed();
                                    moves += 1;
                                    choice
                                } else {
                                    bot_move(&view, BotLevel::Hard)
                                };
                                match choice {
                                    Move::Play(table) => game.play(who, table).expect("a valid play"),
                                    Move::Draw => {
                                        game.draw(who).unwrap();
                                    }
                                }
                            }
                            let outcome = game.outcome().unwrap();
                            if outcome.winners.contains(&seat) {
                                wins += 1.0 / outcome.winners.len() as f64;
                            }
                            score += outcome.scores[seat] as i64;
                        }
                        (wins, score, time, moves)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let wins: f64 = results.iter().map(|r| r.0).sum();
        let score: i64 = results.iter().map(|r| r.1).sum();
        let time: std::time::Duration = results.iter().map(|r| r.2).sum();
        let moves: u32 = results.iter().map(|r| r.3).sum();
        (wins / games as f64, score as f64 / games as f64, time / moves.max(1))
    }

    fn report(name: &str, policy: impl Fn(&PlayerView) -> Move + Sync + Copy) {
        for (players, games) in [(2, 2000), (4, 2000)] {
            let (share, score, time) = arena(games, players, policy);
            let fair = 1.0 / players as f64;
            println!(
                "{name}, {players} players, {games} games vs Hard: wins {:.1}% (an equal player {:.1}%), score {score:+.1} a game, {:?} a move",
                share * 100.0,
                fair * 100.0,
                time
            );
        }
    }

    /// Extra Hard against Hard: `cargo test -p engine --release -- --ignored
    /// --nocapture extra_strength`. Hard against itself first, as a check.
    #[test]
    #[ignore]
    fn extra_strength() {
        report("Hard", |view| bot_move(view, BotLevel::Hard));
        report("Extra Hard", |view| bot_move(view, BotLevel::ExtraHard));
    }

    /// Tries weights: `EXTRA_WEIGHTS="tile,wait,point,risk,danger_point;..." cargo test -p
    /// engine --release -- --ignored --nocapture extra_weights`.
    #[test]
    #[ignore]
    fn extra_weights() {
        let spec = std::env::var("EXTRA_WEIGHTS").unwrap_or_else(|_| "100,1,1,1000,3".into());
        for set in spec.split(';') {
            let w: Vec<i32> = set.split(',').map(|x| x.trim().parse().unwrap()).collect();
            let weights = Weights { tile: w[0], wait: w[1], point: w[2], risk: w[3], danger_point: w[4] };
            report(&format!("{weights:?}"), move |view| extra_move(view, weights));
        }
    }
}


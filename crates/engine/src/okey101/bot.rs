//! Okey 101's computer players. Easy and Medium play what they can; Hard
//! and Extra Hard also choose their discard with care (`careful_discard`).
//!
//! Before opening a bot looks for the sets worth the most on its rack (the
//! classic solver, `solve`, with the okey as its joker and a false okey as
//! the okey's tile) and opens once they reach the bar, else with pairs if
//! it has enough. Once open it lays down what it can, then adds single
//! tiles to the ends of the table's sets. It takes the previous player's
//! discard only when that lets it open, or put more down, this turn. It
//! discards a tile that costs nothing, the one least linked to the rest of
//! its rack (`discard`). It always keeps a tile to discard, so going out
//! is laying all but one.
//!
//! The careful ones keep pairs when their rack is pair-heavy and its sets
//! weak, and between tiles equally linked to their rack discard the one of
//! least use to the next player (who would open with it, costing its
//! number ×10): nobody can use it any more (its neighbours all in sight),
//! or the next player threw it away themselves. `Care` holds what they
//! heed, with what was measured.

use crate::bot::BotLevel;
use crate::solve::{solve, Objective};
use crate::tile::{Counts, Tile, MAX_VALUE};

use super::{face, fits, kind, plain, safe_discard, set_points, HandView, MeldKind, Opened, Phase, Turn, RUN_END_TILES};

/// A computer player's move in Okey 101.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OkeyMove {
    /// Draw from the pool; the turn goes on.
    Draw,
    /// End the turn.
    Turn(Turn),
}

/// A careful bot's rack counts as pair-heavy with this many pairs...
const PAIR_HEAVY: usize = 4;
/// ...when its best sets are worth less than this.
const WEAK_SETS: u32 = 60;
/// An opponent who has opened with this many tiles or fewer may go out soon.
const DANGER_TILES: usize = 4;

/// What a careful bot heeds when it discards (measured by `okey_strength`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Care {
    /// How much a tile's use to the next player counts against
    /// discarding it, beside its links to the bot's rack (each worth 3);
    /// at 0 it only breaks ties.
    pub feed: u32,
    /// Keeping pairs when the rack is pair-heavy.
    pub pairs: bool,
    /// Throwing away the highest tiles when an opponent may go out soon.
    pub danger: bool,
}

/// Measured 2026-10-28, 400 matches of 7 hands, two Hard against two Easy:
/// this wins 60.5% (Easy 40.0%), 684 points a match against 751. Weighing
/// feeding against links did worse (feed 1: 56.2%, 2: 54.8%), and so did
/// throwing away high tiles near the end (alone: 44.5%, breaking up sets).
pub(crate) const CARE: Care = Care { feed: 0, pairs: true, danger: false };

/// The move of the player whose view this is, on their turn.
pub fn bot_move(view: &HandView, level: BotLevel) -> OkeyMove {
    bot_move_with(view, level, CARE)
}

pub(crate) fn bot_move_with(view: &HandView, level: BotLevel, care: Care) -> OkeyMove {
    let Some(you) = view.you else { return OkeyMove::Draw };
    let careful = matches!(level, BotLevel::Hard | BotLevel::ExtraHard).then_some(care);
    let plan = |rack: &[Tile]| plan(view, you, rack, careful);
    match view.phase {
        Phase::Play => OkeyMove::Turn(plan(&view.rack).turn),
        Phase::Draw => {
            let previous = (view.current + view.rack_sizes.len() - 1) % view.rack_sizes.len();
            let offer = view.discards[previous].last().copied().filter(|&t| t != view.okey);
            if let Some(tile) = offer {
                let mut rack = view.rack.clone();
                rack.push(tile);
                let with = plan(&rack);
                let without = plan(&view.rack);
                let opens = with.opened != Opened::No && without.opened == Opened::No;
                if with.uses(tile, view) && (opens || with.played.len() > without.played.len()) {
                    return OkeyMove::Turn(Turn { take_discard: true, ..with.turn });
                }
            }
            OkeyMove::Draw
        }
    }
}

/// A turn worked out from a rack.
struct Plan {
    turn: Turn,
    /// How the player stands after it.
    opened: Opened,
    /// The rack's tiles that went on the table.
    played: Vec<Tile>,
    /// Sets on the table before the turn.
    before: usize,
}

impl Plan {
    /// Whether `tile` (the discard taken) goes on the table as the rules
    /// ask: into the opening, for a player opening with it.
    fn uses(&self, tile: Tile, view: &HandView) -> bool {
        let opening = view.you.is_some_and(|you| view.opened[you] == Opened::No);
        if opening {
            self.turn.table[self.before..].iter().flatten().any(|&t| t == tile)
        } else {
            self.played.contains(&tile)
        }
    }
}

/// The turn the bot plays with `rack` (its own, or with the discard).
fn plan(view: &HandView, you: usize, rack: &[Tile], careful: Option<Care>) -> Plan {
    let okey = view.okey;
    let mut table = view.table.clone();
    let mut left = rack.to_vec();
    let mut opened = view.opened[you];
    let new_sets = match opened {
        Opened::No => {
            let sets = best_sets(okey, &left, Objective::Points)
                .filter(|sets| sets.iter().map(|s| set_points(okey, s)).sum::<u32>() >= view.opening_points);
            let pairs = || pairs(okey, &left).filter(|pairs| pairs.len() >= view.opening_pairs);
            match (sets, pairs()) {
                (Some(sets), _) => {
                    opened = Opened::Sets;
                    sets
                }
                (None, Some(pairs)) => {
                    opened = Opened::Pairs;
                    pairs
                }
                (None, None) => Vec::new(),
            }
        }
        Opened::Sets => best_sets(okey, &left, Objective::Tiles { joker: -1 }).unwrap_or_default(),
        Opened::Pairs => pairs(okey, &left).unwrap_or_default(),
    };
    for set in &new_sets {
        take_out(&mut left, set);
    }
    table.extend(new_sets);
    if opened != Opened::No {
        add_to_table(okey, &mut table, &mut left);
    }
    let played = Counts::of(rack).minus(&Counts::of(&left)).expect("taken from the rack").tiles();
    let discard = if let Some(care) = careful {
        let pair_heavy = care.pairs && opened == Opened::Pairs
            || (care.pairs && opened == Opened::No
                && pairs(okey, &left).is_some_and(|p| p.len() >= PAIR_HEAVY)
                && best_sets(okey, &left, Objective::Points)
                    .is_none_or(|sets| sets.iter().map(|s| set_points(okey, s)).sum::<u32>() < WEAK_SETS));
        careful_discard(view, you, &table, &left, pair_heavy, care)
    } else {
        discard(okey, &table, &left)
    };
    Plan { turn: Turn { take_discard: false, table, discard }, opened, played, before: view.table.len() }
}

/// The runs and groups to lay down from `rack`, best by `objective`,
/// leaving at least one tile to discard; `None` if there are none.
fn best_sets(okey: Tile, rack: &[Tile], objective: Objective) -> Option<Vec<Vec<Tile>>> {
    let solve_rack = |rack: &[Tile]| solve(&[], &plain(okey, rack), objective);
    let mut best = solve_rack(rack)?;
    if best.played.len() == rack.len() {
        // All of it: keep back the tile whose loss costs least.
        let mut kinds = rack.to_vec();
        kinds.sort_by_key(|t| format!("{t:?}"));
        kinds.dedup();
        best = kinds
            .iter()
            .filter_map(|&keep| {
                let mut rest = rack.to_vec();
                take_out(&mut rest, &[keep]);
                solve_rack(&rest)
            })
            .max_by_key(|solution| solution.score)?;
    }
    let sets: Vec<Vec<Tile>> = best.sets.iter().map(|set| unplain(okey, set)).collect();
    (!sets.is_empty()).then_some(sets)
}

/// The tile to discard from `left`: of those that cost nothing (not the
/// okey, nor fitting the table), the one least linked to the others, as
/// they might yet make a set with it; the highest of equals, as it costs
/// most left over. If all cost a penalty, as `safe_discard` picks.
fn discard(okey: Tile, table: &[Vec<Tile>], left: &[Tile]) -> Tile {
    let free: Vec<usize> = (0..left.len()).filter(|&i| left[i] != okey && !fits(okey, table, left[i])).collect();
    let links = |i: usize| -> u32 {
        let [tile] = plain(okey, &[left[i]])[..] else { unreachable!() };
        (0..left.len())
            .filter(|&j| j != i)
            .map(|j| match (tile, plain(okey, &[left[j]])[0]) {
                (Tile::Number { color: c, value: v }, Tile::Number { color: d, value: w }) => {
                    match (c == d, v.abs_diff(w)) {
                        (true, 0) => 1,
                        (true, 1) => 3,
                        (true, 2) => 1,
                        (false, 0) => 2,
                        _ => 0,
                    }
                }
                _ => 0,
            })
            .sum()
    };
    free.into_iter()
        .min_by_key(|&i| (links(i), std::cmp::Reverse(face(okey, left[i]))))
        .map_or_else(|| safe_discard(okey, table, left), |i| left[i])
}

/// How strongly tile `i` of `left` is linked to the others, as it might
/// make a set with them: neighbours of its colour, its number in other
/// colours, its twin (much more when keeping pairs).
fn links(okey: Tile, left: &[Tile], i: usize, pair_heavy: bool) -> u32 {
    let [tile] = plain(okey, &[left[i]])[..] else { unreachable!() };
    (0..left.len())
        .filter(|&j| j != i)
        .map(|j| match (tile, plain(okey, &[left[j]])[0]) {
            (Tile::Number { color: c, value: v }, Tile::Number { color: d, value: w }) => {
                match (c == d, v.abs_diff(w)) {
                    (true, 0) if pair_heavy => 6,
                    (true, 0) => 1,
                    _ if pair_heavy => 0,
                    (true, 1) => 3,
                    (true, 2) => 1,
                    (false, 0) => 2,
                    _ => 0,
                }
            }
            _ => 0,
        })
        .sum()
}

/// A careful bot's discard from `left`, of those that cost nothing (as
/// `discard`): when an opponent who has opened may go out soon, the
/// highest; else the one least linked to its rack and least use to the
/// next player, who would open with it at its number ×10 to the bot.
fn careful_discard(view: &HandView, you: usize, table: &[Vec<Tile>], left: &[Tile], pair_heavy: bool, care: Care) -> Tile {
    let okey = view.okey;
    let free: Vec<usize> = (0..left.len()).filter(|&i| left[i] != okey && !fits(okey, table, left[i])).collect();
    if free.is_empty() {
        return safe_discard(okey, table, left);
    }
    let players = view.rack_sizes.len();
    let danger = (0..players).any(|p| p != you && view.opened[p] != Opened::No && view.rack_sizes[p] <= DANGER_TILES);
    if care.danger && danger {
        let i = free.into_iter().max_by_key(|&i| (face(okey, left[i]), std::cmp::Reverse(links(okey, left, i, false))));
        return left[i.expect("not empty")];
    }
    // Tiles in sight: the table after this turn, every discard, the bot's own rack, the indicator.
    let seen = Counts::of(table.iter().flatten().chain(view.discards.iter().flatten()).chain(left).chain([&view.indicator]));
    let unseen = |color, value: i32| -> u32 {
        if !(1..=MAX_VALUE as i32).contains(&value) {
            return 0;
        }
        let tile = Tile::Number { color, value: value as u8 };
        // A tile that is the okey is wild: anyone may hold it for anything.
        if tile == okey {
            return 0;
        }
        2u32.saturating_sub(seen.count(tile))
    };
    let next = (you + 1) % players;
    // How much use `tile` could be to the next player: the tiles that could
    // still make a set with it, unless they threw one away themselves.
    let feeds = |tile: Tile| -> u32 {
        if view.discards[next].contains(&tile) {
            return 0;
        }
        let [Tile::Number { color, value }] = plain(okey, &[tile])[..] else { return 0 };
        let v = value as i32;
        let run = 2 * (unseen(color, v - 1) + unseen(color, v + 1)) + unseen(color, v - 2) + unseen(color, v + 2);
        let group: u32 = crate::tile::Color::ALL.iter().filter(|&&c| c != color).map(|&c| unseen(c, v)).sum();
        run + group
    };
    let i = free.into_iter().min_by_key(|&i| {
        let feeds = feeds(left[i]);
        (3 * links(okey, left, i, pair_heavy) + care.feed * feeds, feeds, std::cmp::Reverse(face(okey, left[i])))
    });
    left[i.expect("not empty")]
}

/// As many pairs as `rack` makes, the okeys pairing single tiles (or each
/// other), leaving at least one tile; `None` if none.
fn pairs(okey: Tile, rack: &[Tile]) -> Option<Vec<Vec<Tile>>> {
    let mut singles: Vec<Tile> = Vec::new();
    let mut pairs = Vec::new();
    let mut tiles: Vec<Tile> = rack.iter().copied().filter(|&t| t != okey).collect();
    tiles.sort_by_key(|t| format!("{t:?}"));
    let mut i = 0;
    while i < tiles.len() {
        if i + 1 < tiles.len() && tiles[i] == tiles[i + 1] {
            pairs.push(vec![tiles[i], tiles[i]]);
            i += 2;
        } else {
            singles.push(tiles[i]);
            i += 1;
        }
    }
    // The okeys pair the highest singles first: they'd cost most left over.
    singles.sort_by_key(|&t| std::cmp::Reverse(face(okey, t)));
    let okeys = rack.iter().filter(|&&t| t == okey).count();
    let mut spare = okeys;
    for &single in singles.iter().take(okeys) {
        pairs.push(vec![single, okey]);
        spare -= 1;
    }
    for _ in 0..spare / 2 {
        pairs.push(vec![okey, okey]);
    }
    if pairs.len() * 2 == rack.len() {
        pairs.pop();
    }
    (!pairs.is_empty()).then_some(pairs)
}

/// Adds single tiles from `left` to the ends of the table's runs and
/// groups while any fit, at most `RUN_END_TILES` at each end of a set,
/// keeping the okey and one tile to discard.
fn add_to_table(okey: Tile, table: &mut [Vec<Tile>], left: &mut Vec<Tile>) {
    let mut added = vec![(0, 0); table.len()];
    let mut changed = true;
    while changed && left.len() > 1 {
        changed = false;
        for i in 0..left.len() {
            let tile = left[i];
            if tile == okey {
                continue;
            }
            let fit = table.iter().enumerate().filter(|(_, set)| set.len() > 2).find_map(|(s, set)| {
                let back = [set.as_slice(), &[tile]].concat();
                let front = [&[tile], set.as_slice()].concat();
                let fits = |set: &[Tile]| matches!(kind(okey, set), Some(MeldKind::Run | MeldKind::Group));
                if added[s].1 < RUN_END_TILES && fits(&back) {
                    Some((s, back, false))
                } else if added[s].0 < RUN_END_TILES && fits(&front) {
                    Some((s, front, true))
                } else {
                    None
                }
            });
            if let Some((s, longer, at_front)) = fit {
                table[s] = longer;
                if at_front {
                    added[s].0 += 1;
                } else {
                    added[s].1 += 1;
                }
                left.remove(i);
                changed = true;
                break;
            }
        }
    }
}

/// The solver's tiles back as the hand's: its joker the okey, the okey's
/// number a false okey.
fn unplain(okey: Tile, tiles: &[Tile]) -> Vec<Tile> {
    tiles
        .iter()
        .map(|&t| match t {
            Tile::Joker => okey,
            t if t == okey => Tile::Joker,
            t => t,
        })
        .collect()
}

/// Removes one of each of `tiles` from `rack`.
fn take_out(rack: &mut Vec<Tile>, tiles: &[Tile]) {
    for tile in tiles {
        let i = rack.iter().position(|t| t == tile).expect("on the rack");
        rack.remove(i);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Hand, Match, Okey101Rules};
    use super::*;
    use crate::tile::Color::{self, *};

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    /// With the indicator red 4.
    const OKEY: Tile = Tile::Number { color: Red, value: 5 };

    fn play_turn(hand: &mut Hand) {
        let player = hand.current();
        match bot_move(&hand.view(player), BotLevel::Easy) {
            OkeyMove::Draw => {
                hand.draw(player).unwrap();
                if hand.end().is_none() {
                    play_turn(hand);
                }
            }
            OkeyMove::Turn(turn) => hand.play(player, turn).expect("the bot's turns are valid"),
        }
    }

    #[test]
    fn it_opens_when_its_sets_reach_101() {
        let sets: Vec<Vec<Tile>> = [Black, Blue, Orange].iter().map(|&c| vec![n(c, 11), n(c, 12), n(c, 13)]).collect();
        let mut rack = sets.concat();
        rack.extend([n(Red, 1), n(Black, 3)]);
        let mut h = Hand::from_deal(vec![rack, vec![n(Blue, 1)]], vec![], n(Red, 4), 0, Okey101Rules::default());
        play_turn(&mut h);
        assert_eq!(h.opened(0), Opened::Sets);
        assert_eq!(h.rack(0).len(), 1);
    }

    #[test]
    fn it_uses_the_okey_in_its_opening() {
        // 11, okey, 13 in black: 36, with blue and orange: 108.
        let mut rack = vec![n(Black, 11), OKEY, n(Black, 13)];
        rack.extend([Blue, Orange].iter().flat_map(|&c| [n(c, 11), n(c, 12), n(c, 13)]));
        rack.push(n(Red, 1));
        let mut h = Hand::from_deal(vec![rack, vec![n(Blue, 1)]], vec![], n(Red, 4), 0, Okey101Rules::default());
        play_turn(&mut h);
        assert_eq!(h.opened(0), Opened::Sets);
        assert!(h.table().iter().flatten().any(|&t| t == OKEY));
    }

    #[test]
    fn it_takes_the_discard_that_opens_it() {
        // Black 10 and 12-13 need the 11 player 0 discards.
        let mut rack1 = vec![n(Black, 10), n(Black, 12), n(Black, 13)];
        rack1.extend([Blue, Orange].iter().flat_map(|&c| [n(c, 11), n(c, 12), n(c, 13)]));
        rack1.push(n(Red, 1));
        let rack0 = vec![n(Black, 11), n(Red, 2)];
        let mut h = Hand::from_deal(vec![rack0, rack1], vec![n(Blue, 2)], n(Red, 4), 0, Okey101Rules::default());
        h.play(0, Turn { take_discard: false, table: vec![], discard: n(Black, 11) }).unwrap();
        let OkeyMove::Turn(turn) = bot_move(&h.view(1), BotLevel::Hard) else { panic!("it takes the 11") };
        assert!(turn.take_discard);
        h.play(1, turn).unwrap();
        assert_eq!(h.opened(1), Opened::Sets);
    }

    #[test]
    fn pairs_keep_a_tile() {
        let rack = vec![n(Blue, 1), n(Blue, 1), OKEY, n(Black, 9)];
        let made = pairs(OKEY, &rack).unwrap();
        assert_eq!(made, vec![vec![n(Blue, 1), n(Blue, 1)]], "the okey and the 9 would be the last tiles");
        let rack = vec![n(Blue, 1), n(Blue, 1), OKEY, n(Black, 9), n(Orange, 3)];
        assert_eq!(pairs(OKEY, &rack).unwrap().len(), 2, "the okey pairs the 9");
    }

    /// Plays a match of bots at `levels` (a seat each); its totals and the
    /// hands that had a winner.
    fn play_match(levels: &[BotLevel], rules: Okey101Rules, seed: u64) -> (Vec<i32>, usize, usize) {
        play_match_with(levels, rules, seed, CARE)
    }

    fn play_match_with(levels: &[BotLevel], rules: Okey101Rules, seed: u64, care: Care) -> (Vec<i32>, usize, usize) {
        let mut m = Match::new(levels.len(), rules, seed);
        let (mut hands, mut won) = (0, 0);
        loop {
            let mut turns = 0;
            while m.hand().end().is_none() {
                let player = m.hand().current();
                match bot_move_with(&m.hand().view(player), levels[player], care) {
                    OkeyMove::Draw => {
                        m.draw(player).unwrap();
                    }
                    OkeyMove::Turn(turn) => m.play(player, turn).expect("the bot's turns are valid"),
                }
                turns += 1;
                assert!(turns < 400, "seed {seed}: a hand doesn't end");
            }
            hands += 1;
            won += m.hand().end().unwrap().winner.is_some() as usize;
            if !m.next_hand() {
                break;
            }
        }
        (m.totals().to_vec(), hands, won)
    }

    #[test]
    fn whole_matches_of_bots() {
        let (mut hands, mut won) = (0, 0);
        for seed in 0..16 {
            let levels = [BotLevel::Easy, BotLevel::Hard, BotLevel::Easy, BotLevel::Hard];
            let players = 2 + seed as usize % 3;
            let (_, h, w) = play_match(&levels[..players], Okey101Rules { hands: 2, ..Okey101Rules::default() }, seed);
            hands += h;
            won += w;
        }
        println!("{won} of {hands} hands won");
        assert!(won * 2 > hands, "bots that meld usually go out: {won} of {hands}");
    }

    /// Hard against Easy, two each at a table of four, seats alternating:
    /// their average totals per match and how many matches each won
    /// (lowest total). `cargo test -p engine --release -- --ignored
    /// --nocapture okey_strength`; `OKEY_GAMES` for fewer or more matches,
    /// `OKEY_CARE=feed,pairs,danger` (e.g. `0,0,1`) to try other care.
    #[test]
    #[ignore]
    fn okey_strength() {
        let games: u64 = std::env::var("OKEY_GAMES").ok().and_then(|g| g.parse().ok()).unwrap_or(200);
        let care = std::env::var("OKEY_CARE").ok().map_or(CARE, |care| {
            let parts: Vec<u32> = care.split(',').map(|p| p.trim().parse().expect("numbers")).collect();
            Care { feed: parts[0], pairs: parts[1] != 0, danger: parts[2] != 0 }
        });
        let rules = Okey101Rules::default();
        let (mut totals, mut wins) = ([0i64; 2], [0u32; 2]);
        for seed in 0..games {
            let hard_first = seed % 2 == 0;
            let level = |seat: usize| if seat.is_multiple_of(2) == hard_first { BotLevel::Hard } else { BotLevel::Easy };
            let levels: Vec<BotLevel> = (0..4).map(level).collect();
            let (match_totals, _, _) = play_match_with(&levels, rules.clone(), seed, care);
            let lowest = *match_totals.iter().min().unwrap();
            for seat in 0..4 {
                let k = (levels[seat] == BotLevel::Easy) as usize;
                totals[k] += match_totals[seat] as i64;
                wins[k] += (match_totals[seat] == lowest) as u32;
            }
        }
        let average = |k: usize| totals[k] as f64 / (2 * games) as f64;
        println!("{games} matches of {} hands, two Hard ({care:?}) and two Easy bots:", rules.hands);
        println!("  Hard: {:.0} points a match on average, won {} ({:.1}%)", average(0), wins[0], 100.0 * wins[0] as f64 / games as f64);
        println!("  Easy: {:.0} points a match on average, won {} ({:.1}%)", average(1), wins[1], 100.0 * wins[1] as f64 / games as f64);
    }
}

//! Okey 101's computer player: a simple one that plays what it can.
//!
//! Before opening it looks for the sets worth the most on its rack (the
//! classic solver, `solve`, with the okey as its joker and a false okey as
//! the okey's tile) and opens once they reach the bar, else with pairs if
//! it has enough. Once open it lays down what it can, then adds single
//! tiles to the ends of the table's sets. It takes the previous player's
//! discard only when that lets it open, or put more down, this turn. It
//! discards a tile that costs nothing, the one least linked to the rest of
//! its rack (`discard`). It always keeps a tile to discard, so going out
//! is laying all but one.

use crate::solve::{solve, Objective};
use crate::tile::{Counts, Tile};

use super::{face, fits, kind, plain, safe_discard, set_points, HandView, Kind, Opened, Phase, Turn, RUN_END_TILES};

/// A computer player's move in Okey 101.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OkeyMove {
    /// Draw from the pool; the turn goes on.
    Draw,
    /// End the turn.
    Turn(Turn),
}

/// The move of the player whose view this is, on their turn.
pub fn bot_move(view: &HandView) -> OkeyMove {
    let Some(you) = view.you else { return OkeyMove::Draw };
    match view.phase {
        Phase::Play => OkeyMove::Turn(plan(view, you, &view.rack).turn),
        Phase::Draw => {
            let previous = (view.current + view.rack_sizes.len() - 1) % view.rack_sizes.len();
            let offer = view.discards[previous].last().copied().filter(|&t| t != view.okey);
            if let Some(tile) = offer {
                let mut rack = view.rack.clone();
                rack.push(tile);
                let with = plan(view, you, &rack);
                let without = plan(view, you, &view.rack);
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
fn plan(view: &HandView, you: usize, rack: &[Tile]) -> Plan {
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
    let discard = discard(okey, &table, &left);
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
                let fits = |set: &[Tile]| matches!(kind(okey, set), Some(Kind::Run | Kind::Group));
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
        match bot_move(&hand.view(player)) {
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
        let OkeyMove::Turn(turn) = bot_move(&h.view(1)) else { panic!("it takes the 11") };
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

    #[test]
    fn whole_matches_of_bots() {
        let (mut hands, mut won, mut opened) = (0, 0, 0);
        for seed in 0..16 {
            let players = 2 + seed as usize % 3;
            let mut m = Match::new(players, Okey101Rules { hands: 2, ..Okey101Rules::default() }, seed);
            loop {
                let mut turns = 0;
                while m.hand().end().is_none() {
                    let player = m.hand().current();
                    match bot_move(&m.hand().view(player)) {
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
                opened += (0..players).filter(|&p| m.hand().opened(p) != Opened::No).count();
                if !m.next_hand() {
                    break;
                }
            }
        }
        println!("{won} of {hands} hands won, {opened} openings");
        assert!(won * 2 > hands, "bots that meld usually go out: {won} of {hands}");
    }
}

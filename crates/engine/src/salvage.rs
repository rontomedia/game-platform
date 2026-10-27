//! When a player's time runs out mid-turn: the most of the table they left
//! that can be played, with the tiles that can't going back to their rack.

use crate::game::Rules;
use crate::set::classify;
use crate::solve::{solve, Objective};
use crate::tile::Tile;
use crate::turn::check_turn;

/// Jokers the player put down are played if they fit anywhere.
const PLAY_JOKERS: Objective = Objective::Tiles { joker: 1 };

/// A valid turn made from `draft`, the table a player left when their time
/// ran out (the turn began with `before`; `rack` is theirs), or `None` if
/// nothing of it can be played. Tried in order:
/// 1. The draft as it is.
/// 2. Once melded: as many of the tiles the player put down as can be
///    played, the table rearranged as needed, keeping each of the draft's
///    valid sets as the player left it where that still plays as many. The
///    tiles that don't fit go back to the rack.
///
/// Before the initial meld the table's sets must stay as they were, so only
/// new sets of the player's own tiles count, worth the initial meld.
pub fn salvage(
    before: &[Vec<Tile>],
    draft: &[Vec<Tile>],
    rack: &[Tile],
    melded: bool,
    rules: &Rules,
) -> Option<Vec<Vec<Tile>>> {
    let valid = |table: &Vec<Vec<Tile>>| check_turn(before, table, rack, melded, rules).is_ok();
    if valid(&draft.to_vec()) {
        return Some(draft.to_vec());
    }
    let table: Vec<Tile> = before.iter().flatten().copied().collect();
    // The player's tiles in the draft, and its valid sets.
    let put_down = without(&draft.iter().flatten().copied().collect::<Vec<_>>(), &table);
    let put_down = within(&put_down, rack);
    let sets: Vec<Vec<Tile>> = draft.iter().filter(|set| classify(set).is_some()).cloned().collect();

    if !melded {
        // The table's sets as they were, and the draft's new valid sets of
        // the player's tiles; else the best sets of those tiles.
        let mut theirs = put_down.clone();
        let mut new_sets = Vec::new();
        for set in &sets {
            if before.contains(set) {
                continue;
            }
            if let Some(rest) = take_all(&theirs, set) {
                theirs = rest;
                new_sets.push(set.clone());
            }
        }
        let kept = [before.to_vec(), new_sets].concat();
        if valid(&kept) {
            return Some(kept);
        }
        let best = solve(&[], &put_down, Objective::Points)?;
        return Some([before.to_vec(), best.sets].concat()).filter(valid);
    }

    // 2. The most that can be played...
    let best = solve(&table, &put_down, PLAY_JOKERS)?;
    let most = best.played.len();
    // ...keeping the player's valid sets where that still plays as many.
    let mut kept: Vec<Vec<Tile>> = Vec::new();
    let mut rest = best.sets;
    for set in sets {
        let trial = [kept.clone(), vec![set]].concat();
        let in_kept: Vec<Tile> = trial.iter().flatten().copied().collect();
        let from_rack = without(&in_kept, &table);
        let Some(spare) = take_all(&put_down, &from_rack) else { continue };
        let need = without(&table, &in_kept);
        if let Some(solution) = solve(&need, &spare, PLAY_JOKERS).filter(|s| from_rack.len() + s.played.len() == most) {
            kept = trial;
            rest = solution.sets;
        }
    }
    Some([kept, rest].concat()).filter(valid)
}

/// `tiles` without one of each of `taken` (those it has).
fn without(tiles: &[Tile], taken: &[Tile]) -> Vec<Tile> {
    let mut left = tiles.to_vec();
    for tile in taken {
        if let Some(i) = left.iter().position(|t| t == tile) {
            left.remove(i);
        }
    }
    left
}

/// The tiles of `tiles` that `pool` has (one each).
fn within(tiles: &[Tile], pool: &[Tile]) -> Vec<Tile> {
    let mut left = pool.to_vec();
    tiles
        .iter()
        .filter(|tile| match left.iter().position(|t| t == *tile) {
            Some(i) => {
                left.remove(i);
                true
            }
            None => false,
        })
        .copied()
        .collect()
}

/// `tiles` without all of `taken`, if it has them all.
fn take_all(tiles: &[Tile], taken: &[Tile]) -> Option<Vec<Tile>> {
    let left = without(tiles, taken);
    (left.len() + taken.len() == tiles.len()).then_some(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::Color::{self, *};

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    const J: Tile = Tile::Joker;

    fn run(color: Color, values: std::ops::RangeInclusive<u8>) -> Vec<Tile> {
        values.map(|v| n(color, v)).collect()
    }

    fn salvaged(before: &[Vec<Tile>], draft: &[Vec<Tile>], rack: &[Tile], melded: bool) -> Option<Vec<Vec<Tile>>> {
        let table = salvage(before, draft, rack, melded, &Rules::default());
        if let Some(table) = &table {
            assert!(check_turn(before, table, rack, melded, &Rules::default()).is_ok(), "{table:?}");
        }
        table
    }

    #[test]
    fn a_valid_draft_is_played_as_it_is() {
        let before = [run(Red, 3..=5)];
        let draft = [run(Red, 3..=6)];
        assert_eq!(salvaged(&before, &draft, &[n(Red, 6)], true), Some(draft.to_vec()));
    }

    #[test]
    fn unfinished_sets_go_back_to_the_rack() {
        let before = [run(Red, 3..=5)];
        // A new run, and a pair that isn't a set yet.
        let draft = [run(Red, 3..=5), run(Blue, 7..=9), vec![n(Black, 1), n(Black, 2)]];
        let rack = [n(Blue, 7), n(Blue, 8), n(Blue, 9), n(Black, 1), n(Black, 2)];
        assert_eq!(salvaged(&before, &draft, &rack, true), Some(vec![run(Red, 3..=5), run(Blue, 7..=9)]));
    }

    #[test]
    fn table_tiles_left_loose_are_set_again() {
        // 3-4-5-6-7 split, the 3 and 4 left on their own, the 8 added to the
        // rest: the 3 and 4 can only go back into the run.
        let before = [run(Red, 3..=7)];
        let draft = [vec![n(Red, 3), n(Red, 4)], run(Red, 5..=8)];
        assert_eq!(salvaged(&before, &draft, &[n(Red, 8), n(Blue, 1)], true), Some(vec![run(Red, 3..=8)]));
        // Valid sets stay as the player left them where they can.
        let before = [run(Red, 3..=8), vec![n(Blue, 9), n(Black, 9), n(Orange, 9)]];
        let draft = [run(Red, 3..=5), vec![n(Red, 6), n(Red, 7)], vec![n(Red, 8), n(Blue, 9)], vec![n(Black, 9), n(Orange, 9), n(Red, 9)]];
        let table = salvaged(&before, &draft, &[n(Red, 9)], true).unwrap();
        assert!(table.contains(&run(Red, 3..=5)), "kept as the player left it: {table:?}");
        // The blue 9 left loose can only join the other 9s.
        assert!(table.iter().any(|set| set.len() == 4 && set.contains(&n(Blue, 9))), "{table:?}");
    }

    #[test]
    fn nothing_to_play() {
        let before = [run(Red, 3..=5)];
        let draft = [run(Red, 3..=5), vec![n(Black, 1), n(Black, 2)]];
        assert_eq!(salvaged(&before, &draft, &[n(Black, 1), n(Black, 2)], true), None);
        assert_eq!(salvaged(&before, &before, &[n(Black, 1)], true), None, "nothing put down");
    }

    #[test]
    fn before_the_initial_meld_only_new_sets_worth_it_count() {
        let before = [run(Red, 3..=5)];
        let meld = run(Blue, 10..=12);
        // The meld, an unfinished pair, and a tile wrongly added to the table's run.
        let draft = [vec![n(Red, 3), n(Red, 4), n(Red, 5), n(Red, 6)], meld.clone(), vec![J, n(Black, 2)]];
        let rack = [n(Blue, 10), n(Blue, 11), n(Blue, 12), J, n(Black, 2), n(Red, 6)];
        assert_eq!(salvaged(&before, &draft, &rack, false), Some(vec![run(Red, 3..=5), meld]));
        // Below the initial meld: nothing.
        let low = run(Blue, 1..=3);
        assert_eq!(salvaged(&before, &[run(Red, 3..=5), low.clone()], &low, false), None);
    }
}

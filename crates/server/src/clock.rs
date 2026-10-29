//! Extra time for tiles (rooms with `tile_bonus`): what a table is worth in
//! time, if it were played as the turn.
//!
//! A table earns time only once it's a valid turn, and a turn only ever
//! gains: the room gives the difference when a table is worth more than
//! anything before it this turn. Time given for tiles that are then taken
//! back counts against the player (see `room.rs`).

use std::time::Duration;

use tile_rummy::{check_turn, Rules, Tile};

/// What a turn earns: per tile played from the rack, and per set new or
/// changed that holds one of them.
#[derive(Clone, Copy, Debug)]
pub struct Bonus {
    pub per_tile: Duration,
    pub per_set: Duration,
}

impl Bonus {
    /// The time `after` earns as a turn from `before` with `rack`: nothing
    /// unless it's a valid turn.
    pub fn worth(&self, before: &[Vec<Tile>], after: &[Vec<Tile>], rack: &[Tile], melded: bool, rules: &Rules) -> Duration {
        let Ok(played) = check_turn(before, after, rack, melded, rules) else { return Duration::ZERO };
        // Sets that weren't on the table as they are now...
        let mut old: Vec<&Vec<Tile>> = before.iter().collect();
        let changed = after.iter().filter(|set| match old.iter().position(|s| s == set) {
            Some(i) => {
                old.swap_remove(i);
                false
            }
            None => true,
        });
        // ...with a tile like one played (tiles have no identity), at most
        // one set per tile played.
        let sets = changed.filter(|set| set.iter().any(|t| played.contains(t))).count().min(played.len());
        self.per_tile * played.len() as u32 + self.per_set * sets as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tile_rummy::Color::{self, *};

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }

    const BONUS: Bonus = Bonus { per_tile: Duration::from_secs(15), per_set: Duration::from_secs(20) };

    fn worth(before: &[Vec<Tile>], after: &[Vec<Tile>], rack: &[Tile], melded: bool) -> u64 {
        BONUS.worth(before, after, rack, melded, &Rules::default()).as_secs()
    }

    #[test]
    fn a_new_set_from_the_rack() {
        let meld = vec![n(Blue, 10), n(Blue, 11), n(Blue, 12)];
        assert_eq!(worth(&[], std::slice::from_ref(&meld), &meld, false), 3 * 15 + 20);
    }

    #[test]
    fn nothing_until_it_is_a_valid_turn() {
        let low = vec![n(Blue, 1), n(Blue, 2), n(Blue, 3)];
        assert_eq!(worth(&[], std::slice::from_ref(&low), &low, false), 0, "below the initial meld");
        assert_eq!(worth(&[], &[low[..2].to_vec()], &low, true), 0, "not a set");
    }

    #[test]
    fn rearranging_counts_the_sets_holding_new_tiles() {
        // A red 5 splits 3-4-5-6-7 into 3-4-5 and 5-6-7: one tile, but two
        // changed sets have a red 5; at most one set per tile counts.
        let run: Vec<Tile> = (3..=7).map(|v| n(Red, v)).collect();
        let split = [vec![n(Red, 3), n(Red, 4), n(Red, 5)], vec![n(Red, 5), n(Red, 6), n(Red, 7)]];
        assert_eq!(worth(&[run], &split, &[n(Red, 5)], true), 15 + 20);
        // A tile added to the end of a run, another set left as it was.
        let untouched = vec![n(Black, 1), n(Black, 2), n(Black, 3)];
        let before = [untouched.clone(), vec![n(Blue, 4), n(Blue, 5), n(Blue, 6)]];
        let after = [untouched, vec![n(Blue, 4), n(Blue, 5), n(Blue, 6), n(Blue, 7)]];
        assert_eq!(worth(&before, &after, &[n(Blue, 7)], true), 15 + 20);
    }
}

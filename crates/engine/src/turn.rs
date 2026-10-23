use std::fmt;

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::game::Rules;
use crate::set::{classify, points};
use crate::tile::{Counts, Tile};

/// Why a turn, or a move in a game, isn't allowed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TurnError {
    /// A tile that isn't in the game, such as a red 14.
    NoSuchTile,
    /// The set at this place in the new table isn't a run or a group.
    InvalidSet { index: usize },
    /// A tile left the table. Tiles on the table stay there; a joker taken
    /// from a set must be used again in the same turn.
    TileTakenFromTable,
    /// A tile was added that the player's rack doesn't hold.
    TileNotOnRack,
    /// No tile came from the rack. To pass, draw instead.
    NothingPlayed,
    /// The first sets a player puts down must come from their own rack only.
    InitialMeldChangedTable,
    /// The first sets a player puts down must be worth at least the rules'
    /// initial meld.
    InitialMeldTooLow { points: u32, needed: u32 },
    NotYourTurn,
    GameOver,
}

impl fmt::Display for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TurnError::NoSuchTile => write!(f, "a tile that isn't in the game"),
            TurnError::InvalidSet { index } => write!(f, "set {} is not a run or a group", index + 1),
            TurnError::TileTakenFromTable => write!(f, "tiles on the table must stay on the table"),
            TurnError::TileNotOnRack => write!(f, "a tile that isn't on your rack"),
            TurnError::NothingPlayed => write!(f, "play at least one tile from your rack, or draw"),
            TurnError::InitialMeldChangedTable => write!(f, "your first meld must use only tiles from your rack"),
            TurnError::InitialMeldTooLow { points, needed } => {
                write!(f, "your first meld is worth {points} points; it needs {needed}")
            }
            TurnError::NotYourTurn => write!(f, "it isn't your turn"),
            TurnError::GameOver => write!(f, "the game is over"),
        }
    }
}

impl std::error::Error for TurnError {}

/// Checks a turn that changes the table from `before` to `after`, playing
/// tiles from `rack`. `melded`: whether the player has made their initial
/// meld in an earlier turn. Returns the tiles that left the rack.
///
/// Sets may be split, joined and rearranged freely, as long as every tile
/// that was on the table still is and every set is valid at the end.
pub fn check_turn(
    before: &[Vec<Tile>],
    after: &[Vec<Tile>],
    rack: &[Tile],
    melded: bool,
    rules: &Rules,
) -> Result<Vec<Tile>, TurnError> {
    if !after.iter().flatten().chain(rack).all(|t| t.exists()) {
        return Err(TurnError::NoSuchTile);
    }
    if let Some(index) = after.iter().position(|set| classify(set).is_none()) {
        return Err(TurnError::InvalidSet { index });
    }
    let played = Counts::of(after.iter().flatten())
        .minus(&Counts::of(before.iter().flatten()))
        .ok_or(TurnError::TileTakenFromTable)?;
    Counts::of(rack).minus(&played).ok_or(TurnError::TileNotOnRack)?;
    if played.is_empty() {
        return Err(TurnError::NothingPlayed);
    }
    if !melded {
        // The table's sets must all still be there as they were; what's
        // left is the new sets, made of the played tiles only.
        let mut new_sets: Vec<&Vec<Tile>> = after.iter().collect();
        for set in before {
            let i = new_sets.iter().position(|s| *s == set).ok_or(TurnError::InitialMeldChangedTable)?;
            new_sets.swap_remove(i);
        }
        let total: u32 = new_sets.iter().map(|set| points(set).unwrap_or(0)).sum();
        if total < rules.initial_meld {
            return Err(TurnError::InitialMeldTooLow { points: total, needed: rules.initial_meld });
        }
    }
    Ok(played.tiles())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::Color::{self, *};

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    const J: Tile = Tile::Joker;

    fn rules() -> Rules {
        Rules::default()
    }

    fn check(before: &[Vec<Tile>], after: &[Vec<Tile>], rack: &[Tile], melded: bool) -> Result<Vec<Tile>, TurnError> {
        check_turn(before, after, rack, melded, &rules())
    }

    fn red_run() -> Vec<Tile> {
        vec![n(Red, 3), n(Red, 4), n(Red, 5), n(Red, 6)]
    }

    #[test]
    fn initial_meld_of_30_or_more() {
        let rack = [n(Blue, 10), n(Blue, 11), n(Blue, 12), n(Black, 1)];
        let meld = vec![n(Blue, 10), n(Blue, 11), n(Blue, 12)];
        assert_eq!(check(&[red_run()], &[red_run(), meld.clone()], &rack, false), Ok(meld));
    }

    #[test]
    fn initial_meld_may_be_several_sets() {
        let rack = [n(Blue, 5), n(Blue, 6), n(Blue, 7), n(Red, 4), n(Blue, 4), n(Black, 4)];
        let after = [vec![n(Blue, 5), n(Blue, 6), n(Blue, 7)], vec![n(Red, 4), n(Blue, 4), n(Black, 4)]];
        assert!(check(&[], &after, &rack, false).is_ok(), "18 + 12 = 30");
    }

    #[test]
    fn initial_meld_counts_jokers_at_their_value() {
        let rack = [n(Blue, 9), J, n(Blue, 11)];
        assert!(check(&[], &[rack.to_vec()], &rack, false).is_ok(), "9 + 10 + 11 = 30");
    }

    #[test]
    fn initial_meld_below_30() {
        let rack = [n(Blue, 1), n(Blue, 2), n(Blue, 3)];
        assert_eq!(
            check(&[], &[rack.to_vec()], &rack, false),
            Err(TurnError::InitialMeldTooLow { points: 6, needed: 30 })
        );
    }

    #[test]
    fn initial_meld_may_not_use_the_table() {
        // Red 7 and 8 on the end of the table's run: a valid table, but
        // the first meld can't touch it.
        let rack = [n(Red, 7), n(Red, 8), n(Blue, 10), n(Blue, 11), n(Blue, 12)];
        let mut longer = red_run();
        longer.extend([n(Red, 7), n(Red, 8)]);
        let after = [longer, vec![n(Blue, 10), n(Blue, 11), n(Blue, 12)]];
        assert_eq!(check(&[red_run()], &after, &rack, false), Err(TurnError::InitialMeldChangedTable));
        assert!(check(&[red_run()], &after, &rack, true).is_ok(), "fine once melded");
    }

    #[test]
    fn rearranging_the_table_after_melding() {
        // Split 3-4-5-6 into 3-4-5 and 6-7-8, using a 7 and 8 from the rack.
        let rack = [n(Red, 7), n(Red, 8), n(Black, 1)];
        let after = [vec![n(Red, 3), n(Red, 4), n(Red, 5)], vec![n(Red, 6), n(Red, 7), n(Red, 8)]];
        assert_eq!(check(&[red_run()], &after, &rack, true), Ok(vec![n(Red, 7), n(Red, 8)]));
    }

    #[test]
    fn a_joker_taken_from_the_table_must_be_played() {
        let before = [vec![n(Red, 3), J, n(Red, 5)]];
        let rack = [n(Red, 4), n(Blue, 9), n(Black, 9)];
        // The red 4 replaces the joker, which goes into a group of 9s.
        let swapped = [vec![n(Red, 3), n(Red, 4), n(Red, 5)], vec![n(Blue, 9), n(Black, 9), J]];
        assert!(check(&before, &swapped, &rack, true).is_ok());
        // Keeping the joker on the rack instead.
        let kept = [vec![n(Red, 3), n(Red, 4), n(Red, 5)]];
        assert_eq!(check(&before, &kept, &rack, true), Err(TurnError::TileTakenFromTable));
    }

    #[test]
    fn every_set_must_be_valid() {
        let rack = [n(Red, 8), n(Blue, 2)];
        let mut gap = red_run();
        gap.push(n(Red, 8));
        let after = [vec![n(Black, 1), n(Black, 2), n(Black, 3)], gap];
        assert_eq!(
            check(&[vec![n(Black, 1), n(Black, 2), n(Black, 3)], red_run()], &after, &rack, true),
            Err(TurnError::InvalidSet { index: 1 })
        );
    }

    #[test]
    fn tiles_must_come_from_the_rack() {
        let after = [red_run(), vec![n(Blue, 10), n(Blue, 11), n(Blue, 12)]];
        let rack = [n(Blue, 10), n(Blue, 11)];
        assert_eq!(check(&[red_run()], &after, &rack, true), Err(TurnError::TileNotOnRack));
        // A red 7 on the run and another in a group, when the rack has one.
        let mut longer = red_run();
        longer.push(n(Red, 7));
        let after = [longer, vec![n(Red, 7), n(Blue, 7), n(Black, 7)]];
        let rack = [n(Red, 7), n(Blue, 7), n(Black, 7)];
        assert_eq!(check(&[red_run()], &after, &rack, true), Err(TurnError::TileNotOnRack));
    }

    #[test]
    fn something_must_be_played() {
        let rearranged = [vec![n(Red, 3), n(Red, 4), n(Red, 5), n(Red, 6)]];
        assert_eq!(check(&[red_run()], &rearranged, &[n(Red, 7)], true), Err(TurnError::NothingPlayed));
    }

    #[test]
    fn impossible_tiles_are_rejected() {
        let fake = [n(Red, 14), n(Red, 15), n(Red, 16)];
        assert_eq!(check(&[], &[fake.to_vec()], &fake, true), Err(TurnError::NoSuchTile));
    }
}

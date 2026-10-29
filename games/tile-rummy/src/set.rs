use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::tile::{Color, Tile, MAX_VALUE};

/// What makes a set valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum SetKind {
    /// Three or more consecutive numbers of one color, in order.
    Run,
    /// Three or four of one number, each a different color.
    Group,
}

/// Whether `tiles`, in the order they lie on the table, form a valid set,
/// and which kind. Jokers fill any place. A set of only jokers counts as a
/// run.
pub fn classify(tiles: &[Tile]) -> Option<SetKind> {
    if tiles.len() < 3 || !tiles.iter().all(|t| t.exists()) {
        return None;
    }
    if run_start(tiles).is_some() {
        Some(SetKind::Run)
    } else if group_value(tiles).is_some() {
        Some(SetKind::Group)
    } else {
        None
    }
}

/// What a valid set counts towards the initial meld: the sum of the numbers,
/// a joker counting as the number it stands for. `None` if not a valid set.
///
/// One number and jokers can be read as a run or a group (joker, joker, 7:
/// 5-6-7 or three 7s); the higher reading counts.
pub fn points(tiles: &[Tile]) -> Option<u32> {
    classify(tiles)?;
    let len = tiles.len() as u32;
    let run = run_start(tiles).map(|start| (start as u32..start as u32 + len).sum());
    let group = group_value(tiles).map(|value| value as u32 * len);
    run.max(group)
}

fn numbers(tiles: &[Tile]) -> impl Iterator<Item = (usize, Color, u8)> + '_ {
    tiles.iter().enumerate().filter_map(|(i, tile)| match *tile {
        Tile::Number { color, value } => Some((i, color, value)),
        Tile::Joker => None,
    })
}

/// The number the run starts at, if the tiles are one. Only jokers: 1.
fn run_start(tiles: &[Tile]) -> Option<u8> {
    // The first number fixes where the run starts; every tile after must
    // be one higher than the place before it.
    let (start, color) = match numbers(tiles).next() {
        Some((index, color, value)) => ((value as usize).checked_sub(index).filter(|&s| s >= 1)?, Some(color)),
        None => (1, None),
    };
    let fits = start + tiles.len() - 1 <= MAX_VALUE as usize
        && numbers(tiles).all(|(i, c, v)| Some(c) == color && v as usize == start + i);
    fits.then_some(start as u8)
}

/// The number all tiles share, if the tiles are a group.
fn group_value(tiles: &[Tile]) -> Option<u8> {
    if tiles.len() > Color::ALL.len() {
        return None;
    }
    let mut colors = Vec::new();
    let mut value = None;
    for (_, color, v) in numbers(tiles) {
        if colors.contains(&color) || value.is_some_and(|value| value != v) {
            return None;
        }
        colors.push(color);
        value = Some(v);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::Color::*;

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    const J: Tile = Tile::Joker;

    #[test]
    fn runs() {
        assert_eq!(classify(&[n(Red, 3), n(Red, 4), n(Red, 5)]), Some(SetKind::Run));
        assert_eq!(classify(&[n(Red, 3), J, n(Red, 5)]), Some(SetKind::Run));
        assert_eq!(classify(&[J, n(Red, 2), n(Red, 3)]), Some(SetKind::Run));
        assert_eq!(classify(&(1..=13).map(|v| n(Blue, v)).collect::<Vec<_>>()), Some(SetKind::Run));
        assert_eq!(classify(&[J, n(Blue, 1), n(Blue, 2)]), None, "nothing below 1");
        assert_eq!(classify(&[n(Blue, 12), n(Blue, 13), J]), None, "nothing above 13");
        assert_eq!(classify(&[n(Red, 3), n(Red, 5), n(Red, 4)]), None, "out of order");
        assert_eq!(classify(&[n(Red, 3), n(Blue, 4), n(Red, 5)]), None, "two colors");
        assert_eq!(classify(&[n(Red, 3), n(Red, 3), n(Red, 3)]), None, "repeated number, one color");
    }

    #[test]
    fn groups() {
        assert_eq!(classify(&[n(Red, 7), n(Blue, 7), n(Black, 7)]), Some(SetKind::Group));
        assert_eq!(classify(&[n(Red, 7), J, n(Black, 7), n(Orange, 7)]), Some(SetKind::Group));
        assert_eq!(classify(&[J, J, n(Black, 7)]), Some(SetKind::Run), "also a run: 7, 8, 9 or 5, 6, 7");
        assert_eq!(classify(&[n(Red, 7), n(Red, 7), n(Black, 7)]), None, "repeated color");
        assert_eq!(classify(&[n(Red, 7), n(Blue, 8), n(Black, 7)]), None, "two numbers");
        assert_eq!(classify(&[n(Red, 7), n(Blue, 7), n(Black, 7), n(Orange, 7), J]), None, "five");
    }

    #[test]
    fn too_short_only_jokers_or_impossible_tiles() {
        assert_eq!(classify(&[n(Red, 3), n(Red, 4)]), None);
        assert_eq!(classify(&[J, J, J]), Some(SetKind::Run));
        assert_eq!(classify(&[n(Red, 0), n(Blue, 0), n(Black, 0)]), None, "no 0 tiles");
        assert_eq!(classify(&[n(Red, 14), n(Red, 15), n(Red, 16)]), None, "no 14 tiles");
    }

    #[test]
    fn points_count_jokers_as_what_they_stand_for() {
        assert_eq!(points(&[n(Red, 3), n(Red, 4), n(Red, 5)]), Some(12));
        assert_eq!(points(&[n(Red, 9), J, n(Red, 11)]), Some(30));
        assert_eq!(points(&[J, n(Red, 2), n(Red, 3)]), Some(6));
        assert_eq!(points(&[n(Red, 10), J, n(Black, 10), n(Orange, 10)]), Some(40));
        assert_eq!(points(&[J, J, n(Black, 7)]), Some(21), "three 7s beats 5-6-7");
        assert_eq!(points(&[n(Black, 2), J, J]), Some(9), "2-3-4 beats three 2s");
        assert_eq!(points(&[n(Red, 10), n(Red, 12)]), None);
    }
}

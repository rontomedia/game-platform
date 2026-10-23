//! The game rules. For now only tiles and checking a single set; turns,
//! the initial meld, scoring and rule variants come later.

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

/// Highest number on a tile; numbers run from 1.
pub const MAX_VALUE: u8 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum Color {
    Black,
    Blue,
    Orange,
    Red,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Tile {
    Number { color: Color, value: u8 },
    /// Stands for any tile its set needs.
    Joker,
}

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
    if tiles.len() < 3 {
        return None;
    }
    if is_run(tiles) {
        Some(SetKind::Run)
    } else if is_group(tiles) {
        Some(SetKind::Group)
    } else {
        None
    }
}

fn numbers(tiles: &[Tile]) -> impl Iterator<Item = (usize, Color, u8)> + '_ {
    tiles.iter().enumerate().filter_map(|(i, tile)| match *tile {
        Tile::Number { color, value } => Some((i, color, value)),
        Tile::Joker => None,
    })
}

fn is_run(tiles: &[Tile]) -> bool {
    // The first number fixes where the run starts; every tile after must
    // be one higher than the place before it.
    let Some((index, color, value)) = numbers(tiles).next() else {
        return tiles.len() <= MAX_VALUE as usize;
    };
    let Some(start) = (value as usize).checked_sub(index).filter(|&s| s >= 1) else {
        return false;
    };
    start + tiles.len() - 1 <= MAX_VALUE as usize
        && numbers(tiles).all(|(i, c, v)| c == color && v as usize == start + i)
}

fn is_group(tiles: &[Tile]) -> bool {
    if tiles.len() > 4 {
        return false;
    }
    let mut colors = Vec::new();
    let mut value = None;
    for (_, color, v) in numbers(tiles) {
        if colors.contains(&color) || value.is_some_and(|value| value != v) {
            return false;
        }
        colors.push(color);
        value = Some(v);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use Color::*;

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    const J: Tile = Tile::Joker;

    #[test]
    fn runs() {
        assert_eq!(classify(&[n(Red, 3), n(Red, 4), n(Red, 5)]), Some(SetKind::Run));
        assert_eq!(classify(&[n(Red, 3), J, n(Red, 5)]), Some(SetKind::Run));
        assert_eq!(classify(&[J, n(Blue, 1), n(Blue, 2)]), None, "nothing below 1");
        assert_eq!(classify(&[n(Blue, 12), n(Blue, 13), J]), None, "nothing above 13");
        assert_eq!(classify(&[n(Red, 3), n(Red, 5), n(Red, 4)]), None, "out of order");
        assert_eq!(classify(&[n(Red, 3), n(Blue, 4), n(Red, 5)]), None, "two colors");
    }

    #[test]
    fn groups() {
        assert_eq!(classify(&[n(Red, 7), n(Blue, 7), n(Black, 7)]), Some(SetKind::Group));
        assert_eq!(classify(&[n(Red, 7), J, n(Black, 7), n(Orange, 7)]), Some(SetKind::Group));
        assert_eq!(classify(&[n(Red, 7), n(Red, 7), n(Black, 7)]), None, "repeated color");
        assert_eq!(classify(&[n(Red, 7), n(Blue, 7), n(Black, 7), n(Orange, 7), J]), None, "five");
    }

    #[test]
    fn too_short_or_only_jokers() {
        assert_eq!(classify(&[n(Red, 3), n(Red, 4)]), None);
        assert_eq!(classify(&[J, J, J]), Some(SetKind::Run));
    }
}

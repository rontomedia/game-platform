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

impl Color {
    pub const ALL: [Color; 4] = [Color::Black, Color::Blue, Color::Orange, Color::Red];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Tile {
    Number { color: Color, value: u8 },
    /// Stands for any tile its set needs.
    Joker,
}

impl Tile {
    /// Whether the tile exists in the game. Tiles arrive from clients, so
    /// a number can be anything.
    pub fn exists(self) -> bool {
        match self {
            Tile::Number { value, .. } => (1..=MAX_VALUE).contains(&value),
            Tile::Joker => true,
        }
    }

    /// What the tile costs its holder when the game ends with it on the
    /// rack.
    pub fn penalty(self, joker_penalty: u32) -> u32 {
        match self {
            Tile::Number { value, .. } => value as u32,
            Tile::Joker => joker_penalty,
        }
    }
}

/// Every tile of a game: each number in each color twice, and two jokers.
pub fn full_set() -> Vec<Tile> {
    let mut tiles = Vec::with_capacity(KINDS * 2);
    for _ in 0..2 {
        for color in Color::ALL {
            tiles.extend((1..=MAX_VALUE).map(|value| Tile::Number { color, value }));
        }
        tiles.push(Tile::Joker);
    }
    tiles
}

/// Distinct tiles: each color-number pair, and the joker.
const KINDS: usize = Color::ALL.len() * MAX_VALUE as usize + 1;

/// How many of each tile a collection holds: comparing two collections
/// regardless of order. Only for tiles that `exist`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Counts([u32; KINDS]);

impl Counts {
    pub fn of<'a>(tiles: impl IntoIterator<Item = &'a Tile>) -> Counts {
        let mut counts = Counts([0; KINDS]);
        for &tile in tiles {
            counts.0[index(tile)] += 1;
        }
        counts
    }

    /// The tiles in `self` beyond those in `other`, or `None` if `other`
    /// has a tile `self` lacks.
    pub fn minus(&self, other: &Counts) -> Option<Counts> {
        let mut rest = self.clone();
        for (r, o) in rest.0.iter_mut().zip(other.0) {
            *r = r.checked_sub(o)?;
        }
        Some(rest)
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&n| n == 0)
    }

    /// How many of `tile` there are.
    pub fn count(&self, tile: Tile) -> u32 {
        self.0[index(tile)]
    }

    /// The tiles, in a fixed order.
    pub fn tiles(&self) -> Vec<Tile> {
        let mut tiles = Vec::new();
        for (i, &n) in self.0.iter().enumerate() {
            tiles.extend(std::iter::repeat_n(tile_at(i), n as usize));
        }
        tiles
    }
}

fn index(tile: Tile) -> usize {
    match tile {
        Tile::Number { color, value } => color as usize * MAX_VALUE as usize + (value - 1) as usize,
        Tile::Joker => KINDS - 1,
    }
}

fn tile_at(index: usize) -> Tile {
    if index == KINDS - 1 {
        return Tile::Joker;
    }
    Tile::Number { color: Color::ALL[index / MAX_VALUE as usize], value: (index % MAX_VALUE as usize) as u8 + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_set_has_106_tiles() {
        let tiles = full_set();
        assert_eq!(tiles.len(), 106);
        assert_eq!(tiles.iter().filter(|t| **t == Tile::Joker).count(), 2);
        assert!(tiles.iter().all(|t| t.exists()));
    }

    #[test]
    fn index_round_trips() {
        for tile in full_set() {
            assert_eq!(tile_at(index(tile)), tile);
        }
    }

    #[test]
    fn counts_minus() {
        let red5 = Tile::Number { color: Color::Red, value: 5 };
        let all = Counts::of(&[red5, red5, Tile::Joker]);
        let some = Counts::of(&[red5]);
        assert_eq!(all.minus(&some).unwrap().tiles(), vec![red5, Tile::Joker]);
        assert!(some.minus(&all).is_none());
    }
}

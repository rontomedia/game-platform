//! Plain Okey ("düz okey"): Okey 101's tiles, indicator and okey, without
//! a table. A turn is draw (from the pool, or the previous player's
//! discard), discard; a player finishes by discarding when the other 14
//! tiles are all in runs and groups, or seven pairs. Runs may end 12-13-1
//! (1 low or high, not both). Each player starts with 20 points; a win
//! costs each other player 2, or 4 finishing on the okey or with seven
//! pairs (8 for both), and holding the indicator's twin at the deal costs
//! the others 1. The game ends when someone reaches 0: the highest win.
//!
//! Rules from pagat.com's "Okey" (read 2026-10-28). The indicator's twin
//! is shown for its holder at the deal (it can only gain them a point); a
//! pool that runs out deals the hand again.

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::bot::BotLevel;
use crate::okey101::{okey_for, plain, OkeyError, Phase};
use crate::rng::Rng;
use crate::tile::{full_set, Color, Counts, Tile, MAX_VALUE};

/// The settings of a plain Okey game; any left out take their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(default, rename_all = "camelCase")]
pub struct PlainRules {
    /// The points each player starts with; the game ends when someone has
    /// none left.
    pub points: u32,
}

impl Default for PlainRules {
    fn default() -> PlainRules {
        PlainRules { points: 20 }
    }
}

/// What a win costs each other player, before doubling.
const WIN: i32 = 2;
/// Tiles dealt to the player who starts; the others get one fewer.
const STARTER_TILES: usize = 15;
/// A complete hand.
const HAND: usize = 14;

/// The end of a turn: the tile discarded, having taken the previous
/// player's discard instead of drawing if `take_discard`; with `finish`,
/// the other 14 tiles are shown as a winning hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PlainTurn {
    #[serde(default)]
    pub take_discard: bool,
    pub discard: Tile,
    #[serde(default)]
    pub finish: bool,
}

/// How a winning hand is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum Finish {
    /// Runs and groups.
    Sets,
    /// Seven pairs.
    Pairs,
}

/// How a hand ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PlainHandEnd {
    /// Who finished; absent when the pool ran out (the hand is dealt again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub winner: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub finish: Option<Finish>,
    /// Whether the last discard was the okey.
    pub okey_last: bool,
    /// The winner's 14 tiles, shown.
    pub hand: Vec<Tile>,
    /// Points each player lost this hand (indicator shows included).
    pub lost: Vec<u32>,
}

/// What one player may know about a plain Okey game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PlainView {
    /// This player's seat; absent for someone watching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub you: Option<usize>,
    pub rules: PlainRules,
    pub indicator: Tile,
    pub okey: Tile,
    pub rack: Vec<Tile>,
    pub rack_sizes: Vec<usize>,
    /// Each player's discards, oldest first; the last of the previous
    /// player's may be taken.
    pub discards: Vec<Vec<Tile>>,
    /// Who showed the indicator's twin this hand.
    pub showed: Vec<bool>,
    pub pool: usize,
    pub current: usize,
    pub phase: Phase,
    /// Points left, the current hand's not yet counted.
    pub points: Vec<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub end: Option<PlainHandEnd>,
    /// The highest, once someone has no points left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub winners: Option<Vec<usize>>,
}

/// A plain Okey game: hands until someone has no points left.
#[derive(Clone, Debug)]
pub struct PlainGame {
    rules: PlainRules,
    seed: u64,
    /// Hands dealt: seeds each deal.
    dealt: u64,
    starter: usize,
    points: Vec<i32>,
    indicator: Tile,
    okey: Tile,
    pool: Vec<Tile>,
    racks: Vec<Vec<Tile>>,
    discards: Vec<Vec<Tile>>,
    showed: Vec<bool>,
    current: usize,
    phase: Phase,
    end: Option<PlainHandEnd>,
}

impl PlainGame {
    /// Panics unless there are 2 to 4 players.
    pub fn new(players: usize, rules: PlainRules, seed: u64) -> PlainGame {
        assert!((2..=4).contains(&players), "2 to 4 players, not {players}");
        let points = vec![rules.points as i32; players];
        let mut game = PlainGame {
            rules,
            seed,
            dealt: 0,
            starter: 0,
            points,
            indicator: Tile::Joker,
            okey: Tile::Joker,
            pool: Vec::new(),
            racks: Vec::new(),
            discards: Vec::new(),
            showed: Vec::new(),
            current: 0,
            phase: Phase::Play,
            end: None,
        };
        game.deal();
        game
    }

    /// A fresh hand from `starter`: 15 tiles to them, 14 to the others, the
    /// indicator turned up (never a joker) and its twin shown.
    fn deal(&mut self) {
        let players = self.points.len();
        let mut pool = full_set();
        Rng::new(self.seed ^ self.dealt.wrapping_mul(0xA24B_AED4_963E_E407)).shuffle(&mut pool);
        self.dealt += 1;
        let at = pool.iter().rposition(|t| matches!(t, Tile::Number { .. })).expect("the set has numbers");
        self.indicator = pool.remove(at);
        self.okey = okey_for(self.indicator);
        self.racks = (0..players)
            .map(|p| {
                let size = if p == self.starter { STARTER_TILES } else { STARTER_TILES - 1 };
                pool.split_off(pool.len() - size)
            })
            .collect();
        self.pool = pool;
        self.discards = vec![Vec::new(); players];
        self.showed = self.racks.iter().map(|rack| rack.contains(&self.indicator)).collect();
        self.current = self.starter;
        self.phase = Phase::Play;
        self.end = None;
    }

    pub fn players(&self) -> usize {
        self.points.len()
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn okey(&self) -> Tile {
        self.okey
    }

    pub fn rack(&self, player: usize) -> &[Tile] {
        &self.racks[player]
    }

    pub fn end(&self) -> Option<&PlainHandEnd> {
        self.end.as_ref()
    }

    pub fn points(&self) -> &[i32] {
        &self.points
    }

    pub fn is_over(&self) -> bool {
        self.points.iter().any(|&p| p <= 0)
    }

    /// The players with the most points, once someone has none left.
    pub fn winners(&self) -> Option<Vec<usize>> {
        let best = *self.points.iter().max()?;
        self.is_over().then(|| (0..self.players()).filter(|&p| self.points[p] == best).collect())
    }

    /// The tile the current player may take instead of drawing.
    pub fn discard_on_offer(&self) -> Option<Tile> {
        self.discards[self.previous()].last().copied()
    }

    /// The player draws from the pool. With the pool empty the hand ends
    /// with no winner, and is dealt again.
    pub fn draw(&mut self, player: usize) -> Result<Option<Tile>, OkeyError> {
        self.check_turn_of(player)?;
        if self.phase != Phase::Draw {
            return Err(OkeyError::AlreadyDrawn);
        }
        let Some(tile) = self.pool.pop() else {
            let lost = vec![0; self.players()];
            self.end = Some(PlainHandEnd { winner: None, finish: None, okey_last: false, hand: Vec::new(), lost });
            return Ok(None);
        };
        self.racks[player].push(tile);
        self.phase = Phase::Play;
        Ok(Some(tile))
    }

    /// Ends the player's turn. A rejected turn changes nothing.
    pub fn play(&mut self, player: usize, turn: PlainTurn) -> Result<(), OkeyError> {
        self.check_turn_of(player)?;
        let (rack, finish) = self.check(player, &turn)?;
        if turn.take_discard {
            let previous = self.previous();
            self.discards[previous].pop();
        }
        self.racks[player] = rack;
        self.discards[player].push(turn.discard);
        if let Some(finish) = finish {
            return self.finish(player, finish, turn.discard == self.okey);
        }
        self.current = (self.current + 1) % self.players();
        self.phase = Phase::Draw;
        Ok(())
    }

    /// Checks `turn` for `player` without changing anything.
    pub fn check_turn(&self, player: usize, turn: &PlainTurn) -> Result<(), OkeyError> {
        self.check_turn_of(player)?;
        self.check(player, turn).map(|_| ())
    }

    /// Deals the next hand once this one has ended and the game goes on.
    /// The start moves on, unless the pool ran out. Returns whether it dealt.
    pub fn next_hand(&mut self) -> bool {
        let Some(end) = &self.end else { return false };
        if self.is_over() {
            return false;
        }
        if end.winner.is_some() {
            self.starter = (self.starter + 1) % self.players();
        }
        self.deal();
        true
    }

    pub fn view(&self, player: usize) -> PlainView {
        PlainView { you: Some(player), rack: self.racks[player].clone(), ..self.watcher_view() }
    }

    pub fn watcher_view(&self) -> PlainView {
        PlainView {
            you: None,
            rules: self.rules.clone(),
            indicator: self.indicator,
            okey: self.okey,
            rack: Vec::new(),
            rack_sizes: self.racks.iter().map(Vec::len).collect(),
            discards: self.discards.clone(),
            showed: self.showed.clone(),
            pool: self.pool.len(),
            current: self.current,
            phase: self.phase,
            points: self.points.clone(),
            end: self.end.clone(),
            winners: self.winners(),
        }
    }

    fn previous(&self) -> usize {
        (self.current + self.players() - 1) % self.players()
    }

    fn check_turn_of(&self, player: usize) -> Result<(), OkeyError> {
        if self.end.is_some() {
            Err(OkeyError::HandOver)
        } else if player != self.current {
            Err(OkeyError::NotYourTurn)
        } else {
            Ok(())
        }
    }

    /// The rack after `turn`, and how it finishes, if it does.
    fn check(&self, player: usize, turn: &PlainTurn) -> Result<(Vec<Tile>, Option<Finish>), OkeyError> {
        if !turn.discard.exists() {
            return Err(OkeyError::NoSuchTile);
        }
        let mut rack = self.racks[player].clone();
        match (turn.take_discard, self.phase) {
            (true, Phase::Play) => return Err(OkeyError::AlreadyDrawn),
            (false, Phase::Draw) => return Err(OkeyError::DrawFirst),
            (true, Phase::Draw) => rack.push(self.discard_on_offer().ok_or(OkeyError::NoDiscard)?),
            (false, Phase::Play) => {}
        }
        let at = rack.iter().position(|&t| t == turn.discard).ok_or(OkeyError::TileNotOnRack)?;
        rack.remove(at);
        let finish = if turn.finish { Some(complete(self.okey, &rack).ok_or(OkeyError::NotComplete)?) } else { None };
        Ok((rack, finish))
    }

    /// `winner` showed `finish`: the others lose 2 each, doubled for the okey
    /// discarded last and for seven pairs; and 1 for each indicator shown
    /// by someone else. Then the points are counted.
    fn finish(&mut self, winner: usize, finish: Finish, okey_last: bool) -> Result<(), OkeyError> {
        let times = if okey_last { 2 } else { 1 } * if finish == Finish::Pairs { 2 } else { 1 };
        let players = self.players();
        let lost: Vec<u32> = (0..players)
            .map(|p| {
                let win = if p == winner { 0 } else { WIN * times };
                let shows = (0..players).filter(|&q| q != p && self.showed[q]).count() as i32;
                (win + shows) as u32
            })
            .collect();
        for (points, &lost) in self.points.iter_mut().zip(&lost) {
            *points -= lost as i32;
        }
        let hand = self.racks[winner].clone();
        self.end = Some(PlainHandEnd { winner: Some(winner), finish: Some(finish), okey_last, hand, lost });
        Ok(())
    }
}

/// How `tiles` (14) win, if they do: all in runs and groups, or seven
/// pairs. The okey is wild, a false okey the okey's tile.
pub fn complete(okey: Tile, tiles: &[Tile]) -> Option<Finish> {
    if tiles.len() != HAND {
        return None;
    }
    let plain = plain(okey, tiles);
    let jokers = plain.iter().filter(|&&t| t == Tile::Joker).count();
    let mut numbers: Vec<Tile> = plain.into_iter().filter(|&t| t != Tile::Joker).collect();
    sort(&mut numbers);
    if cover(&numbers, jokers, true) == numbers.len() {
        return Some(Finish::Sets);
    }
    (pairs(&numbers, jokers) == HAND / 2).then_some(Finish::Pairs)
}

/// Tiles by colour, then number: the first is the lowest of its colour.
fn sort(tiles: &mut [Tile]) {
    tiles.sort_by_key(|t| match t {
        Tile::Number { color, value } => (*color as u8, *value),
        Tile::Joker => (u8::MAX, 0),
    });
}

/// How many pairs `numbers` and `jokers` make: twins, then a joker with a
/// single, then jokers together.
fn pairs(numbers: &[Tile], jokers: usize) -> usize {
    let counts = Counts::of(numbers);
    let mut kinds = numbers.to_vec();
    kinds.dedup();
    let twins: usize = kinds.iter().map(|&t| counts.count(t) as usize / 2).sum();
    let singles: usize = kinds.iter().map(|&t| counts.count(t) as usize % 2).sum();
    let matched = jokers.min(singles);
    twins + matched + (jokers - matched) / 2
}

/// The most of `numbers` (sorted) that runs and groups can hold, with up to
/// `jokers` filling in. With `all`, stops looking once a tile can't be
/// placed (for `complete`, where every tile must be). Jokers left over
/// must fit somewhere, which a set with room always gives.
fn cover(numbers: &[Tile], jokers: usize, all: bool) -> usize {
    let Some((&first, rest)) = numbers.split_first() else { return 0 };
    let Tile::Number { color, value } = first else { unreachable!("numbers only") };
    let mut best = 0;
    // Leave it out.
    if !all {
        best = cover(rest, jokers, false);
    }
    let try_set = |used: &[usize], joker_count: usize, best: &mut usize| {
        // `used`: indices into `rest` of the other tiles in the set.
        let left: Vec<Tile> = rest.iter().enumerate().filter(|(i, _)| !used.contains(i)).map(|(_, &t)| t).collect();
        let placed = 1 + used.len() + cover(&left, jokers - joker_count, all);
        *best = (*best).max(placed);
    };
    // Groups: its number in other colours.
    let others: Vec<usize> = rest
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, Tile::Number { color: c, value: v } if *v == value && *c != color))
        .map(|(i, _)| i)
        .collect();
    for mask in 0u32..(1 << others.len()) {
        let chosen: Vec<usize> = (0..others.len()).filter(|&b| mask & (1 << b) != 0).map(|b| others[b]).collect();
        let mut colors: Vec<Color> = vec![color];
        let distinct = chosen.iter().all(|&i| match rest[i] {
            Tile::Number { color: c, .. } if !colors.contains(&c) => {
                colors.push(c);
                true
            }
            _ => false,
        });
        if !distinct {
            continue;
        }
        for joker_count in 0..=jokers {
            let size = 1 + chosen.len() + joker_count;
            if (3..=4).contains(&size) {
                try_set(&chosen, joker_count, &mut best);
            }
        }
    }
    // Runs of its colour with it the lowest number (jokers may come before),
    // up to 13 or on to 1; or, for a 1, ending at it after 12-13.
    let find = |v: u8, taken: &[usize]| {
        rest.iter().enumerate().position(|(i, t)| !taken.contains(&i) && *t == Tile::Number { color, value: v })
    };
    for before in 0..=jokers.min(value as usize - 1) {
        let mut used = Vec::new();
        let mut joker_count = before;
        let mut next = value as usize + 1;
        while next <= MAX_VALUE as usize + 1 {
            // 14 is the 1 after 13, if the run doesn't start at 1.
            let v = if next == MAX_VALUE as usize + 1 {
                if value - before as u8 == 1 {
                    break;
                }
                1
            } else {
                next as u8
            };
            match find(v, &used) {
                Some(i) => used.push(i),
                None if joker_count < jokers => joker_count += 1,
                None => break,
            }
            next += 1;
            if 1 + used.len() + joker_count >= 3 {
                try_set(&used, joker_count, &mut best);
            }
        }
    }
    if value == 1 {
        // 1 at the top: ...12, 13, 1.
        let mut used = Vec::new();
        let mut joker_count = 0;
        for v in (2..=MAX_VALUE).rev() {
            match find(v, &used) {
                Some(i) => used.push(i),
                None if joker_count < jokers => joker_count += 1,
                None => break,
            }
            if 1 + used.len() + joker_count >= 3 {
                try_set(&used, joker_count, &mut best);
            }
        }
    }
    best
}

/// A computer player's move in plain Okey.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlainMove {
    Draw,
    Turn(PlainTurn),
}

/// The turn played for the player whose view this is when their time runs
/// out, once they have drawn: a finish if their hand wins; else the first
/// of `next`, the tiles they set aside to throw next in order, that they
/// hold (finishing if the other 14 win); else the discard an Easy bot would
/// make.
pub fn timeout_turn(view: &PlainView, next: &[Tile]) -> Option<PlainTurn> {
    let PlainMove::Turn(bot) = plain_bot_move(view, BotLevel::Easy) else { return None };
    let held = next.iter().find_map(|tile| view.rack.iter().position(|t| t == tile));
    let Some(at) = held.filter(|_| !bot.finish) else {
        return Some(bot);
    };
    let mut rest = view.rack.clone();
    let discard = rest.remove(at);
    Some(PlainTurn { take_discard: false, discard, finish: complete(view.okey, &rest).is_some() })
}

/// The move of the player whose view this is, on their turn. It finishes
/// when it can (on the okey when that also wins, as it doubles); else
/// keeps the 14 tiles that fit runs and groups (or pairs) best. Hard and
/// Extra Hard take the previous discard whenever it improves their hand,
/// Easy and Medium only to finish with it.
pub fn plain_bot_move(view: &PlainView, level: BotLevel) -> PlainMove {
    let Some(_) = view.you else { return PlainMove::Draw };
    let okey = view.okey;
    match view.phase {
        Phase::Play => PlainMove::Turn(best_turn(okey, &view.rack).0),
        Phase::Draw => {
            let previous = (view.current + view.rack_sizes.len() - 1) % view.rack_sizes.len();
            if let Some(tile) = view.discards[previous].last().copied() {
                let mut rack = view.rack.clone();
                rack.push(tile);
                let (turn, worth) = best_turn(okey, &rack);
                let careful = matches!(level, BotLevel::Hard | BotLevel::ExtraHard);
                if turn.finish || (careful && turn.discard != tile && worth > hand_worth(okey, &view.rack)) {
                    return PlainMove::Turn(PlainTurn { take_discard: true, ..turn });
                }
            }
            PlainMove::Draw
        }
    }
}

/// The best discard from 15 tiles: one that finishes (the okey first),
/// else the one leaving the hand worth most; and that worth.
fn best_turn(okey: Tile, rack: &[Tile]) -> (PlainTurn, (usize, usize)) {
    let mut kinds = rack.to_vec();
    sort(&mut kinds);
    kinds.dedup();
    // The okey first: finishing on it doubles.
    kinds.sort_by_key(|&t| t != okey);
    let rest_without = |tile: Tile| {
        let mut rest = rack.to_vec();
        let at = rest.iter().position(|&t| t == tile).expect("on the rack");
        rest.remove(at);
        rest
    };
    for &tile in &kinds {
        if complete(okey, &rest_without(tile)).is_some() {
            return (PlainTurn { take_discard: false, discard: tile, finish: true }, (HAND, HAND));
        }
    }
    let (tile, worth) = kinds
        .iter()
        // Never the okey unless finishing: it's wild.
        .filter(|&&t| t != okey || kinds.len() == 1)
        .map(|&tile| (tile, hand_worth(okey, &rest_without(tile))))
        .max_by_key(|&(tile, worth)| (worth, std::cmp::Reverse(value_of(tile))))
        .expect("a rack has tiles");
    (PlainTurn { take_discard: false, discard: tile, finish: false }, worth)
}

/// How close `tiles` are to winning: the most in runs and groups, then
/// the pairs.
fn hand_worth(okey: Tile, tiles: &[Tile]) -> (usize, usize) {
    let plain = plain(okey, tiles);
    let jokers = plain.iter().filter(|&&t| t == Tile::Joker).count();
    let mut numbers: Vec<Tile> = plain.into_iter().filter(|&t| t != Tile::Joker).collect();
    sort(&mut numbers);
    (cover(&numbers, jokers, false), pairs(&numbers, jokers))
}

fn value_of(tile: Tile) -> u8 {
    match tile {
        Tile::Number { value, .. } => value,
        Tile::Joker => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::Color::*;

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    /// Red 5, as if the indicator were red 4.
    const OKEY: Tile = Tile::Number { color: Red, value: 5 };

    #[test]
    fn winning_hands() {
        // 1-2-3 blue, 12-13-1 black, three 7s, 9-10-11-12 orange, and a 4.
        let mut hand = vec![n(Blue, 1), n(Blue, 2), n(Blue, 3), n(Black, 12), n(Black, 13), n(Black, 1)];
        hand.extend([n(Blue, 7), n(Black, 7), n(Orange, 7), n(Orange, 9), n(Orange, 10), n(Orange, 11), n(Orange, 12)]);
        hand.push(n(Blue, 4));
        assert_eq!(complete(OKEY, &hand), Some(Finish::Sets), "12-13-1 is a run");
        hand[13] = n(Red, 9);
        assert_eq!(complete(OKEY, &hand), None);
        hand[13] = OKEY;
        assert_eq!(complete(OKEY, &hand), Some(Finish::Sets), "the okey fits anywhere");
        hand[13] = Tile::Joker;
        assert_eq!(complete(OKEY, &hand), None, "a false okey is only the red 5");

        // Complete only if black 13-1-2 were a run.
        let mut wrap = vec![n(Black, 13), n(Black, 1), n(Black, 2)];
        wrap.extend([Blue, Orange].iter().flat_map(|&c| [n(c, 9), n(c, 10), n(c, 11)]));
        wrap.extend((9..=13).map(|v| n(Red, v)));
        assert_eq!(complete(OKEY, &wrap), None, "13-1-2 isn't a run");

        let pairs: Vec<Tile> = (1..=6).flat_map(|v| [n(Blue, v), n(Blue, v)]).chain([n(Black, 9), OKEY]).collect();
        assert_eq!(complete(OKEY, &pairs), Some(Finish::Pairs));
    }

    #[test]
    fn a_game_of_bots_ends() {
        for seed in 0..8 {
            let players = 2 + seed as usize % 3;
            let mut game = PlainGame::new(players, PlainRules { points: 6 }, seed);
            let mut hands = 0;
            loop {
                let mut turns = 0;
                while game.end().is_none() {
                    let player = game.current();
                    let level = if player.is_multiple_of(2) { BotLevel::Hard } else { BotLevel::Easy };
                    match plain_bot_move(&game.view(player), level) {
                        PlainMove::Draw => {
                            game.draw(player).unwrap();
                        }
                        PlainMove::Turn(turn) => game.play(player, turn).expect("the bot's turns are valid"),
                    }
                    turns += 1;
                    assert!(turns < 300, "seed {seed}: a hand doesn't end");
                }
                hands += 1;
                assert!(hands < 60, "seed {seed}: the game doesn't end");
                if !game.next_hand() {
                    break;
                }
            }
            assert!(game.is_over());
            assert!(game.winners().is_some_and(|w| !w.is_empty()));
        }
    }

    #[test]
    fn scoring() {
        let mut game = PlainGame::new(3, PlainRules::default(), 1);
        game.okey = OKEY;
        game.showed = vec![false, true, false];
        let mut hand = vec![n(Blue, 1), n(Blue, 2), n(Blue, 3), n(Black, 12), n(Black, 13), n(Black, 1)];
        hand.extend([n(Blue, 7), n(Black, 7), n(Orange, 7), n(Orange, 9), n(Orange, 10), n(Orange, 11), n(Orange, 12)]);
        // A complete 14 with blue 1-4, and a red 9 that fits nothing.
        hand.extend([n(Blue, 4), n(Red, 9)]);
        game.racks[0] = hand;
        game.current = 0;
        game.phase = Phase::Play;
        assert_eq!(
            game.play(0, PlainTurn { take_discard: false, discard: n(Blue, 7), finish: true }),
            Err(OkeyError::NotComplete)
        );
        game.play(0, PlainTurn { take_discard: false, discard: n(Red, 9), finish: true }).unwrap();
        let end = game.end().unwrap();
        assert_eq!((end.finish, end.okey_last), (Some(Finish::Sets), false));
        // 2 each for the win; and 1 to those who didn't show for player 1's indicator.
        assert_eq!(end.lost, vec![1, 2, 3]);
        assert_eq!(game.points(), &[19, 18, 17]);
    }

    #[test]
    fn when_time_runs_out_the_tile_set_aside_is_discarded() {
        let mut game = PlainGame::new(2, PlainRules::default(), 1);
        game.okey = OKEY;
        let mut hand = vec![n(Blue, 1), n(Blue, 2), n(Blue, 3), n(Black, 12), n(Black, 13), n(Black, 1)];
        hand.extend([n(Blue, 7), n(Black, 7), n(Orange, 7), n(Orange, 9), n(Orange, 10), n(Orange, 11), n(Orange, 12)]);
        hand.extend([n(Blue, 4), n(Red, 9)]);
        game.racks[0] = hand;
        game.current = 0;
        game.phase = Phase::Play;
        // A winning hand finishes whatever was set aside.
        let turn = timeout_turn(&game.view(0), &[n(Blue, 7)]).unwrap();
        assert!(turn.finish);
        assert_eq!(turn.discard, n(Red, 9));
        // Not a winning hand any more: the first tile set aside that's held.
        game.racks[0][13] = n(Red, 11);
        let turn = timeout_turn(&game.view(0), &[n(Black, 3), n(Blue, 7)]).unwrap();
        assert_eq!((turn.discard, turn.finish), (n(Blue, 7), false));
        let turn = timeout_turn(&game.view(0), &[n(Black, 3)]).unwrap();
        assert_ne!(turn.discard, n(Black, 3), "none held: the bot's choice");
    }
}

//! Okey 101: the same tiles, played another way. An indicator tile turned
//! up picks the okey (the tile one higher in its colour, 1 after 13): both
//! of those are wild, and the two jokers ("false okeys") stand for the okey
//! tile itself. A turn is draw (from the pool, or the previous player's
//! discard if it goes on the table at once), meld, discard. A player opens
//! with sets worth 101 or with five pairs (each opening after must beat
//! the last), can add to any set on the table but never rearrange it, and
//! goes out by melding all but one tile and discarding it. Scores are
//! penalty points, lowest wins, over several hands (`Match`).
//!
//! Rules from four descriptions read 2026-10-28: Turkish Wikipedia's
//! "Okey 101", English Wikipedia's "Okey", pagat.com's Okey 101 and
//! okey.gg's rules, scoring and penalties pages. Where most agree, theirs;
//! where only pagat and okey.gg speak and differ, okey.gg's (a complete
//! set of rules played online today). Not built: 2-against-2 games, a
//! false okey as the indicator ("riziko", dealt again here as pagat and
//! okey.gg do), and claiming the indicator with its twin.

use std::fmt;

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::rng::Rng;
use crate::set::{classify, points, SetKind};
use crate::tile::{full_set, Counts, Tile, MAX_VALUE};

mod bot;

pub use bot::{bot_move, OkeyMove};

/// The settings of an Okey 101 match; any left out take their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(default, rename_all = "camelCase")]
pub struct Okey101Rules {
    /// Hands in a match (Wikipedia: 3 to 11; okey.gg: 7 to 21).
    pub hands: u32,
    /// Each opening must beat the one before it this hand, points and
    /// pairs counted apart (after 120 points, 121; after 6 pairs, 7), as
    /// okey.gg plays by default. Off: always 101 points or 5 pairs, as
    /// pagat and Wikipedia describe.
    pub progressive: bool,
    /// Whether penalties during play are written: wrong discards, an okey
    /// taken from your set, the next player opening with your discard.
    /// The scores at the end of a hand always count.
    pub penalties: bool,
}

impl Default for Okey101Rules {
    fn default() -> Okey101Rules {
        Okey101Rules { hands: 7, progressive: false, penalties: true }
    }
}

/// What a player's sets must be worth together to open with sets, at
/// least.
pub const OPENING_POINTS: u32 = 101;
/// Pairs a player must lay down to open with pairs, at least.
pub const OPENING_PAIRS: usize = 5;
/// The unit of the scores: a penalty, an okey left on a rack, a reward.
pub const PENALTY: i32 = 101;
/// Opening with this many points, or `SILER_PAIRS` pairs, earns -101
/// ("Siler").
const SILER_POINTS: u32 = 150;
const SILER_PAIRS: usize = 7;
/// Tiles a turn may add to one end of a run on the table.
const RUN_END_TILES: usize = 2;
/// Tiles dealt to the player who starts; the others get one fewer.
const STARTER_TILES: usize = 22;

/// How a player opened, if they have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum Opened {
    No,
    /// With runs and groups: may lay down runs and groups from then on,
    /// never pairs.
    Sets,
    /// With pairs: may lay down pairs from then on, and add to the runs
    /// and groups on the table, but not lay down new ones.
    Pairs,
}

/// The part of their turn a player is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// To draw from the pool, or to take the previous player's discard
    /// with a `Turn` that puts it on the table.
    Draw,
    /// Drawn (or starting the hand): to meld and discard with a `Turn`.
    Play,
}

/// The end of a player's turn, sent whole: the table after it and the
/// tile discarded. `take_discard` instead of drawing: the previous
/// player's discard is taken and must be on the table at the end. A turn
/// that can't be played changes nothing, so a player may try the discard
/// on their rack and still draw instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    #[serde(default)]
    pub take_discard: bool,
    /// The table's sets as they were, in the same order, each the same or
    /// added to at its ends; then any new sets.
    pub table: Vec<Vec<Tile>>,
    pub discard: Tile,
}

/// Why a move isn't allowed in Okey 101.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OkeyError {
    NotYourTurn,
    HandOver,
    /// A tile that isn't in the game, such as a red 14.
    NoSuchTile,
    /// Draw, or take the discard, before ending the turn.
    DrawFirst,
    /// Already drawn this turn: the discard can't be taken too.
    AlreadyDrawn,
    /// The previous player has no discard to take.
    NoDiscard,
    /// A discarded okey can't be taken.
    OkeyDiscard,
    /// The set at this place isn't a run, group or pair.
    InvalidSet { index: usize },
    /// The table's set at this place was changed other than by adding
    /// tiles at its ends (at most two at each end of a run) or taking an
    /// okey from it. Pairs can't be added to.
    SetChanged { index: usize },
    /// An okey can be taken from the set at this place only by a player
    /// who opened on an earlier turn, and from a group only once it has
    /// all four colours.
    OkeyNotFree { index: usize },
    /// A tile left the table.
    TileTakenFromTable,
    /// A tile was played or discarded that the player's rack doesn't hold.
    TileNotOnRack,
    /// Only a player who has opened may add to the table's sets.
    NotOpened,
    /// Runs and groups and pairs can't be mixed: a player opens with one
    /// or the other and keeps to it.
    SetsAndPairs,
    OpeningTooLow { points: u32, needed: u32 },
    TooFewPairs { pairs: usize, needed: usize },
    /// The previous player's discard was taken but isn't on the table (or,
    /// for a player opening, isn't in their opening).
    DiscardNotUsed,
    /// Plain Okey: finishing with a hand that isn't 14 tiles in sets or
    /// seven pairs.
    NotComplete,
}

impl fmt::Display for OkeyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OkeyError::NotYourTurn => write!(f, "it isn't your turn"),
            OkeyError::HandOver => write!(f, "the hand is over"),
            OkeyError::NoSuchTile => write!(f, "a tile that isn't in the game"),
            OkeyError::DrawFirst => write!(f, "draw a tile or take the discard first"),
            OkeyError::AlreadyDrawn => write!(f, "you have already drawn this turn"),
            OkeyError::NoDiscard => write!(f, "there is no discard to take"),
            OkeyError::OkeyDiscard => write!(f, "a discarded okey can't be taken"),
            OkeyError::InvalidSet { index } => write!(f, "set {} is not a run, group or pair", index + 1),
            OkeyError::SetChanged { index } => write!(f, "set {} may only be added to at its ends", index + 1),
            OkeyError::OkeyNotFree { index } => write!(f, "the okey in set {} can't be taken", index + 1),
            OkeyError::TileTakenFromTable => write!(f, "tiles on the table must stay on the table"),
            OkeyError::TileNotOnRack => write!(f, "a tile that isn't on your rack"),
            OkeyError::NotOpened => write!(f, "open before adding to the table's sets"),
            OkeyError::SetsAndPairs => write!(f, "runs and groups can't be mixed with pairs"),
            OkeyError::OpeningTooLow { points, needed } => {
                write!(f, "your opening is worth {points} points; it needs {needed}")
            }
            OkeyError::TooFewPairs { pairs, needed } => write!(f, "{pairs} pairs; opening needs {needed}"),
            OkeyError::DiscardNotUsed => write!(f, "the discard you took must go on the table"),
            OkeyError::NotComplete => write!(f, "the other 14 tiles must all be in sets, or seven pairs"),
        }
    }
}

impl std::error::Error for OkeyError {}

/// How a hand ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct HandEnd {
    /// Who went out; absent when the pool ran out or the hand is void.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub winner: Option<usize>,
    /// Everyone opened with pairs, so nobody could go out: nothing counts
    /// and the same player starts the hand dealt again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub void: bool,
    /// Points per player for the hand, those written during it included;
    /// the winner's are negative.
    pub scores: Vec<i32>,
}

/// What one player may know about a hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct HandView {
    /// This player's seat; absent for someone watching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub you: Option<usize>,
    pub rules: Okey101Rules,
    pub indicator: Tile,
    /// The wild tile, from the indicator.
    pub okey: Tile,
    pub rack: Vec<Tile>,
    pub table: Vec<Vec<Tile>>,
    pub rack_sizes: Vec<usize>,
    pub opened: Vec<Opened>,
    /// What an opening needs now: points, or pairs.
    pub opening_points: u32,
    pub opening_pairs: usize,
    /// Each player's discards, oldest first; the last of the previous
    /// player's may be taken.
    pub discards: Vec<Vec<Tile>>,
    /// Points written so far this hand: penalties, and -101 rewards.
    pub written: Vec<i32>,
    pub pool: usize,
    pub current: usize,
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub end: Option<HandEnd>,
}

/// One hand, from deal to scores. Seats take turns in order `0..players`
/// from `starter`.
#[derive(Clone, Debug)]
pub struct Hand {
    rules: Okey101Rules,
    indicator: Tile,
    okey: Tile,
    pool: Vec<Tile>,
    racks: Vec<Vec<Tile>>,
    table: Vec<Vec<Tile>>,
    /// Who put each tile of the table where it is: an okey taken from a
    /// set costs that player.
    owners: Vec<Vec<usize>>,
    discards: Vec<Vec<Tile>>,
    opened: Vec<Opened>,
    opening_points: u32,
    opening_pairs: usize,
    written: Vec<i32>,
    current: usize,
    phase: Phase,
    end: Option<HandEnd>,
}

/// What a valid turn does, worked out before anything changes.
struct Checked {
    rack: Vec<Tile>,
    owners: Vec<Vec<usize>>,
    opened: Opened,
    /// Made this turn: points (sets) or pairs.
    opening: Option<(u32, usize)>,
    /// Players whose okeys were taken from the table.
    okeys_taken_from: Vec<usize>,
}

impl Hand {
    /// A shuffled deal; the same seed deals the same hand. `starter` gets
    /// 22 tiles and begins by discarding, the others get 21.
    ///
    /// Panics unless there are 2 to 4 players and `starter` is one of them.
    pub fn new(players: usize, rules: Okey101Rules, seed: u64, starter: usize) -> Hand {
        assert!((2..=4).contains(&players), "2 to 4 players, not {players}");
        assert!(starter < players, "no seat {starter}");
        let mut pool = full_set();
        Rng::new(seed).shuffle(&mut pool);
        // The indicator is turned up from the pool; a joker can't be one.
        let at = pool.iter().rposition(|t| matches!(t, Tile::Number { .. })).expect("the set has numbers");
        let indicator = pool.remove(at);
        let racks = (0..players)
            .map(|p| {
                let size = if p == starter { STARTER_TILES } else { STARTER_TILES - 1 };
                pool.split_off(pool.len() - size)
            })
            .collect();
        Hand::from_deal(racks, pool, indicator, starter, rules)
    }

    fn from_deal(racks: Vec<Vec<Tile>>, pool: Vec<Tile>, indicator: Tile, starter: usize, rules: Okey101Rules) -> Hand {
        let players = racks.len();
        Hand {
            rules,
            indicator,
            okey: okey_for(indicator),
            pool,
            racks,
            table: Vec::new(),
            owners: Vec::new(),
            discards: vec![Vec::new(); players],
            opened: vec![Opened::No; players],
            opening_points: OPENING_POINTS,
            opening_pairs: OPENING_PAIRS,
            written: vec![0; players],
            current: starter,
            phase: Phase::Play,
            end: None,
        }
    }

    pub fn players(&self) -> usize {
        self.racks.len()
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

    pub fn table(&self) -> &[Vec<Tile>] {
        &self.table
    }

    pub fn opened(&self, player: usize) -> Opened {
        self.opened[player]
    }

    /// What an opening needs now: points with sets, or pairs.
    pub fn opening_needs(&self) -> (u32, usize) {
        (self.opening_points, self.opening_pairs)
    }

    pub fn pool_size(&self) -> usize {
        self.pool.len()
    }

    pub fn end(&self) -> Option<&HandEnd> {
        self.end.as_ref()
    }

    /// The tile the current player may take instead of drawing: the
    /// previous player's last discard, unless it's the okey.
    pub fn discard_on_offer(&self) -> Option<Tile> {
        self.discards[self.previous()].last().copied().filter(|&t| t != self.okey)
    }

    /// The player draws from the pool. With the pool empty the hand ends
    /// with no winner (the player didn't take the discard either).
    pub fn draw(&mut self, player: usize) -> Result<Option<Tile>, OkeyError> {
        self.check_turn_of(player)?;
        if self.phase != Phase::Draw {
            return Err(OkeyError::AlreadyDrawn);
        }
        let Some(tile) = self.pool.pop() else {
            self.finish_without_winner();
            return Ok(None);
        };
        self.racks[player].push(tile);
        self.phase = Phase::Play;
        Ok(Some(tile))
    }

    /// Ends the player's turn: their melds and their discard. A rejected
    /// turn changes nothing.
    pub fn play(&mut self, player: usize, turn: Turn) -> Result<(), OkeyError> {
        self.check_turn_of(player)?;
        let checked = self.check(player, &turn)?;
        let previous = self.previous();
        let taken = if turn.take_discard { self.discards[previous].pop() } else { None };
        // A direct finish: going out on the turn of opening, before anyone.
        let nobody_opened = self.opened.iter().all(|&o| o == Opened::No);
        self.table = turn.table;
        self.owners = checked.owners;
        self.racks[player] = checked.rack;
        self.discards[player].push(turn.discard);
        self.opened[player] = checked.opened;
        if let Some((points, pairs)) = checked.opening {
            if self.rules.progressive {
                match checked.opened {
                    Opened::Sets => self.opening_points = points + 1,
                    _ => self.opening_pairs = pairs + 1,
                }
            }
            if points >= SILER_POINTS || pairs >= SILER_PAIRS {
                self.written[player] -= PENALTY;
            }
            if let (Some(tile), true) = (taken, self.rules.penalties) {
                let times = if checked.opened == Opened::Pairs { 20 } else { 10 };
                self.written[previous] += self.face(tile) * times;
            }
        }
        if self.rules.penalties {
            for owner in checked.okeys_taken_from {
                self.written[owner] += PENALTY;
            }
        }
        if self.racks[player].is_empty() {
            self.finish(player, turn.discard, nobody_opened);
            return Ok(());
        }
        // Wrong discards: the okey, or a tile that could go on the table.
        if self.rules.penalties && (turn.discard == self.okey || self.fits_table(turn.discard)) {
            self.written[player] += PENALTY;
        }
        if self.opened.iter().all(|&o| o == Opened::Pairs) {
            self.end = Some(HandEnd { winner: None, void: true, scores: vec![0; self.players()] });
            return Ok(());
        }
        self.current = (self.current + 1) % self.players();
        self.phase = Phase::Draw;
        Ok(())
    }

    /// Checks `turn` for `player` without changing anything.
    pub fn check_turn(&self, player: usize, turn: &Turn) -> Result<(), OkeyError> {
        self.check_turn_of(player)?;
        self.check(player, turn).map(|_| ())
    }

    pub fn view(&self, player: usize) -> HandView {
        HandView { you: Some(player), rack: self.racks[player].clone(), ..self.watcher_view() }
    }

    pub fn watcher_view(&self) -> HandView {
        HandView {
            you: None,
            rules: self.rules.clone(),
            indicator: self.indicator,
            okey: self.okey,
            rack: Vec::new(),
            table: self.table.clone(),
            rack_sizes: self.racks.iter().map(Vec::len).collect(),
            opened: self.opened.clone(),
            opening_points: self.opening_points,
            opening_pairs: self.opening_pairs,
            discards: self.discards.clone(),
            written: self.written.clone(),
            pool: self.pool.len(),
            current: self.current,
            phase: self.phase,
            end: self.end.clone(),
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

    fn check(&self, player: usize, turn: &Turn) -> Result<Checked, OkeyError> {
        let after = &turn.table;
        if !after.iter().flatten().all(|t| t.exists()) || !turn.discard.exists() {
            return Err(OkeyError::NoSuchTile);
        }
        let mut rack = self.racks[player].clone();
        let taken = match (turn.take_discard, self.phase) {
            (true, Phase::Play) => return Err(OkeyError::AlreadyDrawn),
            (false, Phase::Draw) => return Err(OkeyError::DrawFirst),
            (true, Phase::Draw) => {
                let tile = *self.discards[self.previous()].last().ok_or(OkeyError::NoDiscard)?;
                if tile == self.okey {
                    return Err(OkeyError::OkeyDiscard);
                }
                rack.push(tile);
                Some(tile)
            }
            (false, Phase::Play) => None,
        };
        let before = &self.table;
        if after.len() < before.len() {
            return Err(OkeyError::TileTakenFromTable);
        }
        let opened_before = self.opened[player];
        // The table's sets: each the same, or added to at its ends.
        let mut kinds = Vec::with_capacity(after.len());
        let mut owners = Vec::with_capacity(after.len());
        let mut added_to_table = false;
        let mut added_points = 0;
        let mut okeys_taken_from = Vec::new();
        for (index, (old, new)) in before.iter().zip(after).enumerate() {
            let Extension { start, swapped } = self.extension(old, new).ok_or(OkeyError::SetChanged { index })?;
            let kind = self.kind(new).ok_or(OkeyError::InvalidSet { index })?;
            kinds.push(kind);
            let end = new.len() - old.len() - start;
            if kind == MeldKind::Run && (start > RUN_END_TILES || end > RUN_END_TILES) {
                return Err(OkeyError::SetChanged { index });
            }
            if !swapped.is_empty() {
                let complete = kind != MeldKind::Group || new.len() == 4;
                if opened_before == Opened::No || !complete {
                    return Err(OkeyError::OkeyNotFree { index });
                }
                okeys_taken_from.extend(swapped.iter().map(|&j| self.owners[index][j]).filter(|&o| o != player));
            }
            added_to_table |= new.len() > old.len() || !swapped.is_empty();
            added_points += self.points(new).saturating_sub(self.points(old));
            let mut set_owners = vec![player; start];
            set_owners.extend(self.owners[index].iter().enumerate().map(|(j, &o)| if swapped.contains(&j) { player } else { o }));
            set_owners.resize(new.len(), player);
            owners.push(set_owners);
        }
        for (index, set) in after.iter().enumerate().skip(before.len()) {
            kinds.push(self.kind(set).ok_or(OkeyError::InvalidSet { index })?);
            owners.push(vec![player; set.len()]);
        }
        let played = Counts::of(after.iter().flatten())
            .minus(&Counts::of(before.iter().flatten()))
            .ok_or(OkeyError::TileTakenFromTable)?;
        Counts::of(&rack)
            .minus(&played)
            .and_then(|left| left.minus(&Counts::of(&[turn.discard])))
            .ok_or(OkeyError::TileNotOnRack)?;

        let new_sets = &after[before.len()..];
        let new_kinds = &kinds[before.len()..];
        let pairs = new_kinds.iter().filter(|&&k| k == MeldKind::Pair).count();
        let sets = new_kinds.len() - pairs;
        if pairs > 0 && sets > 0 {
            return Err(OkeyError::SetsAndPairs);
        }
        let (opened, opening) = match opened_before {
            Opened::Sets if pairs > 0 => return Err(OkeyError::SetsAndPairs),
            Opened::Pairs if sets > 0 => return Err(OkeyError::SetsAndPairs),
            Opened::Sets | Opened::Pairs => (opened_before, None),
            Opened::No if new_sets.is_empty() => {
                if added_to_table {
                    return Err(OkeyError::NotOpened);
                }
                (Opened::No, None)
            }
            Opened::No if pairs > 0 => {
                if pairs < self.opening_pairs {
                    return Err(OkeyError::TooFewPairs { pairs, needed: self.opening_pairs });
                }
                (Opened::Pairs, Some((0, pairs)))
            }
            Opened::No => {
                // Everything laid down on the opening turn counts, tiles
                // added to the table's sets too.
                let worth = new_sets.iter().map(|set| self.points(set)).sum::<u32>() + added_points;
                if worth < self.opening_points {
                    return Err(OkeyError::OpeningTooLow { points: worth, needed: self.opening_points });
                }
                (Opened::Sets, Some((worth, 0)))
            }
        };
        if let Some(tile) = taken {
            let used = match opened_before {
                // A player opening must use it in their opening.
                Opened::No => new_sets.iter().flatten().any(|&t| t == tile),
                _ => played.count(tile) > 0,
            };
            if !used {
                return Err(OkeyError::DiscardNotUsed);
            }
        }
        Ok(Checked { rack: remove(&rack, &played, turn.discard), owners, opened, opening, okeys_taken_from })
    }

    /// Whether `new` is the table's set `old` added to at its ends, okeys
    /// in it possibly swapped for real tiles. Pairs stay as they are.
    fn extension(&self, old: &[Tile], new: &[Tile]) -> Option<Extension> {
        if old.len() == 2 {
            return (old == new).then_some(Extension { start: 0, swapped: Vec::new() });
        }
        let added = new.len().checked_sub(old.len())?;
        (0..=added).find_map(|start| {
            let mut swapped = Vec::new();
            for (j, (&was, &is)) in old.iter().zip(&new[start..]).enumerate() {
                if was != is {
                    if was != self.okey {
                        return None;
                    }
                    swapped.push(j);
                }
            }
            Some(Extension { start, swapped })
        })
    }

    fn kind(&self, set: &[Tile]) -> Option<MeldKind> {
        kind(self.okey, set)
    }

    fn points(&self, set: &[Tile]) -> u32 {
        set_points(self.okey, set)
    }

    fn fits_table(&self, tile: Tile) -> bool {
        fits(self.okey, &self.table, tile)
    }

    fn face(&self, tile: Tile) -> i32 {
        face(self.okey, tile)
    }

    /// The turn played for `player` when their time runs out, once they
    /// have drawn: their `draft` (their table so far) if it makes a valid
    /// turn with some discard, else the table as it was; the discard as
    /// `safe_discard` picks it from what's left.
    pub fn timeout_turn(&self, player: usize, draft: Option<&[Vec<Tile>]>) -> Turn {
        let fallback = || {
            let discard = safe_discard(self.okey, &self.table, &self.racks[player]);
            Turn { take_discard: false, table: self.table.clone(), discard }
        };
        let Some(draft) = draft else { return fallback() };
        let played = Counts::of(draft.iter().flatten()).minus(&Counts::of(self.table.iter().flatten()));
        let left = played.and_then(|played| Counts::of(&self.racks[player]).minus(&played));
        let Some(left) = left.filter(|left| !left.is_empty()) else { return fallback() };
        let turn = Turn { take_discard: false, table: draft.to_vec(), discard: safe_discard(self.okey, draft, &left.tiles()) };
        if self.check_turn(player, &turn).is_ok() {
            turn
        } else {
            fallback()
        }
    }

    /// What an opened player pays for their rack: its numbers, `times`
    /// and doubled after opening with pairs, and 101 for each okey, never
    /// doubled (okey.gg).
    fn leftovers(&self, player: usize, times: i32) -> i32 {
        let rack = &self.racks[player];
        let okeys = rack.iter().filter(|&&t| t == self.okey).count() as i32;
        let numbers: i32 = rack.iter().filter(|&&t| t != self.okey).map(|&t| self.face(t)).sum();
        let pairs = if self.opened[player] == Opened::Pairs { 2 } else { 1 };
        numbers * times * pairs + okeys * PENALTY
    }

    /// `winner` went out discarding `last`. The winner gets -101, each
    /// player who never opened 202, and those who opened their
    /// `leftovers`, all doubled when the last discard is the okey and
    /// doubled again when the winner opened with pairs or went out on the
    /// turn they opened, before anyone.
    fn finish(&mut self, winner: usize, last: Tile, direct: bool) {
        let okey_last = if last == self.okey { 2 } else { 1 };
        let special = if direct || self.opened[winner] == Opened::Pairs { 2 } else { 1 };
        let times = okey_last * special;
        let scores = (0..self.players())
            .map(|p| {
                let score = match self.opened[p] {
                    _ if p == winner => -PENALTY * times,
                    Opened::No => 2 * PENALTY * times,
                    _ => self.leftovers(p, times),
                };
                score + self.written[p]
            })
            .collect();
        self.end = Some(HandEnd { winner: Some(winner), void: false, scores });
    }

    /// The pool ran out. If anyone opened, every rack is scored as if
    /// someone had gone out, without a winner; if nobody did, only what
    /// was written counts.
    fn finish_without_winner(&mut self) {
        let anyone_opened = self.opened.iter().any(|&o| o != Opened::No);
        let scores = (0..self.players())
            .map(|p| {
                let score = match self.opened[p] {
                    _ if !anyone_opened => 0,
                    Opened::No => 2 * PENALTY,
                    _ => self.leftovers(p, 1),
                };
                score + self.written[p]
            })
            .collect();
        self.end = Some(HandEnd { winner: None, void: false, scores });
    }
}

/// What makes a set valid in Okey 101.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "lowercase")]
pub enum MeldKind {
    Run,
    Group,
    /// Two of the same tile (or one and an okey).
    Pair,
}

/// What a player's turn so far adds up to towards opening: the points of
/// the runs and groups laid down and added to the table, and the pairs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub points: u32,
    pub pairs: usize,
}

/// What `tiles` make, with `okey` wild: `None` if not a valid set.
pub fn meld_kind(okey: Tile, tiles: &[Tile]) -> Option<MeldKind> {
    kind(okey, tiles)
}

/// What the table `after` adds up to towards opening, from the view's
/// table: new sets after the table's own, and tiles added to its sets.
pub fn progress(view: &HandView, after: &[Vec<Tile>]) -> Progress {
    let okey = view.okey;
    let before = &view.table;
    let new = after.get(before.len()..).unwrap_or_default();
    let added: u32 = before
        .iter()
        .zip(after)
        .map(|(old, new)| set_points(okey, new).saturating_sub(set_points(okey, old)))
        .sum();
    let points = new.iter().filter(|s| kind(okey, s) != Some(MeldKind::Pair)).map(|s| set_points(okey, s)).sum::<u32>();
    let pairs = new.iter().filter(|s| kind(okey, s) == Some(MeldKind::Pair)).count();
    Progress { points: points + added, pairs }
}

/// Checks `turn` from what its player sees, as `Hand::check_turn` would:
/// for instant feedback in the browser (the server checks it again).
pub fn check_view_turn(view: &HandView, turn: &Turn) -> Result<(), OkeyError> {
    let Some(you) = view.you else { return Err(OkeyError::NotYourTurn) };
    let players = view.rack_sizes.len();
    // Others' racks and the pool don't matter to a check; who laid which
    // tile only to penalties.
    let racks = (0..players).map(|p| if p == you { view.rack.clone() } else { Vec::new() }).collect();
    let mut hand = Hand::from_deal(racks, Vec::new(), view.indicator, view.current, view.rules.clone());
    hand.owners = view.table.iter().map(|set| vec![you; set.len()]).collect();
    hand.table = view.table.clone();
    hand.discards = view.discards.clone();
    hand.opened = view.opened.clone();
    hand.opening_points = view.opening_points;
    hand.opening_pairs = view.opening_pairs;
    hand.phase = view.phase;
    hand.end = view.end.clone();
    hand.check_turn(you, turn)
}

/// How a table's set was added to: tiles before it (`start`), and which
/// of its places held an okey now swapped for a real tile.
struct Extension {
    start: usize,
    swapped: Vec<usize>,
}

/// The tile one higher than the indicator in its colour, 1 after 13.
pub(crate) fn okey_for(indicator: Tile) -> Tile {
    match indicator {
        Tile::Number { color, value } => Tile::Number { color, value: value % MAX_VALUE + 1 },
        Tile::Joker => panic!("a joker can't be the indicator"),
    }
}

/// The tiles as classic sets read them: the okey wild (a joker), a false
/// okey the okey's own number.
pub(crate) fn plain(okey: Tile, tiles: &[Tile]) -> Vec<Tile> {
    tiles
        .iter()
        .map(|&t| match t {
            t if t == okey => Tile::Joker,
            Tile::Joker => okey,
            t => t,
        })
        .collect()
}

fn kind(okey: Tile, set: &[Tile]) -> Option<MeldKind> {
    let plain = plain(okey, set);
    if let [a, b] = plain[..] {
        return (a == b || a == Tile::Joker || b == Tile::Joker).then_some(MeldKind::Pair);
    }
    classify(&plain).map(|kind| match kind {
        SetKind::Run => MeldKind::Run,
        SetKind::Group => MeldKind::Group,
    })
}

/// What a valid run or group counts towards an opening, the okey as the
/// tile it stands for; 0 for anything else.
fn set_points(okey: Tile, set: &[Tile]) -> u32 {
    points(&plain(okey, set)).unwrap_or(0)
}

/// Whether `tile` could be added to an end of a run or group on `table`.
/// The okey fits anywhere and has a penalty of its own.
fn fits(okey: Tile, table: &[Vec<Tile>], tile: Tile) -> bool {
    tile != okey
        && table.iter().filter(|set| set.len() > 2).any(|set| {
            [0, set.len()].into_iter().any(|at| {
                let mut longer = set.clone();
                longer.insert(at, tile);
                matches!(kind(okey, &longer), Some(MeldKind::Run | MeldKind::Group))
            })
        })
}

/// A tile's number; a false okey's is the okey's.
fn face(okey: Tile, tile: Tile) -> i32 {
    match (tile, okey) {
        (Tile::Number { value, .. }, _) | (Tile::Joker, Tile::Number { value, .. }) => value as i32,
        (Tile::Joker, Tile::Joker) => unreachable!("the okey is a number"),
    }
}

/// The tile to discard from `rack` (not empty) that costs nothing: not
/// the okey, nor a tile that fits `table` (the table after the turn), the
/// highest of those, as it would cost most left on the rack. If every
/// tile costs a penalty, any but the okey.
pub fn safe_discard(okey: Tile, table: &[Vec<Tile>], rack: &[Tile]) -> Tile {
    let free = |&&t: &&Tile| t != okey && !fits(okey, table, t);
    let highest = |tiles: Vec<&Tile>| tiles.into_iter().max_by_key(|&&t| face(okey, t)).copied();
    highest(rack.iter().filter(free).collect())
        .or_else(|| highest(rack.iter().filter(|&&t| t != okey).collect()))
        .unwrap_or(rack[0])
}

/// `rack` without the tiles in `played` and one `discard`.
fn remove(rack: &[Tile], played: &Counts, discard: Tile) -> Vec<Tile> {
    let mut rack = rack.to_vec();
    for tile in played.tiles().into_iter().chain([discard]) {
        let i = rack.iter().position(|&t| t == tile).expect("checked against the rack");
        rack.remove(i);
    }
    rack
}

/// A match: `rules.hands` hands, lowest total wins. The start passes to
/// the next seat each hand; a void hand is dealt again by the same
/// starter and doesn't count.
#[derive(Clone, Debug)]
pub struct Match {
    rules: Okey101Rules,
    seed: u64,
    /// Hands dealt, void ones included: seeds each deal.
    dealt: u64,
    /// Hands finished that count.
    played: u32,
    starter: usize,
    totals: Vec<i32>,
    hand: Hand,
}

/// What one player may know about a match.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(rename_all = "camelCase")]
pub struct MatchView {
    pub hand: HandView,
    /// Hands finished.
    pub played: u32,
    /// Points so far, hands finished only.
    pub totals: Vec<i32>,
    /// Lowest totals, once the match is over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    pub winners: Option<Vec<usize>>,
}

impl Match {
    /// Panics unless there are 2 to 4 players and at least one hand.
    pub fn new(players: usize, rules: Okey101Rules, seed: u64) -> Match {
        assert!(rules.hands > 0, "a match needs a hand");
        let hand = Hand::new(players, rules.clone(), deal_seed(seed, 0), 0);
        Match { rules, seed, dealt: 1, played: 0, starter: 0, totals: vec![0; players], hand }
    }

    pub fn hand(&self) -> &Hand {
        &self.hand
    }

    pub fn draw(&mut self, player: usize) -> Result<Option<Tile>, OkeyError> {
        let tile = self.hand.draw(player)?;
        self.tally();
        Ok(tile)
    }

    pub fn play(&mut self, player: usize, turn: Turn) -> Result<(), OkeyError> {
        self.hand.play(player, turn)?;
        self.tally();
        Ok(())
    }

    /// Hands finished.
    pub fn played(&self) -> u32 {
        self.played
    }

    pub fn totals(&self) -> &[i32] {
        &self.totals
    }

    pub fn is_over(&self) -> bool {
        self.played >= self.rules.hands
    }

    /// The players with the lowest total, once the match is over.
    pub fn winners(&self) -> Option<Vec<usize>> {
        let lowest = *self.totals.iter().min()?;
        self.is_over().then(|| (0..self.totals.len()).filter(|&p| self.totals[p] == lowest).collect())
    }

    /// Deals the next hand once this one has ended and the match goes on
    /// (the room shows the scores in between). Returns whether it dealt.
    pub fn next_hand(&mut self) -> bool {
        let Some(end) = self.hand.end() else { return false };
        if self.is_over() {
            return false;
        }
        if !end.void {
            self.starter = (self.starter + 1) % self.totals.len();
        }
        let seed = deal_seed(self.seed, self.dealt);
        self.hand = Hand::new(self.totals.len(), self.rules.clone(), seed, self.starter);
        self.dealt += 1;
        true
    }

    pub fn view(&self, player: usize) -> MatchView {
        MatchView { hand: self.hand.view(player), ..self.watcher_view() }
    }

    pub fn watcher_view(&self) -> MatchView {
        MatchView {
            hand: self.hand.watcher_view(),
            played: self.played,
            totals: self.totals.clone(),
            winners: self.winners(),
        }
    }

    /// Adds a hand's scores to the totals the moment it ends.
    fn tally(&mut self) {
        if let Some(end) = self.hand.end().filter(|end| !end.void) {
            for (total, score) in self.totals.iter_mut().zip(&end.scores) {
                *total += score;
            }
            self.played += 1;
        }
    }
}

/// Each hand of a match its own seed, from the match's.
fn deal_seed(seed: u64, hand: u64) -> u64 {
    Rng::new(seed ^ hand.wrapping_mul(0xA24B_AED4_963E_E407)).next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::Color::{self, *};

    fn n(color: Color, value: u8) -> Tile {
        Tile::Number { color, value }
    }
    const F: Tile = Tile::Joker;

    /// A hand with the indicator red 4, so red 5 is the okey.
    fn hand(racks: Vec<Vec<Tile>>, pool: Vec<Tile>) -> Hand {
        Hand::from_deal(racks, pool, n(Red, 4), 0, Okey101Rules::default())
    }
    const OKEY: Tile = Tile::Number { color: Red, value: 5 };

    /// Puts `table` down as laid by `owner`, partway through a hand.
    fn lay(h: &mut Hand, table: Vec<Vec<Tile>>, owner: usize) {
        h.owners = table.iter().map(|set| vec![owner; set.len()]).collect();
        h.table = table;
    }

    fn turn(table: Vec<Vec<Tile>>, discard: Tile) -> Turn {
        Turn { take_discard: false, table, discard }
    }

    /// Runs worth 108: 11-12-13 in three colours.
    fn opening() -> Vec<Vec<Tile>> {
        [Black, Blue, Orange].iter().map(|&c| vec![n(c, 11), n(c, 12), n(c, 13)]).collect()
    }

    /// Every tile in the hand, wherever it is, sorted.
    fn all_tiles(hand: &Hand) -> Vec<Tile> {
        let mut tiles: Vec<Tile> = hand
            .pool
            .iter()
            .chain(hand.racks.iter().flatten())
            .chain(hand.table.iter().flatten())
            .chain(hand.discards.iter().flatten())
            .copied()
            .collect();
        tiles.sort_by_key(|t| format!("{t:?}"));
        tiles
    }

    /// Every tile of the set is in exactly one place (a real deal only).
    fn assert_all_tiles_present(hand: &Hand) {
        let mut tiles = all_tiles(hand);
        tiles.push(hand.indicator);
        let mut all = full_set();
        let key = |t: &Tile| format!("{t:?}");
        tiles.sort_by_key(key);
        all.sort_by_key(key);
        assert_eq!(tiles, all);
        assert_eq!(hand.owners.iter().map(Vec::len).collect::<Vec<_>>(), hand.table.iter().map(Vec::len).collect::<Vec<_>>());
    }

    #[test]
    fn deal() {
        let hand = Hand::new(4, Okey101Rules::default(), 9, 2);
        let sizes: Vec<usize> = (0..4).map(|p| hand.rack(p).len()).collect();
        assert_eq!(sizes, vec![21, 21, 22, 21]);
        assert_eq!(hand.pool_size(), 106 - 85 - 1);
        assert_eq!(hand.current(), 2);
        assert_eq!(hand.phase(), Phase::Play, "the starter only discards");
        assert!(matches!(hand.indicator, Tile::Number { .. }));
        assert_all_tiles_present(&hand);
    }

    #[test]
    fn the_okey_is_one_above_the_indicator() {
        assert_eq!(okey_for(n(Black, 5)), n(Black, 6));
        assert_eq!(okey_for(n(Blue, 13)), n(Blue, 1));
    }

    #[test]
    fn sets_with_the_okey_and_false_okeys() {
        let h = hand(vec![vec![], vec![]], vec![]);
        assert_eq!(h.kind(&[n(Blue, 3), OKEY, n(Blue, 5)]), Some(MeldKind::Run), "the okey is wild");
        assert_eq!(h.kind(&[n(Red, 4), F, n(Red, 6)]), Some(MeldKind::Run), "a false okey is red 5");
        assert_eq!(h.kind(&[n(Blue, 4), F, n(Blue, 6)]), None, "and nothing else");
        assert_eq!(h.kind(&[n(Black, 9), n(Blue, 9), OKEY]), Some(MeldKind::Group));
        assert_eq!(h.kind(&[n(Black, 9), n(Black, 9)]), Some(MeldKind::Pair));
        assert_eq!(h.kind(&[n(Black, 9), OKEY]), Some(MeldKind::Pair));
        assert_eq!(h.kind(&[n(Black, 9), n(Blue, 9)]), None, "a pair is two of the same tile");
        assert_eq!(h.kind(&[n(Black, 12), n(Black, 13), n(Black, 1)]), None, "1 only below 2");
        assert_eq!(h.points(&[n(Blue, 3), OKEY, n(Blue, 5)]), 12);
    }

    #[test]
    fn opening_needs_101() {
        let mut rack: Vec<Tile> = opening().concat();
        rack.extend([n(Red, 1), n(Red, 2), n(Red, 3), n(Black, 2)]);
        let mut h = hand(vec![rack, vec![n(Blue, 1)]], vec![n(Orange, 1)]);
        let low = vec![vec![n(Red, 1), n(Red, 2), n(Red, 3)]];
        assert_eq!(h.play(0, turn(low, n(Black, 2))), Err(OkeyError::OpeningTooLow { points: 6, needed: 101 }));
        h.play(0, turn(opening(), n(Black, 2))).unwrap();
        assert_eq!(h.opened(0), Opened::Sets);
        assert_eq!(h.rack(0).len(), 3);
        assert_eq!((h.current(), h.phase()), (1, Phase::Draw));
        assert_eq!(h.play(1, turn(opening(), n(Blue, 1))), Err(OkeyError::DrawFirst));
    }

    #[test]
    fn each_opening_must_beat_the_last() {
        let mut rack: Vec<Tile> = opening().concat();
        rack.push(n(Red, 1));
        let mut fixed = hand(vec![rack.clone(), vec![]], vec![]);
        fixed.play(0, turn(opening(), n(Red, 1))).unwrap();
        assert_eq!(fixed.opening_needs(), (101, 5), "not progressive by default");

        let mut h = hand(vec![rack, vec![]], vec![]);
        h.rules.progressive = true;
        h.play(0, turn(opening(), n(Red, 1))).unwrap();
        assert_eq!(h.opening_needs(), (109, 5));
    }

    #[test]
    fn a_big_opening_earns_siler() {
        // 11-12-13 in four colours and a run of black 8-9-10: 171.
        let mut sets = opening();
        sets.push(vec![n(Red, 11), n(Red, 12), n(Red, 13)]);
        sets.push(vec![n(Black, 8), n(Black, 9), n(Black, 10)]);
        let mut rack = sets.concat();
        rack.extend([n(Blue, 1), n(Blue, 3)]);
        let mut h = hand(vec![rack, vec![]], vec![]);
        h.play(0, turn(sets, n(Blue, 1))).unwrap();
        assert_eq!(h.written, vec![-101, 0]);
    }

    #[test]
    fn opening_with_pairs_then_only_pairs() {
        let pairs: Vec<Vec<Tile>> = (1..=5).map(|v| vec![n(Blue, v), n(Blue, v)]).collect();
        let mut rack = pairs.concat();
        rack.extend([n(Black, 7), n(Black, 8), n(Black, 9), n(Orange, 2), n(Orange, 2), n(Black, 1)]);
        let mut h = hand(vec![rack, vec![n(Blue, 13)]], vec![n(Orange, 1), n(Orange, 3)]);
        let four = pairs[..4].to_vec();
        assert_eq!(h.play(0, turn(four, n(Black, 1))), Err(OkeyError::TooFewPairs { pairs: 4, needed: 5 }));
        let mut mixed = pairs.clone();
        mixed.push(vec![n(Black, 7), n(Black, 8), n(Black, 9)]);
        assert_eq!(h.play(0, turn(mixed, n(Black, 1))), Err(OkeyError::SetsAndPairs));
        h.play(0, turn(pairs.clone(), n(Black, 1))).unwrap();
        assert_eq!(h.opened(0), Opened::Pairs);

        h.draw(1).unwrap();
        h.play(1, turn(pairs.clone(), n(Blue, 13))).unwrap();
        h.draw(0).unwrap();
        let mut run = pairs.clone();
        run.push(vec![n(Black, 7), n(Black, 8), n(Black, 9)]);
        assert_eq!(h.play(0, turn(run, n(Orange, 1))), Err(OkeyError::SetsAndPairs), "no runs after pairs");
        let mut more = pairs.clone();
        more.push(vec![n(Orange, 2), n(Orange, 2)]);
        h.play(0, turn(more, n(Orange, 1))).unwrap();
    }

    #[test]
    fn the_tables_sets_are_only_added_to_at_their_ends() {
        let mut rack: Vec<Tile> = opening().concat();
        rack.extend([n(Black, 10), n(Blue, 10), n(Orange, 1)]);
        let mut h = hand(vec![rack, vec![n(Black, 10)]], vec![n(Red, 9), n(Red, 8)]);
        h.play(0, turn(opening(), n(Orange, 1))).unwrap();
        h.draw(1).unwrap();
        // Player 1 hasn't opened, so can't add to the table.
        let mut added = opening();
        added[0].insert(0, n(Black, 10));
        assert_eq!(h.play(1, turn(added.clone(), n(Red, 8))), Err(OkeyError::NotOpened));
        h.play(1, turn(opening(), n(Red, 8))).unwrap();
        h.draw(0).unwrap();
        // Rearranging isn't allowed: the black run split in two.
        let mut split = opening();
        split[0] = vec![n(Black, 10), n(Black, 11), n(Black, 12)];
        split.push(vec![n(Black, 13)]);
        assert_eq!(h.play(0, turn(split, n(Blue, 10))), Err(OkeyError::SetChanged { index: 0 }));
        h.play(0, turn(added, n(Blue, 10))).unwrap();
        assert_eq!(h.table()[0].len(), 4);
        assert_eq!(h.written, vec![101, 0], "blue 10 fitted the blue run: a penalty");
    }

    #[test]
    fn at_most_two_tiles_on_an_end_of_a_run() {
        let rack = vec![n(Blue, 4), n(Blue, 5), n(Blue, 6), n(Blue, 10), n(Black, 1)];
        let mut h = hand(vec![rack, vec![]], vec![]);
        h.opened[0] = Opened::Sets;
        lay(&mut h, vec![vec![n(Blue, 7), n(Blue, 8), n(Blue, 9)]], 1);
        let three: Vec<Tile> = (4..=10).map(|v| n(Blue, v)).collect();
        assert_eq!(h.play(0, turn(vec![three], n(Black, 1))), Err(OkeyError::SetChanged { index: 0 }));
        let two: Vec<Tile> = (5..=10).map(|v| n(Blue, v)).collect();
        h.play(0, turn(vec![two], n(Blue, 4))).unwrap();
    }

    #[test]
    fn taking_the_discard_means_using_it() {
        let rack0 = vec![n(Black, 1), n(Black, 2)];
        let mut rack1: Vec<Tile> = opening().concat();
        rack1[0] = n(Black, 10); // holds 10, 12, 13: needs the 11
        rack1.extend([n(Red, 1), n(Red, 2)]);
        let mut h = hand(vec![rack0, rack1], vec![n(Orange, 9)]);
        assert_eq!(h.play(0, turn(vec![], n(Black, 11))), Err(OkeyError::TileNotOnRack));
        h.racks[0].push(n(Black, 11));
        let tiles = all_tiles(&h);
        h.play(0, turn(vec![], n(Black, 11))).unwrap();
        assert_eq!(h.discard_on_offer(), Some(n(Black, 11)));
        let mut sets = opening();
        sets[0] = vec![n(Black, 10), n(Black, 11), n(Black, 12), n(Black, 13)];
        let take = |table: Vec<Vec<Tile>>| Turn { take_discard: true, table, discard: n(Red, 1) };
        assert_eq!(h.play(1, take(vec![])), Err(OkeyError::DiscardNotUsed));
        h.play(1, take(sets)).unwrap();
        assert_eq!(h.discards[0], vec![], "taken");
        assert_eq!(h.rack(1), &[n(Red, 2)]);
        assert_eq!(h.written, vec![110, 0], "opened with player 0's 11: 11 x 10");
        assert_eq!(all_tiles(&h), tiles);
    }

    #[test]
    fn a_discarded_okey_cant_be_taken() {
        let mut h = hand(vec![vec![OKEY, n(Red, 1)], vec![n(Red, 3), n(Red, 4)]], vec![n(Blue, 3)]);
        h.play(0, turn(vec![], OKEY)).unwrap();
        assert_eq!(h.written, vec![101, 0], "discarding the okey");
        assert_eq!(h.discard_on_offer(), None);
        let take = Turn { take_discard: true, table: vec![vec![n(Red, 3), n(Red, 4), OKEY]], discard: n(Red, 3) };
        assert_eq!(h.play(1, take), Err(OkeyError::OkeyDiscard));
    }

    #[test]
    fn an_okey_on_the_table_can_be_taken_by_someone_opened() {
        let mut h = hand(vec![vec![], vec![n(Blue, 8), n(Black, 4), n(Orange, 4), n(Red, 1)]], vec![]);
        h.current = 1;
        lay(&mut h, vec![vec![n(Blue, 7), OKEY, n(Blue, 9)]], 0);
        let taken = vec![vec![n(Blue, 7), n(Blue, 8), n(Blue, 9)], vec![n(Black, 4), n(Orange, 4), OKEY]];
        assert_eq!(h.play(1, turn(taken.clone(), n(Red, 1))), Err(OkeyError::OkeyNotFree { index: 0 }), "not opened");
        h.opened = vec![Opened::Sets, Opened::Sets];
        let kept = vec![vec![n(Blue, 7), n(Blue, 8), n(Blue, 9)]];
        assert_eq!(h.play(1, turn(kept, n(Red, 1))), Err(OkeyError::TileTakenFromTable), "the okey must go back down");
        h.play(1, turn(taken, n(Red, 1))).unwrap();
        assert_eq!(h.written, vec![101, 0], "player 0's okey was taken");
        assert_eq!(h.owners, vec![vec![0, 1, 0], vec![1, 1, 1]]);
    }

    #[test]
    fn an_okey_leaves_a_group_only_when_all_four_colours_are_there() {
        let rack = vec![n(Orange, 7), n(Black, 7), n(Black, 3), n(Orange, 3), n(Red, 1)];
        let mut h = hand(vec![rack, vec![]], vec![]);
        h.opened[0] = Opened::Sets;
        lay(&mut h, vec![vec![n(Red, 7), n(Blue, 7), OKEY]], 0);
        let early = vec![vec![n(Red, 7), n(Blue, 7), n(Black, 7)], vec![n(Black, 3), n(Orange, 3), OKEY]];
        assert_eq!(h.play(0, turn(early, n(Red, 1))), Err(OkeyError::OkeyNotFree { index: 0 }));
        let four =
            vec![vec![n(Red, 7), n(Blue, 7), n(Orange, 7), n(Black, 7)], vec![n(Black, 3), n(Orange, 3), OKEY]];
        h.play(0, turn(four, n(Red, 1))).unwrap();
        assert_eq!(h.written, vec![0, 0], "taking your own okey costs nothing");
    }

    #[test]
    fn penalties_can_be_switched_off() {
        let mut h = hand(vec![vec![OKEY, n(Red, 1)], vec![n(Blue, 2)]], vec![n(Blue, 3)]);
        h.rules.penalties = false;
        h.play(0, turn(vec![], OKEY)).unwrap();
        assert_eq!(h.written, vec![0, 0]);
    }

    #[test]
    fn going_out_scores() {
        // Player 0 opened earlier and now goes out; 1 opened with sets,
        // 2 with pairs, 3 never.
        let mut h = Hand::from_deal(
            vec![
                vec![n(Black, 7), n(Black, 8), n(Black, 9), n(Red, 2)],
                vec![n(Blue, 10), OKEY],
                vec![n(Blue, 3), F, OKEY],
                vec![n(Orange, 1)],
            ],
            vec![],
            n(Red, 4),
            0,
            Okey101Rules::default(),
        );
        h.opened = vec![Opened::Sets, Opened::Sets, Opened::Pairs, Opened::No];
        h.written[3] = 101;
        h.play(0, turn(vec![vec![n(Black, 7), n(Black, 8), n(Black, 9)]], n(Red, 2))).unwrap();
        let end = h.end().unwrap();
        assert_eq!(end.winner, Some(0));
        // 1: 10 + okey 101; 2: (3 + false okey 5) x 2 + okey 101; 3: 202 + 101.
        assert_eq!(end.scores, vec![-101, 111, 117, 303]);
        assert_eq!(h.draw(1), Err(OkeyError::HandOver));
    }

    #[test]
    fn going_out_on_the_okey_doubles_the_winner_and_who_never_opened() {
        let rack = vec![n(Black, 7), n(Black, 8), n(Black, 9), OKEY];
        let mut h = hand(vec![rack, vec![n(Blue, 6)], vec![n(Blue, 6)]], vec![]);
        h.opened = vec![Opened::Sets, Opened::Sets, Opened::No];
        h.play(0, turn(vec![vec![n(Black, 7), n(Black, 8), n(Black, 9)]], OKEY)).unwrap();
        assert_eq!(h.end().unwrap().scores, vec![-202, 12, 404], "no penalty for the okey going out");
    }

    #[test]
    fn going_out_with_pairs_on_the_okey() {
        let mut h = hand(vec![vec![n(Black, 7), n(Black, 7), OKEY], vec![n(Blue, 6)], vec![n(Blue, 6)]], vec![]);
        h.opened = vec![Opened::Pairs, Opened::Pairs, Opened::No];
        h.play(0, turn(vec![vec![n(Black, 7), n(Black, 7)]], OKEY)).unwrap();
        // Player 1's 6: doubled for their pairs, then twice more.
        assert_eq!(h.end().unwrap().scores, vec![-404, 48, 808]);
    }

    #[test]
    fn a_direct_finish() {
        // Seven runs worth 207, and one tile to discard: Siler too.
        let sets: Vec<Vec<Tile>> =
            [(Black, 11), (Blue, 11), (Orange, 11), (Black, 8), (Blue, 8), (Orange, 8), (Black, 5)]
                .iter()
                .map(|&(c, v)| vec![n(c, v), n(c, v + 1), n(c, v + 2)])
                .collect();
        let mut rack = sets.concat();
        rack.push(n(Red, 13));
        let mut h = hand(vec![rack, vec![n(Blue, 13)], vec![n(Orange, 13)]], vec![]);
        h.play(0, turn(sets, n(Red, 13))).unwrap();
        assert_eq!(h.end().unwrap().scores, vec![-202 - 101, 404, 404]);
    }

    #[test]
    fn every_turn_ends_with_a_discard() {
        let mut h = hand(vec![vec![n(Black, 7), n(Black, 8), n(Black, 9)], vec![]], vec![]);
        h.opened[0] = Opened::Sets;
        let all = vec![vec![n(Black, 7), n(Black, 8), n(Black, 9)]];
        assert_eq!(h.play(0, turn(all, n(Black, 9))), Err(OkeyError::TileNotOnRack));
    }

    #[test]
    fn an_empty_pool_ends_the_hand() {
        // Nobody opened: nothing but what was written.
        let mut h = hand(vec![vec![n(Black, 2), n(Black, 3)], vec![OKEY, n(Blue, 3)]], vec![]);
        h.play(0, turn(vec![], n(Black, 2))).unwrap();
        assert_eq!(h.draw(1), Ok(None));
        assert_eq!(h.end().unwrap(), &HandEnd { winner: None, void: false, scores: vec![0, 0] });

        // Someone opened: every rack counts.
        let mut h = hand(vec![vec![n(Black, 2), n(Black, 3)], vec![OKEY, n(Blue, 3)], vec![n(Blue, 9)]], vec![]);
        h.opened[1] = Opened::Sets;
        h.play(0, turn(vec![], n(Black, 2))).unwrap();
        assert_eq!(h.draw(1), Ok(None));
        assert_eq!(h.end().unwrap().scores, vec![202, 104, 202]);
    }

    #[test]
    fn everyone_on_pairs_voids_the_hand() {
        let pairs: Vec<Vec<Tile>> = (1..=5).map(|v| vec![n(Orange, v), n(Orange, v)]).collect();
        let mut rack = pairs.concat();
        rack.extend([n(Black, 1), n(Black, 9)]);
        let mut h = hand(vec![rack, vec![n(Blue, 3)]], vec![n(Red, 9)]);
        h.opened[1] = Opened::Pairs;
        h.written[1] = 101;
        h.play(0, turn(pairs, n(Black, 1))).unwrap();
        assert_eq!(h.end().unwrap(), &HandEnd { winner: None, void: true, scores: vec![0, 0] });
    }

    #[test]
    fn a_view_hides_other_racks() {
        let hand = Hand::new(3, Okey101Rules::default(), 4, 0);
        let view = hand.view(1);
        assert_eq!(view.rack, hand.rack(1));
        assert_eq!(view.rack_sizes, vec![22, 21, 21]);
        let json = serde_json::to_string(&view).unwrap();
        assert!(json.contains("\"phase\":\"play\""), "{json}");
        assert!(json.contains("\"openingPoints\":101"), "{json}");
        assert!(!json.contains("end"), "{json}");
    }

    /// A naive player: draws (never takes the discard), opens with runs
    /// and groups of three when they're worth enough, then lays down any
    /// it finds and adds a tile to the end of a table set; discards the
    /// first tile it holds that isn't the okey.
    fn naive_turn(m: &mut Match) {
        let h = m.hand();
        let player = h.current();
        if h.phase() == Phase::Draw {
            m.draw(player).unwrap();
            if m.hand().end().is_some() {
                return;
            }
        }
        let h = m.hand();
        let mut rack = h.rack(player).to_vec();
        let mut sets = Vec::new();
        'search: loop {
            for i in 0..rack.len() {
                for j in 0..rack.len() {
                    for k in 0..rack.len() {
                        if i == j || j == k || i == k {
                            continue;
                        }
                        let set = vec![rack[i], rack[j], rack[k]];
                        if matches!(h.kind(&set), Some(MeldKind::Run | MeldKind::Group)) && rack.len() > 3 {
                            let mut picked = [i, j, k];
                            picked.sort_unstable();
                            for p in picked.into_iter().rev() {
                                rack.remove(p);
                            }
                            sets.push(set);
                            continue 'search;
                        }
                    }
                }
            }
            break;
        }
        let worth: u32 = sets.iter().map(|s| h.points(s)).sum();
        let opened = h.opened(player) != Opened::No || worth >= h.opening_needs().0;
        if !opened {
            sets.clear();
            rack = h.rack(player).to_vec();
        }
        let mut table = h.table().to_vec();
        table.extend(sets);
        if opened {
            // One tile at most on each set already down.
            for set in table.iter_mut().take(h.table().len()) {
                let fit = rack.iter().position(|&tile| {
                    set.len() > 2 && matches!(h.kind(&[set.as_slice(), &[tile]].concat()), Some(MeldKind::Run | MeldKind::Group))
                });
                if let (Some(i), true) = (fit, rack.len() > 1) {
                    set.push(rack.remove(i));
                }
            }
        }
        let discard = rack.iter().copied().find(|&t| t != h.okey()).unwrap_or(rack[0]);
        m.play(player, turn(table, discard)).expect("the naive player only makes valid turns");
    }

    #[test]
    fn whole_matches_end_and_keep_every_tile() {
        let mut went_out = 0;
        for seed in 0..12 {
            let players = 2 + seed as usize % 3;
            let mut m = Match::new(players, Okey101Rules { hands: 3, ..Okey101Rules::default() }, seed);
            loop {
                let mut turns = 0;
                while m.hand().end().is_none() {
                    naive_turn(&mut m);
                    assert_all_tiles_present(m.hand());
                    turns += 1;
                    assert!(turns < 300, "seed {seed}: a hand doesn't end");
                }
                went_out += m.hand().end().unwrap().winner.is_some() as usize;
                if !m.next_hand() {
                    break;
                }
            }
            assert_eq!(m.played(), 3);
            assert!(m.winners().is_some_and(|w| !w.is_empty()));
        }
        assert!(went_out > 0, "someone goes out now and then");
    }

    #[test]
    fn a_turn_checked_from_the_view_and_its_progress() {
        let mut rack: Vec<Tile> = opening().concat();
        rack.extend([n(Red, 1), n(Black, 2)]);
        let h = hand(vec![rack, vec![n(Blue, 1)]], vec![]);
        let view = h.view(0);
        let low = vec![vec![n(Black, 11), n(Black, 12), n(Black, 13)]];
        assert_eq!(progress(&view, &low), Progress { points: 36, pairs: 0 });
        assert_eq!(
            check_view_turn(&view, &turn(low, n(Red, 1))),
            Err(OkeyError::OpeningTooLow { points: 36, needed: 101 })
        );
        assert_eq!(progress(&view, &opening()).points, 108);
        assert_eq!(check_view_turn(&view, &turn(opening(), n(Red, 1))), Ok(()));
        assert_eq!(check_view_turn(&h.view(1), &turn(vec![], n(Blue, 1))), Err(OkeyError::NotYourTurn));
        assert_eq!(meld_kind(OKEY, &[n(Blue, 2), OKEY]), Some(MeldKind::Pair));
    }

    #[test]
    fn a_safe_discard() {
        let table = vec![vec![n(Blue, 7), n(Blue, 8), n(Blue, 9)]];
        // Blue 10 fits, the okey costs 101: black 4 it is (the highest left).
        assert_eq!(safe_discard(OKEY, &table, &[OKEY, n(Blue, 10), n(Black, 4), n(Black, 2)]), n(Black, 4));
        assert_eq!(safe_discard(OKEY, &table, &[OKEY, n(Blue, 10)]), n(Blue, 10), "a penalty, but not the okey");
        assert_eq!(safe_discard(OKEY, &table, &[OKEY]), OKEY);
    }

    #[test]
    fn when_time_runs_out_a_valid_draft_is_played() {
        let mut rack: Vec<Tile> = opening().concat();
        rack.extend([n(Red, 1), n(Black, 2)]);
        let h = hand(vec![rack, vec![]], vec![]);
        let turn = h.timeout_turn(0, Some(&opening()));
        assert_eq!(turn.table, opening());
        assert_eq!(turn.discard, n(Black, 2));
        assert!(h.check_turn(0, &turn).is_ok());
        // Too little to open: the table stays as it was.
        let low = vec![vec![n(Black, 11), n(Black, 12), n(Black, 13)]];
        assert_eq!(h.timeout_turn(0, Some(&low)).table, Vec::<Vec<Tile>>::new());
        assert!(h.check_turn(0, &h.timeout_turn(0, None)).is_ok());
    }

    #[test]
    fn a_bot_with_nothing_to_lay_draws_then_discards() {
        let mut h = hand(vec![vec![n(Black, 2), n(Black, 3)], vec![OKEY, n(Blue, 3)]], vec![n(Red, 9)]);
        let OkeyMove::Turn(turn) = bot_move(&h.view(0), crate::BotLevel::Easy) else { panic!("the starter discards") };
        h.play(0, turn).unwrap();
        assert_eq!(bot_move(&h.view(1), crate::BotLevel::Easy), OkeyMove::Draw);
        h.draw(1).unwrap();
        let OkeyMove::Turn(turn) = bot_move(&h.view(1), crate::BotLevel::Easy) else { panic!("then discards") };
        assert_eq!(turn.discard, n(Red, 9));
        h.play(1, turn).unwrap();
    }

    #[test]
    fn a_void_hand_is_dealt_again() {
        let mut m = Match::new(2, Okey101Rules::default(), 3);
        let first = m.hand().rack(0).to_vec();
        m.hand.end = Some(HandEnd { winner: None, void: true, scores: vec![0, 0] });
        m.tally();
        assert_eq!(m.played(), 0);
        assert!(m.next_hand());
        assert_eq!(m.hand().current(), 0, "the same starter");
        assert_ne!(m.hand().rack(0), first, "a new deal");
    }

    #[test]
    fn the_start_moves_on_each_hand() {
        let mut m = Match::new(4, Okey101Rules::default(), 1);
        let first = m.hand().rack(0).to_vec();
        m.hand.pool.clear();
        m.hand.phase = Phase::Draw;
        m.draw(0).unwrap();
        assert_eq!(m.played(), 1, "a hand the pool ended counts");
        assert!(m.next_hand());
        assert_eq!(m.hand().current(), 1);
        assert_eq!(m.hand().rack(1).len(), 22);
        assert_ne!(m.hand().rack(0), first);
    }
}

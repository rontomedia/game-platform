//! Tile rummy's part of the messages between the client and the match
//! server (crates/protocol has the shared part): the room's rule set, the
//! moves a player sends, and the states and rejections they get. Inside
//! the shared messages as `game`.

use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
use tsify::Tsify;

use crate::okey101::{MatchView, Okey101Rules, OkeyError};
use crate::plain_okey::{PlainRules, PlainView};
use crate::{BotLevel, PlayerView, Tile, TurnError};

/// Bumped when tile rummy's messages change in a way an older client can't
/// read (`Welcome { game }`).
pub const RUMMY_PROTOCOL: u32 = 1;

/// The match lengths an Okey 101 room may choose, in hands.
pub const OKEY_HANDS: [u32; 6] = [1, 3, 5, 7, 9, 11];

/// The points a plain Okey room may start each player with.
pub const OKEY_POINTS: [u32; 4] = [4, 10, 20, 30];

/// A tile rummy room's game: its rule set and the options that go with it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(default, rename_all = "camelCase")]
pub struct RummyRules {
    /// The game played: the classic one, or Okey 101 or plain Okey with
    /// their settings.
    pub rules: RuleSet,
    /// Extra time on a turn for tiles put down: given as the table becomes
    /// a valid turn worth more than before (`Clock`). Classic only.
    pub tile_bonus: bool,
    /// Okey: each false okey shows the tile it stands for (the okey's), a
    /// help for players new to the game. Off by default, as the tiles are
    /// plain at a real table; the classic game has no false okeys.
    pub false_okey_marks: bool,
}

/// The rule sets a room can play.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuleSet {
    /// Rearranging a shared table, 30 to open (`Game`).
    #[default]
    Classic,
    /// Draw, meld, discard; 101 to open (`okey101`).
    Okey101(Okey101Rules),
    /// Draw, discard, no table: 14 tiles in sets to win (`plain_okey`).
    Okey(PlainRules),
}

impl RummyRules {
    /// The rules with anything out of range back at its default. Extra time
    /// for tiles is the classic game's only, marked false okeys Okey's.
    pub fn checked(self) -> RummyRules {
        let (rules, tile_bonus) = match self.rules {
            RuleSet::Classic => (RuleSet::Classic, self.tile_bonus),
            RuleSet::Okey101(rules) => {
                let hands = if OKEY_HANDS.contains(&rules.hands) { rules.hands } else { Okey101Rules::default().hands };
                (RuleSet::Okey101(Okey101Rules { hands, ..rules }), false)
            }
            RuleSet::Okey(rules) => {
                let points = if OKEY_POINTS.contains(&rules.points) { rules.points } else { PlainRules::default().points };
                (RuleSet::Okey(PlainRules { points }), false)
            }
        };
        let false_okey_marks = self.false_okey_marks && rules != RuleSet::Classic;
        RummyRules { rules, tile_bonus, false_okey_marks }
    }
}

/// The bot that may take over a seat: any level but Extra Hard (else
/// Medium).
pub fn stand_in(level: BotLevel) -> BotLevel {
    if level == BotLevel::ExtraHard {
        BotLevel::Medium
    } else {
        level
    }
}

/// From a player to the game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RummyRequest {
    /// End the turn with the table rearranged to this (see `check_turn`).
    Play { table: Vec<Vec<Tile>> },
    /// Classic: end the turn by drawing a tile instead. Okey: draw from
    /// the pool, and the turn goes on.
    Draw,
    /// Okey 101: end the turn with the table as it now is and this tile
    /// discarded, having taken the previous player's discard instead of
    /// drawing if `take_discard` (see `okey101::Turn`). Plain Okey: the
    /// table is left empty, and `finish` shows the other 14 tiles as a
    /// winning hand (`plain_okey::PlainTurn`).
    Discard {
        #[serde(default)]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        take_discard: bool,
        #[serde(default)]
        table: Vec<Vec<Tile>>,
        discard: Tile,
        #[serde(default)]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        finish: bool,
    },
    /// The table as the current player is rearranging it: played for them
    /// if their time runs out and it's a valid turn, and passed on for the
    /// others to watch in rooms with `liveTurns`. Not a move: sets may be
    /// invalid.
    Draft { table: Vec<Vec<Tile>> },
    /// Okey: the tiles the player has set aside to discard next, in the
    /// order to throw them, any time (none: nothing set aside). If their
    /// time runs out the first they still hold is thrown for them (in Okey
    /// 101, the first that also costs no penalty).
    DiscardNext { tiles: Vec<Tile> },
}

/// From the game to a player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RummyMessage {
    /// The game as this player may see it, after every change (a watcher:
    /// with no rack, and `view.you` absent).
    State {
        view: PlayerView,
        /// Milliseconds left in the current turn; absent once the game is
        /// over.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        turn_ms: Option<u32>,
        /// Milliseconds left in each player's time bank, in seat order (the
        /// current player's as of now): rooms with `timeBank`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        banks: Option<Vec<u32>>,
        /// Every player's rack, in seat order: only from a server started
        /// to show them (`--open-racks`), for judging the bots, and to
        /// watchers of a game only bots play.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        racks: Option<Vec<Vec<Tile>>>,
    },
    /// An Okey 101 match as this player may see it, after every change: as
    /// `State`, for rooms playing Okey 101.
    OkeyState {
        view: MatchView,
        /// Milliseconds left in the current turn; absent between hands and
        /// once the match is over.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        turn_ms: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        banks: Option<Vec<u32>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        racks: Option<Vec<Vec<Tile>>>,
        /// Between hands: milliseconds until the next is dealt.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        next_hand_ms: Option<u32>,
    },
    /// A plain Okey game as this player may see it: as `OkeyState`.
    PlainOkeyState {
        view: PlainView,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        turn_ms: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        banks: Option<Vec<u32>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        racks: Option<Vec<Vec<Tile>>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "wasm", tsify(optional))]
        next_hand_ms: Option<u32>,
    },
    /// The current player's table so far this turn (see `RummyRequest::Draft`).
    /// Gone with the next state.
    Draft { by: usize, table: Vec<Vec<Tile>> },
    /// This player's move wasn't allowed; if it was their turn, it still is.
    Rejected { error: TurnError },
    /// As `Rejected`, in an Okey 101 or plain Okey room.
    OkeyRejected { error: OkeyError },
    /// A move of another rule set than the room's (a classic play in an
    /// Okey room): a bug in the client.
    WrongRules,
    /// Something this player should know.
    Notice { notice: RummyNotice },
}

/// What the game tells a player, worded by the client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm", derive(Tsify))]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RummyNotice {
    /// The player's time ran out: of the tiles they had put down, `played`
    /// were played (the valid part of their table) and `back` went back to
    /// their rack. None played: they drew a tile.
    TimeRanOut { played: u32, back: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Game, Rules};

    #[test]
    fn requests_as_json() {
        let play = r#"{"type":"play","table":[[{"kind":"number","color":"red","value":3},{"kind":"joker"}]]}"#;
        assert_eq!(
            serde_json::from_str::<RummyRequest>(play).unwrap(),
            RummyRequest::Play { table: vec![vec![Tile::Number { color: Color::Red, value: 3 }, Tile::Joker]] }
        );
        let discard = r#"{"type":"discard","table":[],"discard":{"kind":"joker"}}"#;
        assert_eq!(
            serde_json::from_str::<RummyRequest>(discard).unwrap(),
            RummyRequest::Discard { take_discard: false, table: vec![], discard: Tile::Joker, finish: false }
        );
        assert_eq!(
            serde_json::from_str::<RummyRequest>(r#"{"type":"discardNext","tiles":[{"kind":"joker"}]}"#).unwrap(),
            RummyRequest::DiscardNext { tiles: vec![Tile::Joker] }
        );
    }

    #[test]
    fn rules_are_checked() {
        let okey = r#"{"rules":{"kind":"okey101","hands":3,"progressive":true,"penalties":false}}"#;
        let rules: RummyRules = serde_json::from_str(okey).unwrap();
        assert_eq!(rules.rules, RuleSet::Okey101(Okey101Rules { hands: 3, progressive: true, penalties: false }));
        let odd = RummyRules { tile_bonus: true, rules: RuleSet::Okey101(Okey101Rules { hands: 4, ..Default::default() }), ..Default::default() };
        assert_eq!(odd.clone().checked().rules, RuleSet::Okey101(Okey101Rules::default()), "4 hands isn't offered");
        assert!(!odd.checked().tile_bonus, "classic only");
        let marked: RummyRules = serde_json::from_str(r#"{"falseOkeyMarks":true,"rules":{"kind":"okey"}}"#).unwrap();
        assert!(marked.clone().checked().false_okey_marks);
        assert!(!RummyRules { rules: RuleSet::Classic, ..marked }.checked().false_okey_marks, "Okey only");
        assert_eq!((stand_in(BotLevel::ExtraHard), stand_in(BotLevel::Hard)), (BotLevel::Medium, BotLevel::Hard));
    }

    #[test]
    fn messages_as_json() {
        let rejected = RummyMessage::Rejected { error: TurnError::InvalidSet { index: 2 } };
        assert_eq!(serde_json::to_string(&rejected).unwrap(), r#"{"type":"rejected","error":{"kind":"invalidSet","index":2}}"#);
        let view = Game::new(2, Rules::default(), 1).view(0);
        let state = RummyMessage::State { view: view.clone(), turn_ms: Some(1500), banks: None, racks: None };
        let state = serde_json::to_string(&state).unwrap();
        assert!(state.starts_with(r#"{"type":"state","view":{"you":0,"rules":{"#), "{state}");
        assert!(state.ends_with(r#""turnMs":1500}"#), "{state}");
        let over = serde_json::to_string(&RummyMessage::State { view, turn_ms: None, banks: None, racks: None }).unwrap();
        assert!(!over.contains("turnMs") && !over.contains("null"), "{over}");
    }
}

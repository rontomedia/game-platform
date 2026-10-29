//! The game a room plays: the classic one (`tile_rummy::Game`), an Okey 101
//! match (`tile_rummy::okey101::Match`) or plain Okey (`tile_rummy::plain_okey`),
//! as a `GameEngine`. Turns, clocks, bots and watchers work the same; what a
//! move is and what a player is sent differ. Only the classic game has
//! extra time for tiles; only Okey a queue of tiles to discard next.

use std::ops::RangeInclusive;
use std::time::Duration;

use tile_rummy::okey101::{self, Match, OkeyMove, Phase, Turn};
use tile_rummy::plain_okey::{plain_bot_move, PlainGame, PlainMove, PlainTurn};
use tile_rummy::{bot_move_with, salvage, within_reach, BotLevel, Game, Lookahead, Move, Rules, Tile};
use protocol::{Notice, RuleSet, ServerMessage};
use serde::{Deserialize, Serialize};

use crate::clock::Bonus;
use crate::engine::{placings_by, ClockInfo, GameEngine, TimedOut};

/// The most tiles a player may set aside to discard next (the board's slot holds as many).
const DISCARD_NEXT: usize = 3;

#[derive(Clone)]
pub struct Contest {
    game: Kind,
    /// Extra time for tiles (classic rooms with `tile_bonus`).
    tile_bonus: bool,
    /// Okey: the tiles each player set aside to discard next, in order:
    /// the first they still hold is thrown if their time runs out. Cleared
    /// by a new hand.
    discard_next: Vec<Vec<Tile>>,
}

#[derive(Clone)]
enum Kind {
    Classic(Game),
    Okey(Match),
    Plain(PlainGame),
}

/// What a room's options say about its game (the rest is the room's).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RummyRules {
    pub rules: RuleSet,
    pub tile_bonus: bool,
    pub false_okey_marks: bool,
}

/// How the server sets up tile rummy's bots and clock.
#[derive(Clone, Copy, Debug)]
pub struct RummyConfig {
    /// How far Extra Hard bots look ahead.
    pub lookahead: Lookahead,
    /// Extra time for tiles, in rooms with `tile_bonus`.
    pub bonus: Bonus,
}

/// A move in any of them, as players send it.
pub enum RummyMove {
    Play(Vec<Vec<Tile>>),
    Draw,
    /// Okey 101 or plain Okey: the end of a turn.
    Discard(Discard),
}

/// The end of an Okey turn, as sent: Okey 101 uses the table, plain Okey
/// `finish`.
pub struct Discard {
    pub take_discard: bool,
    pub table: Vec<Vec<Tile>>,
    pub discard: Tile,
    pub finish: bool,
}

/// Tiles in a table.
fn size(table: &[Vec<Tile>]) -> usize {
    table.iter().map(Vec::len).sum()
}

/// The notice for a player whose time ran out with `put_down` tiles on the
/// table, `played` of them played; none if they hadn't put any down.
fn ran_out(put_down: usize, played: usize) -> Option<ServerMessage> {
    let notice = Notice::TimeRanOut { played: played as u32, back: put_down.saturating_sub(played) as u32 };
    (put_down > 0).then_some(ServerMessage::Notice { notice })
}

impl Contest {
    fn table(&self) -> &[Vec<Tile>] {
        match &self.game {
            Kind::Classic(game) => game.table(),
            Kind::Okey(game) => game.hand().table(),
            Kind::Plain(_) => &[],
        }
    }

    fn rack(&self, player: usize) -> &[Tile] {
        match &self.game {
            Kind::Classic(game) => game.rack(player),
            Kind::Okey(game) => game.hand().rack(player),
            Kind::Plain(game) => game.rack(player),
        }
    }
}

impl GameEngine for Contest {
    type Rules = RummyRules;
    type Move = RummyMove;
    type Level = BotLevel;
    type Config = RummyConfig;
    type Message = ServerMessage;
    type Draft = Vec<Vec<Tile>>;
    type Aside = Vec<Tile>;

    fn new(rules: &RummyRules, players: usize, seed: u64) -> Contest {
        let game = match &rules.rules {
            RuleSet::Classic => Kind::Classic(Game::new(players, Rules::default(), seed)),
            RuleSet::Okey101(rules) => Kind::Okey(Match::new(players, rules.clone(), seed)),
            RuleSet::Okey(rules) => Kind::Plain(PlainGame::new(players, rules.clone(), seed)),
        };
        Contest { game, tile_bonus: rules.tile_bonus, discard_next: vec![Vec::new(); players] }
    }

    fn seats(_rules: &RummyRules) -> RangeInclusive<usize> {
        2..=4
    }

    fn players(&self) -> usize {
        match &self.game {
            Kind::Classic(game) => game.players(),
            Kind::Okey(game) => game.hand().players(),
            Kind::Plain(game) => game.players(),
        }
    }

    fn current(&self) -> usize {
        match &self.game {
            Kind::Classic(game) => game.current(),
            Kind::Okey(game) => game.hand().current(),
            Kind::Plain(game) => game.current(),
        }
    }

    fn in_turn(&self) -> bool {
        match &self.game {
            Kind::Classic(game) => game.outcome().is_none(),
            Kind::Okey(game) => game.hand().end().is_none(),
            Kind::Plain(game) => game.end().is_none(),
        }
    }

    /// All its hands, for Okey.
    fn over(&self) -> bool {
        match &self.game {
            Kind::Classic(game) => game.outcome().is_some(),
            Kind::Okey(game) => game.is_over(),
            Kind::Plain(game) => game.is_over(),
        }
    }

    /// Drawing in Okey doesn't end the turn.
    fn ends_turn(&self, mv: &RummyMove) -> bool {
        !matches!((&self.game, mv), (Kind::Okey(_) | Kind::Plain(_), RummyMove::Draw))
    }

    fn apply(&mut self, seat: usize, mv: RummyMove) -> Result<(), ServerMessage> {
        let rejected = |error| ServerMessage::Rejected { error };
        let okey_rejected = |error| ServerMessage::OkeyRejected { error };
        match (&mut self.game, mv) {
            (Kind::Classic(game), RummyMove::Play(table)) => game.play(seat, table).map_err(rejected),
            (Kind::Classic(game), RummyMove::Draw) => game.draw(seat).map(drop).map_err(rejected),
            (Kind::Okey(game), RummyMove::Draw) => game.draw(seat).map(drop).map_err(okey_rejected),
            (Kind::Okey(game), RummyMove::Discard(Discard { take_discard, table, discard, .. })) => {
                game.play(seat, Turn { take_discard, table, discard }).map_err(okey_rejected)
            }
            (Kind::Plain(game), RummyMove::Draw) => game.draw(seat).map(drop).map_err(okey_rejected),
            (Kind::Plain(game), RummyMove::Discard(Discard { take_discard, discard, finish, .. })) => {
                game.play(seat, PlainTurn { take_discard, discard, finish }).map_err(okey_rejected)
            }
            (Kind::Classic(_), RummyMove::Discard(_)) | (Kind::Okey(_) | Kind::Plain(_), RummyMove::Play(_)) => {
                Err(ServerMessage::Error { message: "not a move of this room's game".into() })
            }
        }
    }

    /// Classic: as much of the table they left as makes a valid turn is
    /// played (see `tile_rummy::salvage`), the rest of their tiles going
    /// back to their rack; if nothing can be, they draw. Okey 101: they
    /// draw if they hadn't (which may end the hand), then their table so
    /// far is played if it's a valid turn, and a tile that costs no penalty
    /// discarded. Plain Okey: they draw if they hadn't, then finish if their
    /// hand wins, else discard the tile they set aside, else as an Easy bot
    /// would.
    fn time_out(&mut self, config: &RummyConfig, draft: Option<Vec<Vec<Tile>>>) -> TimedOut<ServerMessage> {
        let seat = self.current();
        let nothing = TimedOut { kept: Duration::ZERO, notice: None };
        if let Kind::Classic(game) = &self.game {
            let table = draft
                .as_ref()
                .and_then(|draft| salvage(game.table(), draft, game.rack(seat), game.melded(seat), game.rules()));
            // What they had put down, and what of it is played.
            let put_down = draft.as_ref().map_or(0, |d| size(d).saturating_sub(size(game.table())));
            let played = table.as_ref().map_or(0, |t| size(t) - size(game.table()));
            let kept = table.as_ref().map_or(Duration::ZERO, |table| self.worth(config, seat, table));
            let Kind::Classic(game) = &mut self.game else { unreachable!() };
            let played = if table.is_some_and(|table| game.play(seat, table).is_ok()) {
                played
            } else {
                let _ = game.draw(seat);
                0
            };
            return TimedOut { kept, notice: ran_out(put_down, played) };
        }
        match &mut self.game {
            Kind::Classic(_) => nothing,
            Kind::Okey(game) => {
                if game.hand().phase() == Phase::Draw {
                    let _ = game.draw(seat);
                    if game.hand().end().is_some() {
                        return nothing;
                    }
                }
                let turn = game.hand().timeout_turn(seat, draft.as_deref(), &self.discard_next[seat]);
                let before = size(game.hand().table());
                let put_down = draft.as_ref().map_or(0, |d| size(d).saturating_sub(before));
                let played = size(&turn.table) - before;
                if let Err(e) = game.play(seat, turn) {
                    eprintln!("a turn played when time ran out: {e}");
                }
                TimedOut { kept: Duration::ZERO, notice: ran_out(put_down, played) }
            }
            Kind::Plain(game) => {
                if game.phase() == Phase::Draw {
                    let _ = game.draw(seat);
                    if game.end().is_some() {
                        return nothing;
                    }
                }
                if let Some(turn) = tile_rummy::plain_okey::timeout_turn(&game.view(seat), &self.discard_next[seat]) {
                    if let Err(e) = game.play(seat, turn) {
                        eprintln!("a turn played when time ran out: {e}");
                    }
                }
                nothing
            }
        }
    }

    fn bot_move(&self, level: BotLevel, config: &RummyConfig, spent: &dyn Fn() -> f64) -> RummyMove {
        let seat = self.current();
        let turn = |take_discard, table, discard, finish| RummyMove::Discard(Discard { take_discard, table, discard, finish });
        match &self.game {
            Kind::Classic(game) => match bot_move_with(&game.view(seat), level, config.lookahead, spent) {
                Move::Play(table) => RummyMove::Play(table),
                Move::Draw => RummyMove::Draw,
            },
            Kind::Okey(game) => match okey101::bot_move(&game.hand().view(seat), level) {
                OkeyMove::Draw => RummyMove::Draw,
                OkeyMove::Turn(t) => turn(t.take_discard, t.table, t.discard, false),
            },
            Kind::Plain(game) => match plain_bot_move(&game.view(seat), level) {
                PlainMove::Draw => RummyMove::Draw,
                PlainMove::Turn(t) => turn(t.take_discard, Vec::new(), t.discard, t.finish),
            },
        }
    }

    /// Classic: a draw. Okey: a draw if it hadn't, then a safe discard.
    fn bot_fallback(&mut self) {
        let seat = self.current();
        match &mut self.game {
            Kind::Classic(game) => {
                let _ = game.draw(seat);
            }
            Kind::Okey(game) => {
                if game.hand().phase() == Phase::Draw {
                    let _ = game.draw(seat);
                }
                if game.hand().end().is_none() {
                    let fallback = game.hand().timeout_turn(seat, None, &[]);
                    let _ = game.play(seat, fallback);
                }
            }
            Kind::Plain(game) => {
                if game.phase() == Phase::Draw {
                    let _ = game.draw(seat);
                }
                if game.end().is_none() {
                    if let PlainMove::Turn(turn) = plain_bot_move(&game.view(seat), BotLevel::Easy) {
                        let _ = game.play(seat, turn);
                    }
                }
            }
        }
    }

    fn next_hand(&mut self) -> bool {
        for tiles in &mut self.discard_next {
            tiles.clear();
        }
        match &mut self.game {
            Kind::Okey(game) => game.next_hand(),
            Kind::Plain(game) => game.next_hand(),
            Kind::Classic(_) => false,
        }
    }

    fn state(&self, seat: Option<usize>, clock: &ClockInfo, open: bool) -> ServerMessage {
        let ClockInfo { turn_ms, banks, next_hand_ms } = clock.clone();
        let racks = open.then(|| (0..self.players()).map(|p| self.rack(p).to_vec()).collect());
        match &self.game {
            Kind::Classic(game) => {
                let view = seat.map_or_else(|| game.watcher_view(), |seat| game.view(seat));
                ServerMessage::State { view, turn_ms, banks, racks }
            }
            Kind::Okey(game) => {
                let view = seat.map_or_else(|| game.watcher_view(), |seat| game.view(seat));
                ServerMessage::OkeyState { view, turn_ms, banks, racks, next_hand_ms }
            }
            Kind::Plain(game) => {
                let view = seat.map_or_else(|| game.watcher_view(), |seat| game.view(seat));
                ServerMessage::PlainOkeyState { view, turn_ms, banks, racks, next_hand_ms }
            }
        }
    }

    /// Classic: by score; Okey 101: by penalty points, fewest first; plain
    /// Okey: by points left.
    fn placings(&self) -> Option<Vec<u32>> {
        if !self.over() {
            return None;
        }
        Some(match &self.game {
            Kind::Classic(game) => placings_by(&game.outcome()?.scores, |a, b| a > b),
            Kind::Okey(game) => placings_by(game.totals(), |a, b| a < b),
            Kind::Plain(game) => placings_by(game.points(), |a, b| a > b),
        })
    }

    /// Only tiles they could have: the table's, their rack's, and in Okey
    /// 101 the discard they may take. Plain Okey has no table to share.
    fn draft_allowed(&self, draft: &Vec<Vec<Tile>>) -> bool {
        let seat = self.current();
        match &self.game {
            Kind::Classic(game) => within_reach(game.table(), game.rack(seat), draft),
            Kind::Okey(game) => {
                let hand = game.hand();
                let mut rack = hand.rack(seat).to_vec();
                if hand.phase() == Phase::Draw {
                    rack.extend(hand.discard_on_offer());
                }
                within_reach(hand.table(), &rack, draft)
            }
            Kind::Plain(_) => draft.is_empty(),
        }
    }

    fn draft_message(&self, by: usize, draft: &Vec<Vec<Tile>>) -> Option<ServerMessage> {
        Some(ServerMessage::Draft { by, table: draft.clone() })
    }

    /// Extra time for tiles, in classic rooms with `tile_bonus`.
    fn worth(&self, config: &RummyConfig, seat: usize, table: &Vec<Vec<Tile>>) -> Duration {
        match &self.game {
            Kind::Classic(game) if self.tile_bonus => {
                config.bonus.worth(game.table(), table, game.rack(seat), game.melded(seat), game.rules())
            }
            _ => Duration::ZERO,
        }
    }

    fn move_draft(mv: &RummyMove) -> Option<&Vec<Vec<Tile>>> {
        match mv {
            RummyMove::Play(table) => Some(table),
            _ => None,
        }
    }

    /// A classic play, or an Okey 101 turn that puts tiles down.
    fn shown(&self, mv: &RummyMove) -> Option<Vec<Vec<Tile>>> {
        match (&self.game, mv) {
            (Kind::Classic(_), RummyMove::Play(table)) => Some(table.clone()),
            (Kind::Okey(_), RummyMove::Discard(turn)) if self.table() != turn.table.as_slice() => Some(turn.table.clone()),
            _ => None,
        }
    }

    /// Okey: the tiles to discard next, a few at most (`DISCARD_NEXT`).
    fn aside(&mut self, seat: usize, mut tiles: Vec<Tile>) {
        tiles.truncate(DISCARD_NEXT);
        if let Some(queue) = self.discard_next.get_mut(seat) {
            *queue = tiles;
        }
    }
}

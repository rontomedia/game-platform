//! The game a room plays: the classic one (`engine::Game`) or an Okey 101
//! match (`engine::okey101::Match`). The room runs either through this:
//! turns, clocks, bots and watchers work the same; what a move is and
//! what a player is sent differ.

use engine::okey101::{self, Match, OkeyMove, Phase};
use engine::{bot_move_with, within_reach, BotLevel, Game, Lookahead, Move, Rules, Tile};
use protocol::{RuleSet, ServerMessage};

#[derive(Clone)]
pub enum Contest {
    Classic(Game),
    Okey(Match),
}

/// A bot's move in either game.
pub enum BotChoice {
    Classic(Move),
    Okey(OkeyMove),
}

/// What a state message carries besides the view.
pub struct StateExtras {
    pub turn_ms: Option<u32>,
    pub banks: Option<Vec<u32>>,
    pub racks: Option<Vec<Vec<Tile>>>,
    pub next_hand_ms: Option<u32>,
}

impl Contest {
    pub fn new(rules: &RuleSet, players: usize, seed: u64) -> Contest {
        match rules {
            RuleSet::Classic => Contest::Classic(Game::new(players, Rules::default(), seed)),
            RuleSet::Okey101(rules) => Contest::Okey(Match::new(players, rules.clone(), seed)),
        }
    }

    pub fn players(&self) -> usize {
        match self {
            Contest::Classic(game) => game.players(),
            Contest::Okey(game) => game.hand().players(),
        }
    }

    /// Whose turn it is (or was).
    pub fn current(&self) -> usize {
        match self {
            Contest::Classic(game) => game.current(),
            Contest::Okey(game) => game.hand().current(),
        }
    }

    pub fn rack(&self, player: usize) -> &[Tile] {
        match self {
            Contest::Classic(game) => game.rack(player),
            Contest::Okey(game) => game.hand().rack(player),
        }
    }

    pub fn table(&self) -> &[Vec<Tile>] {
        match self {
            Contest::Classic(game) => game.table(),
            Contest::Okey(game) => game.hand().table(),
        }
    }

    /// The whole game is over: all its hands, for Okey 101.
    pub fn over(&self) -> bool {
        match self {
            Contest::Classic(game) => game.outcome().is_some(),
            Contest::Okey(game) => game.is_over(),
        }
    }

    /// Someone is taking a turn: not over, nor between Okey 101's hands.
    pub fn in_turn(&self) -> bool {
        match self {
            Contest::Classic(game) => game.outcome().is_none(),
            Contest::Okey(game) => game.hand().end().is_none(),
        }
    }

    /// Whether the current player's `draft` uses only tiles they could
    /// have: the table's, their rack's, and in Okey 101 the discard they
    /// may take.
    pub fn draft_within_reach(&self, draft: &[Vec<Tile>]) -> bool {
        let seat = self.current();
        match self {
            Contest::Classic(game) => within_reach(game.table(), game.rack(seat), draft),
            Contest::Okey(game) => {
                let hand = game.hand();
                let mut rack = hand.rack(seat).to_vec();
                if hand.phase() == Phase::Draw {
                    rack.extend(hand.discard_on_offer());
                }
                within_reach(hand.table(), &rack, draft)
            }
        }
    }

    /// The current player's bot move, worked out from what they see.
    pub fn bot_move(&self, level: BotLevel, lookahead: Lookahead, spent: &dyn Fn() -> f64) -> BotChoice {
        let seat = self.current();
        match self {
            Contest::Classic(game) => BotChoice::Classic(bot_move_with(&game.view(seat), level, lookahead, spent)),
            Contest::Okey(game) => BotChoice::Okey(okey101::bot_move(&game.hand().view(seat))),
        }
    }

    /// The game as `seat` may see it, or someone watching (`None`).
    pub fn state(&self, seat: Option<usize>, extras: StateExtras) -> ServerMessage {
        let StateExtras { turn_ms, banks, racks, next_hand_ms } = extras;
        match self {
            Contest::Classic(game) => {
                let view = seat.map_or_else(|| game.watcher_view(), |seat| game.view(seat));
                ServerMessage::State { view, turn_ms, banks, racks }
            }
            Contest::Okey(game) => {
                let view = seat.map_or_else(|| game.watcher_view(), |seat| game.view(seat));
                ServerMessage::OkeyState { view, turn_ms, banks, racks, next_hand_ms }
            }
        }
    }
}

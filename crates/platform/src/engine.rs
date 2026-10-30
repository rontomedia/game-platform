//! What a room needs of a game: the `GameEngine` trait. A room seats the
//! players, runs the clock and the bots, and lets others watch; the engine
//! says what a move is, applies it, and what each player is sent.
//!
//! Live turns (a player's work in progress shown to the others) and extra
//! time for what a draft would play are hooks with defaults: a game
//! without them names `()` for `Draft` and `Aside` and leaves them be.

use std::fmt::Debug;
use std::ops::RangeInclusive;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// A game's rules and state, as a room runs it. Cloned for a bot to think
/// about on a thread of its own.
pub trait GameEngine: Clone + Send + Sync + 'static {
    /// The game's part of the room's options: its rule set and house
    /// rules.
    type Rules: Clone + Debug + Default + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// What a player sends the game: a move, a draft or an aside (see
    /// `request`).
    type Request: DeserializeOwned + Send + 'static;
    /// A move, from a player or a bot.
    type Move: Send + 'static;
    /// How well a bot plays; the default is the room's stand-in unless
    /// it chooses another.
    type Level: Copy + Debug + Default + Eq + Serialize + DeserializeOwned + Send + Sync + 'static;
    /// The server's setup for this game's bots and clock.
    type Config: Copy + Send + Sync + 'static;
    /// What the room sends a player for this game: its states, its
    /// rejections, drafts and notices.
    type Message: Clone + Debug + Serialize + Send + 'static;
    /// The current player's work in progress, which they send as they go
    /// (tile rummy: the table); `()` for a game without it.
    type Draft: Clone + Send + 'static;
    /// Something a seated player may tell the room any time that isn't a
    /// move (tile rummy: the tiles to discard next); `()` if none.
    type Aside: Send + 'static;

    /// The version of the game's messages (`Welcome { game }`).
    const PROTOCOL: u32;

    /// What a player's message is for the room.
    fn request(request: Self::Request) -> Request<Self>;
    /// The rules with house rules out of range brought into it.
    fn checked(rules: Self::Rules) -> Self::Rules {
        rules
    }
    /// The bot that may take over a seat, given the one asked for.
    fn stand_in(level: Self::Level) -> Self::Level {
        level
    }

    /// A game of `players`, dealt from `seed`.
    fn new(rules: &Self::Rules, players: usize, seed: u64) -> Self;
    /// How many may play.
    fn seats(rules: &Self::Rules) -> RangeInclusive<usize>;
    fn players(&self) -> usize;
    /// Whose turn it is, or was (between hands, or once it's over).
    fn current(&self) -> usize;
    /// Someone is taking a turn: neither over nor between hands.
    fn in_turn(&self) -> bool;
    /// The whole game is over.
    fn over(&self) -> bool;

    /// Whether this move, if allowed, ends the turn (Okey's draw doesn't).
    /// Asked before it's applied: the room settles the clock first.
    fn ends_turn(&self, _mv: &Self::Move) -> bool {
        true
    }
    /// `seat` makes this move; if it isn't allowed, nothing changes and
    /// the message says why.
    fn apply(&mut self, seat: usize, mv: Self::Move) -> Result<(), Self::Message>;
    /// The current player's time ran out: something is played for them,
    /// from their `draft` if they sent one.
    fn time_out(&mut self, config: &Self::Config, draft: Option<Self::Draft>) -> TimedOut<Self::Message>;
    /// A bot's move for the current player. `spent` is the share of its
    /// time used so far (the engine has no clock).
    fn bot_move(&self, level: Self::Level, config: &Self::Config, spent: &dyn Fn() -> f64) -> Self::Move;
    /// A bot's move that would have ended its turn was refused: some
    /// allowed move instead, ending it.
    fn bot_fallback(&mut self);
    /// Between hands: deals the next. Whether it did.
    fn next_hand(&mut self) -> bool {
        false
    }

    /// The game as `seat` may see it, or someone watching (`None`).
    /// `open`: every hidden hand shown too.
    fn state(&self, seat: Option<usize>, clock: &ClockInfo, open: bool) -> Self::Message;
    /// Each seat's place once the game is over, 0 the best, ties sharing a
    /// place.
    fn placings(&self) -> Option<Vec<u32>>;

    /// Whether the current player may send this draft: it uses only what
    /// they could.
    fn draft_allowed(&self, _draft: &Self::Draft) -> bool {
        false
    }
    /// The message passing `by`'s draft on to the others.
    fn draft_message(&self, _by: usize, _draft: &Self::Draft) -> Option<Self::Message> {
        None
    }
    /// The extra time `seat` earns if this draft is played as their turn.
    fn worth(&self, _config: &Self::Config, _seat: usize, _draft: &Self::Draft) -> Duration {
        Duration::ZERO
    }
    /// The draft a move plays, if it's one (for its worth).
    fn move_draft(_mv: &Self::Move) -> Option<&Self::Draft> {
        None
    }
    /// What of a bot's move to show the others as a draft before it's made.
    fn shown(&self, _mv: &Self::Move) -> Option<Self::Draft> {
        None
    }
    /// A seated player's aside.
    fn aside(&mut self, _seat: usize, _aside: Self::Aside) {}
}

/// A player's message for the game, sorted for the room.
pub enum Request<G: GameEngine> {
    Move(G::Move),
    Draft(G::Draft),
    Aside(G::Aside),
}

/// The clock, as a state message shows it.
#[derive(Clone)]
pub struct ClockInfo {
    /// Milliseconds left in the turn; none between hands or once it's over.
    pub turn_ms: Option<u32>,
    /// Each player's time bank, in seat order: rooms with one.
    pub banks: Option<Vec<u32>>,
    /// Between hands: milliseconds until the next is dealt.
    pub next_hand_ms: Option<u32>,
}

/// What was played for a player whose time ran out.
pub struct TimedOut<M> {
    /// The extra time what was played earned them (the rest of what they
    /// were given counts against them).
    pub kept: Duration,
    /// For that player, after the new state.
    pub notice: Option<M>,
}

/// Places from scores: each seat's place is how many did better.
pub fn placings_by<T: PartialOrd>(scores: &[T], better: impl Fn(&T, &T) -> bool) -> Vec<u32> {
    scores.iter().map(|s| scores.iter().filter(|o| better(o, s)).count() as u32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_share_ties() {
        assert_eq!(placings_by(&[10, 30, 10, 5], |a, b| a > b), [1, 0, 1, 3]);
        assert_eq!(placings_by(&[10, 30, 10], |a, b| a < b), [0, 2, 0]);
    }
}

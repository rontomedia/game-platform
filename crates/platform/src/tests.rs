//! The room's own logic, with a counting game for its engine: the absent
//! player's short turn and its give-back, the time bank, and bot moves
//! that come too late. (Games' rooms are tested in crates/server.)

use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use protocol::RoomOptions;
use serde::Serialize;

use crate::engine::{ClockInfo, GameEngine, Request, TimedOut};
use crate::room::{Command, Config, Connection, Out, PlayerId, Timing};
use crate::rooms::{bot_pool, Rooms};

/// Players add 1 or 2 to a total in turn; the game is over at `Rules`.
#[derive(Clone)]
struct Count {
    players: usize,
    current: usize,
    total: u32,
    target: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
enum Said {
    State { total: u32, current: usize, turn_ms: Option<u32>, banks: Option<Vec<u32>> },
    TooMuch,
}

impl GameEngine for Count {
    type Rules = u32;
    type Request = u32;
    type Move = u32;
    type Level = u8;
    type Config = ();
    type Message = Said;
    type Draft = ();
    type Aside = ();

    const PROTOCOL: u32 = 1;

    fn request(by: u32) -> Request<Count> {
        Request::Move(by)
    }

    fn new(target: &u32, players: usize, _seed: u64) -> Count {
        Count { players, current: 0, total: 0, target: *target }
    }

    fn seats(_rules: &u32) -> RangeInclusive<usize> {
        2..=3
    }

    fn players(&self) -> usize {
        self.players
    }

    fn current(&self) -> usize {
        self.current
    }

    fn in_turn(&self) -> bool {
        self.total < self.target
    }

    fn over(&self) -> bool {
        !self.in_turn()
    }

    fn apply(&mut self, seat: usize, by: u32) -> Result<(), Said> {
        if seat != self.current || !(1..=2).contains(&by) || self.over() {
            return Err(Said::TooMuch);
        }
        self.total += by;
        if self.in_turn() {
            self.current = (self.current + 1) % self.players;
        }
        Ok(())
    }

    fn time_out(&mut self, _config: &(), _draft: Option<()>) -> TimedOut<Said> {
        let _ = self.apply(self.current, 1);
        TimedOut { kept: Duration::ZERO, notice: None }
    }

    fn bot_move(&self, _level: u8, _config: &(), _spent: &dyn Fn() -> f64) -> u32 {
        1
    }

    fn bot_fallback(&mut self) {
        let _ = self.apply(self.current, 1);
    }

    fn state(&self, _seat: Option<usize>, clock: &ClockInfo, _open: bool) -> Said {
        Said::State { total: self.total, current: self.current, turn_ms: clock.turn_ms, banks: clock.banks.clone() }
    }

    fn placings(&self) -> Option<Vec<u32>> {
        self.over().then(|| (0..self.players).map(|p| (p != self.current) as u32).collect())
    }
}

type Room = mpsc::Sender<Command<Count>>;

const TIMING: Timing = Timing {
    turn: Duration::from_secs(60),
    absent_turn: Duration::from_secs(5),
    abandoned: Duration::from_secs(600),
    bot_think: Duration::from_millis(5),
    bot_show: Duration::from_millis(5),
    bot_time: Duration::from_millis(100),
    bank: Duration::from_secs(2),
    bank_cap: Duration::from_millis(2500),
    unearned_warning: Duration::from_secs(60),
    unearned_limit: Duration::from_secs(120),
    between_hands: Duration::from_millis(50),
};

fn open_room(timing: Timing, time_bank: bool) -> (Arc<Rooms<Count>>, Room) {
    let rooms = Arc::new(Rooms::new(Config { timing, open_racks: false }, (), bot_pool(1)));
    let room = rooms.create(RoomOptions { turn_seconds: 60, live_turns: true, time_bank, stand_in: 0, game: 100 });
    (rooms, room)
}

struct Member {
    id: PlayerId,
    token: String,
    inbox: mpsc::UnboundedReceiver<Out<Count>>,
}

/// A connection for a member: its id, and where it receives.
fn connection(id: u64) -> (Connection<Count>, mpsc::UnboundedReceiver<Out<Count>>) {
    let (outbox, inbox) = mpsc::unbounded_channel();
    (Connection { id, outbox }, inbox)
}

impl Member {
    async fn join(room: &Room, name: &str, connection_id: u64) -> Member {
        let (connection, inbox) = connection(connection_id);
        let (reply, answer) = oneshot::channel();
        room.send(Command::Join { name: name.into(), watch: false, connection, reply }).await.unwrap();
        let id = answer.await.unwrap().expect("a seat");
        let mut member = Member { id, token: String::new(), inbox };
        let Out::<Count>::Joined { token, .. } = member.receive().await else { panic!("Joined first") };
        member.token = token;
        member
    }

    async fn receive(&mut self) -> Out<Count> {
        tokio::time::timeout(Duration::from_secs(5), self.inbox.recv()).await.expect("a message within 5 s").expect("room open")
    }

    /// The next state: the total, whose turn, its time and the banks.
    async fn state(&mut self) -> (u32, usize, Option<u32>, Option<Vec<u32>>) {
        loop {
            if let Out::<Count>::Game { message: Said::State { total, current, turn_ms, banks } } = self.receive().await {
                return (total, current, turn_ms, banks);
            }
        }
    }

    /// Whether a state arrives before `wait` is up.
    async fn any_state(&mut self, wait: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + wait;
        while let Ok(Some(message)) = tokio::time::timeout_at(deadline, self.inbox.recv()).await {
            if matches!(message, Out::<Count>::Game { message: Said::State { .. } }) {
                return true;
            }
        }
        false
    }
}

/// Ann (the host, who starts) and Bob in a started game.
async fn two_players(room: &Room) -> (Member, Member) {
    let mut ann = Member::join(room, "Ann", 1).await;
    let mut bob = Member::join(room, "Bob", 2).await;
    room.send(Command::Start { player: ann.id }).await.unwrap();
    ann.state().await;
    bob.state().await;
    (ann, bob)
}

#[tokio::test(start_paused = true)]
async fn an_absent_players_turn_is_short_and_given_back() {
    let (_rooms, room) = open_room(TIMING, false);
    let (ann, mut bob) = two_players(&room).await;
    room.send(Command::Disconnect { player: ann.id, connection: 1 }).await.unwrap();
    let (_, current, turn_ms, _) = bob.state().await;
    assert_eq!(current, 0);
    assert!(turn_ms.unwrap() <= 5000, "{turn_ms:?}");

    let (again, _inbox) = connection(3);
    let (reply, answer) = oneshot::channel();
    room.send(Command::Rejoin { token: ann.token.clone(), connection: again, reply }).await.unwrap();
    assert_eq!(answer.await.unwrap(), Ok(ann.id));
    let (_, _, turn_ms, _) = bob.state().await;
    assert!(turn_ms.unwrap() > 55_000, "her full turn back: {turn_ms:?}");

    // Away again, for longer: her turn is played for her.
    room.send(Command::Disconnect { player: ann.id, connection: 3 }).await.unwrap();
    bob.state().await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let (total, current, _, _) = bob.state().await;
    assert_eq!((total, current), (1, 1));
}

#[tokio::test(start_paused = true)]
async fn the_bank_is_used_after_the_turn_and_fills_up() {
    let timing = Timing { turn: Duration::from_secs(1), ..TIMING };
    let (_rooms, room) = open_room(timing, true);
    let (_ann, mut bob) = two_players(&room).await;
    // Ann does nothing: her second of turn, then her 2 s of bank.
    assert!(!bob.any_state(Duration::from_millis(2900)).await, "still her time");
    let (total, current, _, banks) = bob.state().await;
    assert_eq!((total, current, banks), (1, 1, Some(vec![0, 2000])));
    // Bob moves at once: his second left goes into his bank, up to 2.5 s.
    room.send(Command::Move { player: bob.id, mv: 2 }).await.unwrap();
    let (total, _, _, banks) = bob.state().await;
    assert_eq!((total, banks), (3, Some(vec![0, 2500])));
}

#[tokio::test]
async fn a_bot_move_for_a_turn_gone_by_is_dropped() {
    let (_rooms, room) = open_room(TIMING, false);
    let mut ann = Member::join(&room, "Ann", 1).await;
    room.send(Command::AddBot { player: ann.id, level: 0 }).await.unwrap();
    room.send(Command::Start { player: ann.id }).await.unwrap();
    assert_eq!(ann.state().await.1, 0, "Ann's turn");
    // A bot's move for an old turn, and one for this turn when no bot is
    // thinking: neither counts.
    for turn in [0, 1] {
        room.send(Command::BotMove { turn, mv: 2 }).await.unwrap();
    }
    room.send(Command::Move { player: ann.id, mv: 1 }).await.unwrap();
    assert_eq!(ann.state().await.0, 1);
    // The bot's own move.
    assert_eq!(ann.state().await.0, 2);
}

#[tokio::test]
async fn a_refused_move_is_the_games_message() {
    let (_rooms, room) = open_room(TIMING, false);
    let (mut ann, _bob) = two_players(&room).await;
    room.send(Command::Move { player: ann.id, mv: 5 }).await.unwrap();
    assert!(matches!(ann.receive().await, Out::<Count>::Game { message: Said::TooMuch }));
}

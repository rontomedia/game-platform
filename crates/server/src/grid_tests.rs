//! Grid rooms (`Room<GridMatch>`) driven through the room's commands, with
//! no WebSocket or protocol: bots, a person, watchers, pause and step, and
//! the stand-in.

use std::sync::Arc;
use std::time::Duration;

use grid::{BotLevel, Cell, GridError, GridRules, GridView, RuleSet, Square};
use platform::room::{self, Command, Connection, PlayerId, Timing};
use platform::rooms::{bot_pool, Rooms};
use platform::shared::{Out, RoomInfo, RoomOptions, SharedMessage};
use tokio::sync::{mpsc, oneshot};

use crate::grid::{GridMatch, GridMessage};

type Room = mpsc::Sender<Command<GridMatch>>;

/// Bots quick, people's turns long enough.
const FAST: Timing = Timing {
    turn: Duration::from_secs(30),
    absent_turn: Duration::from_secs(30),
    abandoned: Duration::from_secs(30),
    bot_think: Duration::from_millis(5),
    bot_show: Duration::from_millis(5),
    bot_time: Duration::from_millis(100),
    bank: Duration::from_secs(30),
    bank_cap: Duration::from_secs(30),
    unearned_warning: Duration::from_secs(60),
    unearned_limit: Duration::from_secs(120),
    between_hands: Duration::from_millis(50),
};

fn open_room(timing: Timing, rules: GridRules) -> (Arc<Rooms<GridMatch>>, Room) {
    let rooms = Arc::new(Rooms::new(room::Config { timing, open_racks: false }, (), bot_pool(2)));
    let options = RoomOptions { turn_seconds: 60, live_turns: true, time_bank: false, stand_in: BotLevel::Medium, game: rules };
    let room = rooms.create(options);
    (rooms, room)
}

/// A member of a room, as a connection sees it.
struct Member {
    id: PlayerId,
    inbox: mpsc::UnboundedReceiver<Out<GridMatch>>,
}

impl Member {
    async fn join(room: &Room, name: &str, watch: bool) -> Member {
        let (outbox, inbox) = mpsc::unbounded_channel();
        let (reply, answer) = oneshot::channel();
        let connection = Connection { id: rand::random(), outbox };
        room.send(Command::Join { name: name.into(), watch, connection, reply }).await.unwrap();
        Member { id: answer.await.unwrap().expect("a seat"), inbox }
    }

    async fn receive(&mut self) -> Out<GridMatch> {
        tokio::time::timeout(Duration::from_secs(5), self.inbox.recv()).await.expect("a message within 5 s").expect("room open")
    }

    /// The next state, skipping other messages.
    async fn state(&mut self) -> GridView {
        loop {
            if let Out::Game(GridMessage::State { view, .. }) = self.receive().await {
                return view;
            }
        }
    }

    async fn room(&mut self) -> RoomInfo<GridRules, BotLevel> {
        loop {
            if let Out::Shared(SharedMessage::Room { room }) = self.receive().await {
                return room;
            }
        }
    }

    /// The states that arrive within `wait`, the room's last word on
    /// pausing, and nothing else.
    async fn quiet(&mut self, wait: Duration) -> (Vec<GridView>, Option<bool>) {
        let (mut states, mut paused) = (Vec::new(), None);
        while let Ok(Some(message)) = tokio::time::timeout(wait, self.inbox.recv()).await {
            match message {
                Out::Game(GridMessage::State { view, .. }) => states.push(view),
                Out::Shared(SharedMessage::Room { room }) => paused = Some(room.paused),
                _ => {}
            }
        }
        (states, paused)
    }
}

fn c(row: u8, col: u8) -> Cell {
    Cell { row, col }
}

#[tokio::test]
async fn two_hard_bots_draw_and_a_watcher_sees_it() {
    let (_rooms, room) = open_room(FAST, GridRules::default());
    let mut ann = Member::join(&room, "Ann", false).await;
    room.send(Command::Watch { player: ann.id, watching: true }).await.unwrap();
    for _ in 0..2 {
        room.send(Command::AddBot { player: ann.id, level: BotLevel::Hard }).await.unwrap();
    }
    room.send(Command::Start { player: ann.id }).await.unwrap();
    let mut seen = 0;
    let last = loop {
        let view = ann.state().await;
        assert_eq!(view.you, None, "a watcher's view");
        seen += 1;
        if view.outcome.is_some() {
            break view;
        }
    };
    assert_eq!(last.outcome.unwrap().winner, None, "perfect players draw");
    assert_eq!((last.moves.len(), seen), (9, 10), "the first state, then one a mark");
}

#[tokio::test]
async fn a_person_against_a_bot() {
    let timing = Timing { turn: Duration::from_millis(800), ..FAST };
    let (_rooms, room) = open_room(timing, GridRules::default());
    let mut ann = Member::join(&room, "Ann", false).await;
    room.send(Command::AddBot { player: ann.id, level: BotLevel::Easy }).await.unwrap();
    room.send(Command::Start { player: ann.id }).await.unwrap();
    let view = ann.state().await;
    assert_eq!((view.you, view.current), (Some(0), 0), "X starts");
    room.send(Command::Move { player: ann.id, mv: c(1, 1) }).await.unwrap();
    // Her mark, then the bot's.
    while ann.state().await.moves.len() < 2 {}
    // A taken cell is refused, and it's still her turn...
    room.send(Command::Move { player: ann.id, mv: c(1, 1) }).await.unwrap();
    let error = loop {
        if let Out::Game(GridMessage::Rejected { error }) = ann.receive().await {
            break error;
        }
    };
    assert_eq!(error, GridError::Occupied);
    // ...until it runs out: a mark is put down for her.
    let view = ann.state().await;
    assert_eq!(view.moves.len(), 3);
    let last = view.rules.index(view.moves[2]).unwrap();
    assert_eq!(view.board[last], Square::X);
}

#[tokio::test]
async fn a_bots_only_game_pauses_and_steps() {
    let (_rooms, room) = open_room(FAST, GridRules::named(RuleSet::Gomoku));
    let mut ann = Member::join(&room, "Ann", true).await;
    for _ in 0..2 {
        room.send(Command::AddBot { player: ann.id, level: BotLevel::Easy }).await.unwrap();
    }
    room.send(Command::Start { player: ann.id }).await.unwrap();
    room.send(Command::Pause { player: ann.id, paused: true }).await.unwrap();
    while !ann.room().await.paused {}
    let marks = ann.state().await.moves.len();
    let (states, _) = ann.quiet(Duration::from_millis(200)).await;
    assert!(states.iter().all(|v| v.moves.len() == marks), "nothing moves while paused");

    room.send(Command::Step { player: ann.id }).await.unwrap();
    let (states, paused) = ann.quiet(Duration::from_millis(300)).await;
    assert_eq!(states.iter().map(|v| v.moves.len()).collect::<Vec<_>>(), [marks + 1], "one turn");
    assert_eq!(paused, Some(true), "and paused again");

    room.send(Command::Pause { player: ann.id, paused: false }).await.unwrap();
    while ann.state().await.moves.len() < marks + 3 {}
}

#[tokio::test]
async fn the_stand_in_plays_for_who_left() {
    let (_rooms, room) = open_room(FAST, GridRules::default());
    let mut ann = Member::join(&room, "Ann", false).await;
    let bob = Member::join(&room, "Bob", false).await;
    room.send(Command::Start { player: ann.id }).await.unwrap();
    ann.state().await;
    room.send(Command::Leave { player: bob.id }).await.unwrap();
    let seats = ann.room().await.seats;
    assert_eq!((seats[1].name.as_str(), seats[1].bot), ("Bob", Some(BotLevel::Medium)));
    room.send(Command::Move { player: ann.id, mv: c(0, 0) }).await.unwrap();
    let view = loop {
        let view = ann.state().await;
        if view.moves.len() == 2 {
            break view;
        }
    };
    assert_eq!(view.current, 0, "the stand-in moved for Bob");
}

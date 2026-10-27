//! Whole matches through real WebSockets, against a server on a free port.

use std::net::SocketAddr;
use std::time::Duration;

use engine::{PlayerView, TurnError};
use futures_util::{SinkExt, StreamExt};
use engine::BotLevel;
use protocol::{ClientMessage, RoomInfo, RoomOptions, Seat, ServerMessage, PROTOCOL_VERSION};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::*;
use crate::clock::Bonus;

/// Long enough never to run out during a test.
const SLOW: Timing = Timing {
    turn: Duration::from_secs(60),
    absent_turn: Duration::from_secs(60),
    abandoned: Duration::from_secs(60),
    // Bots don't keep tests waiting.
    bot_think: Duration::from_millis(5),
    bot_show: Duration::from_millis(5),
    bank: Duration::from_secs(60),
    bank_cap: Duration::from_secs(90),
    bonus: Bonus { per_tile: Duration::from_secs(15), per_set: Duration::from_secs(20) },
    unearned_warning: Duration::from_secs(60),
    unearned_limit: Duration::from_secs(120),
};

async fn start_server(timing: Timing) -> SocketAddr {
    start_server_with(Config { timing, open_racks: false }).await
}

async fn start_server_with(config: Config) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, config));
    address
}

struct Client(WebSocketStream<MaybeTlsStream<TcpStream>>);

impl Client {
    async fn connect(server: SocketAddr) -> Client {
        let mut client = Client(tokio_tungstenite::connect_async(format!("ws://{server}/ws")).await.unwrap().0);
        assert_eq!(client.receive().await, ServerMessage::Welcome { protocol: PROTOCOL_VERSION });
        client
    }

    async fn send(&mut self, message: ClientMessage) {
        self.0.send(Message::text(serde_json::to_string(&message).unwrap())).await.unwrap();
    }

    async fn send_raw(&mut self, message: Message) {
        self.0.send(message).await.unwrap();
    }

    async fn receive(&mut self) -> ServerMessage {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(5), self.0.next())
                .await
                .expect("a message within 5 s")
                .expect("connection open")
                .unwrap();
            match frame {
                Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
                Message::Ping(_) | Message::Pong(_) => continue,
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    /// The next `Room` message, skipping others.
    async fn room(&mut self) -> RoomInfo {
        loop {
            if let ServerMessage::Room { room } = self.receive().await {
                return room;
            }
        }
    }

    /// The next `State` message, skipping others.
    async fn state(&mut self) -> (PlayerView, Option<u32>) {
        loop {
            if let ServerMessage::State { view, turn_ms, .. } = self.receive().await {
                return (view, turn_ms);
            }
        }
    }

    async fn error(&mut self) -> String {
        match self.receive().await {
            ServerMessage::Error { message } => message,
            other => panic!("expected an error, got {other:?}"),
        }
    }

    /// Creates a room; its code and this player's token.
    async fn create(&mut self, name: &str) -> (String, String) {
        self.create_with(name, RoomOptions::default()).await
    }

    async fn create_with(&mut self, name: &str, options: RoomOptions) -> (String, String) {
        self.send(ClientMessage::CreateRoom { name: name.into(), options }).await;
        let joined = self.receive().await;
        let ServerMessage::Joined { code, token } = joined else { panic!("expected Joined, got {joined:?}") };
        self.room().await;
        (code, token)
    }

    async fn join(&mut self, code: &str, name: &str) -> String {
        self.send(ClientMessage::JoinRoom { code: code.into(), name: name.into() }).await;
        let joined = self.receive().await;
        let ServerMessage::Joined { token, .. } = joined else { panic!("expected Joined, got {joined:?}") };
        self.room().await;
        token
    }
}

/// Two players in a started game.
struct TwoPlayers {
    server: SocketAddr,
    ann: Client,
    bob: Client,
    code: String,
    /// Ann's, the host's.
    token: String,
    bob_token: String,
    /// What Ann saw when the game started (it's her turn).
    ann_view: PlayerView,
}

async fn two_player_game(timing: Timing) -> TwoPlayers {
    two_player_game_with(timing, RoomOptions::default()).await
}

async fn two_player_game_with(timing: Timing, options: RoomOptions) -> TwoPlayers {
    let server = start_server(timing).await;
    let mut ann = Client::connect(server).await;
    let mut bob = Client::connect(server).await;
    let (code, token) = ann.create_with("Ann", options).await;
    let bob_token = bob.join(&code, "Bob").await;
    ann.room().await; // Bob arrived
    ann.send(ClientMessage::Start).await;
    assert!(ann.room().await.started);
    let (ann_view, _) = ann.state().await;
    assert!(bob.room().await.started);
    bob.state().await;
    TwoPlayers { server, ann, bob, code, token, bob_token, ann_view }
}

#[tokio::test]
async fn creating_and_joining_a_room() {
    let server = start_server(SLOW).await;
    let mut ann = Client::connect(server).await;
    ann.send(ClientMessage::CreateRoom { name: "  Ann  ".into(), options: RoomOptions::default() }).await;
    let ServerMessage::Joined { code, .. } = ann.receive().await else { panic!() };
    assert_eq!(code.len(), 4);
    let room = ann.room().await;
    assert_eq!((room.you, room.host, room.started), (0, 0, false));
    assert_eq!(room.seats[0].name, "Ann", "trimmed");

    let mut bob = Client::connect(server).await;
    bob.join(&code.to_lowercase(), "Bob").await;
    let room = ann.room().await;
    assert_eq!(room.seats.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Ann", "Bob"]);
    assert_eq!(room.you, 0);
}

#[tokio::test]
async fn joining_can_fail() {
    let server = start_server(SLOW).await;
    let mut ann = Client::connect(server).await;
    let mut outsider = Client::connect(server).await;
    outsider.send(ClientMessage::JoinRoom { code: "ZZZZ".into(), name: "X".into() }).await;
    assert_eq!(outsider.error().await, "no room with code ZZZZ");

    let (code, _) = ann.create("Ann").await;
    ann.send(ClientMessage::CreateRoom { name: "Ann".into(), options: RoomOptions::default() }).await;
    assert_eq!(ann.error().await, "already in a room");

    let mut others = Vec::new();
    for name in ["B", "C", "D"] {
        let mut client = Client::connect(server).await;
        client.join(&code, name).await;
        others.push(client);
    }
    outsider.send(ClientMessage::JoinRoom { code: code.clone(), name: "E".into() }).await;
    assert_eq!(outsider.error().await, "the room is full (4 players)");
}

#[tokio::test]
async fn only_the_host_starts_and_not_alone() {
    let server = start_server(SLOW).await;
    let mut ann = Client::connect(server).await;
    let (code, _) = ann.create("Ann").await;
    ann.send(ClientMessage::Start).await;
    assert_eq!(ann.error().await, "a game needs at least 2 players");

    let mut bob = Client::connect(server).await;
    bob.join(&code, "Bob").await;
    bob.send(ClientMessage::Start).await;
    assert_eq!(bob.error().await, "only the host can start the game");

    ann.room().await;
    ann.send(ClientMessage::Start).await;
    assert!(ann.room().await.started);
    let mut late = Client::connect(server).await;
    late.send(ClientMessage::JoinRoom { code, name: "Cy".into() }).await;
    assert_eq!(late.error().await, "the game in this room has started");
}

#[tokio::test]
async fn each_player_sees_only_their_own_rack() {
    let server = start_server(SLOW).await;
    let mut ann = Client::connect(server).await;
    let mut bob = Client::connect(server).await;
    let (code, _) = ann.create("Ann").await;
    bob.join(&code, "Bob").await;
    ann.room().await;
    ann.send(ClientMessage::Start).await;
    ann.room().await;
    bob.room().await;
    let (a, turn_ms) = ann.state().await;
    let (b, _) = bob.state().await;
    assert_eq!((a.you, b.you), (0, 1));
    assert_eq!(a.rack.len(), 14);
    assert_eq!(a.rack_sizes, vec![14, 14]);
    assert_ne!(a.rack, b.rack);
    assert_eq!(a.current, 0);
    assert!(turn_ms.unwrap() > 59_000, "{turn_ms:?}");
}

#[tokio::test]
async fn moves_out_of_turn_or_invalid_are_rejected() {
    let TwoPlayers { mut ann, mut bob, .. } = two_player_game(SLOW).await;
    bob.send(ClientMessage::Draw).await;
    assert!(matches!(bob.receive().await, ServerMessage::Rejected { error: TurnError::NotYourTurn, .. }));
    ann.send(ClientMessage::Play { table: vec![] }).await;
    assert!(matches!(ann.receive().await, ServerMessage::Rejected { error: TurnError::NothingPlayed, .. }));

    // Still Ann's turn: her draw goes through, and both see it.
    ann.send(ClientMessage::Draw).await;
    let (a, _) = ann.state().await;
    let (b, _) = bob.state().await;
    assert_eq!((a.current, b.current), (1, 1));
    assert_eq!(a.rack.len(), 15);
    assert_eq!(b.rack_sizes, vec![15, 14]);
}

#[tokio::test]
async fn a_turn_that_runs_out_draws() {
    let timing = Timing { turn: Duration::from_millis(300), ..SLOW };
    let TwoPlayers { mut ann, bob: _bob, .. } = two_player_game(timing).await;
    let (view, _) = ann.state().await;
    assert_eq!(view.current, 1);
    assert_eq!(view.rack.len(), 15, "Ann drew when her time ran out");
}

#[tokio::test]
async fn coming_back_after_a_lost_connection() {
    let server = start_server(SLOW).await;
    let mut ann = Client::connect(server).await;
    let mut bob = Client::connect(server).await;
    let (code, token) = ann.create("Ann").await;
    bob.join(&code, "Bob").await;
    ann.room().await;
    ann.send(ClientMessage::Start).await;
    let (before, _) = ann.state().await;
    bob.state().await;

    drop(ann);
    let room = bob.room().await;
    assert!(!room.seats[0].connected);

    let mut again = Client::connect(server).await;
    again.send(ClientMessage::Rejoin { code: code.clone(), token: "wrong".into() }).await;
    assert_eq!(again.error().await, "no such seat in this room");
    again.send(ClientMessage::Rejoin { code, token }).await;
    assert!(matches!(again.receive().await, ServerMessage::Joined { .. }));
    assert!(again.room().await.seats[0].connected);
    let (after, _) = again.state().await;
    assert_eq!(after.rack, before.rack, "the same seat and rack");
    again.send(ClientMessage::Draw).await;
    assert_eq!(again.state().await.0.rack.len(), 15, "and can play on");
}

#[tokio::test]
async fn an_absent_players_turn_is_short() {
    let timing = Timing { absent_turn: Duration::from_millis(200), ..SLOW };
    let TwoPlayers { ann, mut bob, .. } = two_player_game(timing).await;
    drop(ann);
    // Bob hears that Ann left, then that her turn passed.
    loop {
        let (view, _) = bob.state().await;
        if view.current == 1 {
            assert_eq!(view.rack_sizes, vec![15, 14]);
            break;
        }
    }
}

#[tokio::test]
async fn coming_back_in_time_keeps_the_turn() {
    let timing = Timing { absent_turn: Duration::from_millis(1500), ..SLOW };
    let TwoPlayers { server, ann, mut bob, code, token, .. } = two_player_game(timing).await;
    drop(ann);
    let (view, turn_ms) = bob.state().await;
    assert_eq!(view.current, 0);
    assert!(turn_ms.unwrap() <= 1500, "cut short while Ann is away: {turn_ms:?}");

    let mut again = Client::connect(server).await;
    again.send(ClientMessage::Rejoin { code, token }).await;
    let (view, turn_ms) = again.state().await;
    assert_eq!(view.current, 0, "still her turn");
    assert!(turn_ms.unwrap() > 50_000, "her full time back: {turn_ms:?}");
}

#[tokio::test]
async fn a_game_of_draws_ends_with_scores() {
    let TwoPlayers { mut ann, mut bob, .. } = two_player_game(SLOW).await;
    let mut turns = 0;
    let outcome = loop {
        let mover = if turns % 2 == 0 { &mut ann } else { &mut bob };
        mover.send(ClientMessage::Draw).await;
        let (view, turn_ms) = ann.state().await;
        bob.state().await;
        turns += 1;
        if let Some(outcome) = view.outcome {
            assert_eq!(turn_ms, None, "no timer after the game");
            break outcome;
        }
    };
    // 78 tiles in the pool, then a round of passes.
    assert_eq!(turns, 80);
    assert_eq!(outcome.scores.len(), 2);
    ann.send(ClientMessage::Draw).await;
    assert!(matches!(ann.receive().await, ServerMessage::Rejected { error: TurnError::GameOver, .. }));
}

#[tokio::test]
async fn an_empty_room_closes() {
    let timing = Timing { abandoned: Duration::from_millis(200), ..SLOW };
    let server = start_server(timing).await;
    let mut ann = Client::connect(server).await;
    let (code, _) = ann.create("Ann").await;
    drop(ann);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let mut bob = Client::connect(server).await;
    bob.send(ClientMessage::JoinRoom { code: code.clone(), name: "Bob".into() }).await;
    assert_eq!(bob.error().await, format!("no room with code {code}"));
}

#[tokio::test]
async fn bad_messages_get_an_error() {
    let server = start_server(SLOW).await;
    let mut client = Client::connect(server).await;
    client.send_raw(Message::text("hello")).await;
    assert!(client.error().await.starts_with("not a message"));
    client.send_raw(Message::binary(vec![1, 2, 3])).await;
    assert_eq!(client.error().await, "messages are JSON text");
    client.send(ClientMessage::Draw).await;
    assert_eq!(client.error().await, "not in a room");
}


/// A table in the middle of Ann's turn: three of her tiles put down (not
/// necessarily a valid set).
fn draft(ann_view: &PlayerView) -> Vec<Vec<engine::Tile>> {
    vec![ann_view.rack[..3].to_vec()]
}

#[tokio::test]
async fn the_others_watch_the_current_players_table() {
    let TwoPlayers { mut ann, mut bob, ann_view, .. } = two_player_game(SLOW).await;
    ann.send(ClientMessage::Draft { table: draft(&ann_view) }).await;
    assert_eq!(bob.receive().await, ServerMessage::Draft { by: 0, table: draft(&ann_view) });
    // Ann gets nothing back: her next message is the state after her draw.
    ann.send(ClientMessage::Draw).await;
    assert!(matches!(ann.receive().await, ServerMessage::State { .. }));
}

#[tokio::test]
async fn no_live_turns_when_the_room_said_so() {
    let options = RoomOptions { live_turns: false, ..RoomOptions::default() };
    let TwoPlayers { mut ann, mut bob, ann_view, .. } = two_player_game_with(SLOW, options).await;
    ann.send(ClientMessage::Draft { table: draft(&ann_view) }).await;
    ann.send(ClientMessage::Draw).await;
    assert!(matches!(bob.receive().await, ServerMessage::State { .. }), "no draft first");
}

#[tokio::test]
async fn only_the_current_player_shares_a_draft_and_only_their_tiles() {
    let TwoPlayers { mut ann, mut bob, .. } = two_player_game(SLOW).await;
    // Not Bob's turn: ignored.
    bob.send(ClientMessage::Draft { table: vec![] }).await;
    // Three jokers: Ann can't have them.
    let joker = engine::Tile::Joker;
    ann.send(ClientMessage::Draft { table: vec![vec![joker, joker, joker]] }).await;
    assert_eq!(ann.error().await, "that table has tiles you don't have");
    ann.send(ClientMessage::Draw).await;
    assert!(matches!(bob.receive().await, ServerMessage::State { .. }), "nothing passed on");
}

#[tokio::test]
async fn coming_back_mid_turn_shows_the_draft_until_the_turn_ends() {
    let TwoPlayers { server, mut ann, bob, code, bob_token, ann_view, .. } = two_player_game(SLOW).await;
    drop(bob);
    ann.send(ClientMessage::Draft { table: draft(&ann_view) }).await;
    let rejoin = || ClientMessage::Rejoin { code: code.clone(), token: bob_token.clone() };

    // Bob comes back during Ann's turn: the room, the state, then her draft.
    let mut bob = Client::connect(server).await;
    bob.send(rejoin()).await;
    assert!(matches!(bob.receive().await, ServerMessage::Joined { .. }));
    assert!(matches!(bob.receive().await, ServerMessage::Room { .. }));
    assert!(matches!(bob.receive().await, ServerMessage::State { .. }));
    assert_eq!(bob.receive().await, ServerMessage::Draft { by: 0, table: draft(&ann_view) });

    // After Ann's turn, the draft is gone.
    ann.send(ClientMessage::Draw).await;
    bob.state().await;
    drop(bob);
    let mut bob = Client::connect(server).await;
    bob.send(rejoin()).await;
    bob.state().await;
    bob.send(ClientMessage::Draw).await;
    assert!(matches!(bob.receive().await, ServerMessage::State { .. }), "no draft before it");
}

#[tokio::test]
async fn the_host_seats_bots_before_the_start() {
    let server = start_server(SLOW).await;
    let mut ann = Client::connect(server).await;
    let mut bob = Client::connect(server).await;
    let (code, _) = ann.create("Ann").await;
    ann.send(ClientMessage::AddBot { level: BotLevel::Hard }).await;
    let room = ann.room().await;
    assert_eq!(room.seats[1], Seat { name: "Ada".into(), connected: true, bot: Some(BotLevel::Hard) });
    ann.send(ClientMessage::AddBot { level: BotLevel::Easy }).await;
    assert_eq!(ann.room().await.seats[2].name, "Bruno");

    bob.join(&code, "Bob").await;
    ann.room().await;
    bob.send(ClientMessage::AddBot { level: BotLevel::Easy }).await;
    assert_eq!(bob.error().await, "only the host can do that");
    bob.send(ClientMessage::RemoveBot { seat: 1 }).await;
    assert_eq!(bob.error().await, "only the host can do that");
    ann.send(ClientMessage::RemoveBot { seat: 3 }).await;
    assert_eq!(ann.error().await, "that seat isn't a bot's");
    ann.send(ClientMessage::RemoveBot { seat: 1 }).await;
    let names: Vec<String> = ann.room().await.seats.into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["Ann", "Bruno", "Bob"]);

    // Seats count bots: two more fill the room.
    ann.send(ClientMessage::AddBot { level: BotLevel::Easy }).await;
    ann.room().await;
    ann.send(ClientMessage::AddBot { level: BotLevel::Easy }).await;
    assert_eq!(ann.error().await, "the room is full (4 players)");
}

/// Ann against bots of `levels`: she only ever draws, they play. Returns
/// the drafts the bots showed and the final view.
async fn against_bots(config: Config, levels: &[BotLevel]) -> (usize, PlayerView, Vec<ServerMessage>) {
    let server = start_server_with(config).await;
    let mut ann = Client::connect(server).await;
    ann.create("Ann").await;
    for &level in levels {
        ann.send(ClientMessage::AddBot { level }).await;
        ann.room().await;
    }
    ann.send(ClientMessage::Start).await;
    let mut drafts = 0;
    let mut states = Vec::new();
    loop {
        let message = ann.receive().await;
        match &message {
            ServerMessage::Draft { by, .. } => {
                assert_ne!(*by, 0);
                drafts += 1;
            }
            ServerMessage::State { view, .. } => {
                assert!(view.table.iter().all(|set| engine::classify(set).is_some()));
                if let Some(_outcome) = &view.outcome {
                    let view = view.clone();
                    states.push(message);
                    return (drafts, view, states);
                }
                if view.current == 0 {
                    ann.send(ClientMessage::Draw).await;
                }
            }
            _ => {}
        }
        states.push(message);
    }
}

#[tokio::test]
async fn a_game_against_bots() {
    let (drafts, last, states) = against_bots(Config { timing: SLOW, open_racks: false }, &[BotLevel::Easy, BotLevel::Hard]).await;
    assert!(drafts > 0, "the bots' plays are shown live first");
    // Someone who never plays can't go out; the bots usually do.
    assert!(!last.outcome.unwrap().winners.is_empty());
    assert!(!last.table.is_empty(), "the bots played");
    assert!(states.iter().all(|m| !matches!(m, ServerMessage::State { racks: Some(_), .. })), "racks stay hidden");
}

#[tokio::test]
async fn open_racks_show_every_rack() {
    let (_, _, states) = against_bots(Config { timing: SLOW, open_racks: true }, &[BotLevel::Easy]).await;
    for message in states {
        if let ServerMessage::State { view, racks, .. } = message {
            let racks = racks.expect("every rack");
            assert_eq!(racks.iter().map(Vec::len).collect::<Vec<_>>(), view.rack_sizes);
            assert_eq!(racks[0], view.rack);
        }
    }
}

#[tokio::test]
async fn a_valid_table_is_played_when_time_runs_out() {
    // Live turns off: the server keeps the table anyway, for the timer.
    let timing = Timing { turn: Duration::from_millis(800), ..SLOW };
    let options = RoomOptions { live_turns: false, ..RoomOptions::default() };
    // Deals until Ann's rack has an initial meld (a bot finds it).
    for _ in 0..40 {
        let TwoPlayers { mut ann, mut bob, ann_view, .. } = two_player_game_with(timing, options.clone()).await;
        let engine::Move::Play(table) = engine::bot_move(&ann_view, BotLevel::Hard) else { continue };
        ann.send(ClientMessage::Draft { table: table.clone() }).await;
        let (view, _) = bob.state().await;
        assert_eq!(view.table, table, "played as she left it");
        assert_eq!(view.current, 1);
        assert!(view.rack_sizes[0] < 14, "from her rack, nothing drawn: {:?}", view.rack_sizes);
        return;
    }
    panic!("no deal with an initial meld in 40 tries");
}

#[tokio::test]
async fn an_invalid_table_draws_when_time_runs_out() {
    let timing = Timing { turn: Duration::from_millis(300), ..SLOW };
    let TwoPlayers { mut ann, mut bob, ann_view, .. } = two_player_game(timing).await;
    ann.send(ClientMessage::Draft { table: draft(&ann_view) }).await;
    bob.receive().await; // the draft
    let (view, _) = bob.state().await;
    assert!(view.table.is_empty());
    assert_eq!(view.rack_sizes, vec![15, 14], "Ann drew");
}

#[tokio::test]
async fn the_bank_follows_the_turns_own_time() {
    let timing = Timing { turn: Duration::from_millis(200), bank: Duration::from_millis(500), ..SLOW };
    let options = RoomOptions { time_bank: true, ..RoomOptions::default() };
    let started = tokio::time::Instant::now();
    let TwoPlayers { mut bob, ann: _ann, .. } = two_player_game_with(timing, options).await;
    // Ann (still here) does nothing: her turn's 200 ms, then her bank's 500.
    let (view, _) = loop {
        let state = bob.state().await;
        if state.0.current == 1 {
            break state;
        }
    };
    assert!(started.elapsed() >= Duration::from_millis(650), "{:?}", started.elapsed());
    assert_eq!(view.rack_sizes[0], 15, "then she drew");
    bob.send(ClientMessage::Draw).await;
    let ServerMessage::State { banks, .. } = bob.receive().await else { panic!() };
    let banks = banks.unwrap();
    assert_eq!(banks[0], 0, "Ann's bank is spent");
    assert!((600..=700).contains(&banks[1]), "Bob's has his turn's time left over added: {banks:?}");
}

/// A two-player game in which Ann can make her initial meld; the table
/// for it.
async fn ann_can_meld(timing: Timing, options: RoomOptions) -> (TwoPlayers, Vec<Vec<engine::Tile>>) {
    for _ in 0..40 {
        let game = two_player_game_with(timing, options.clone()).await;
        if let engine::Move::Play(table) = engine::bot_move(&game.ann_view, BotLevel::Hard) {
            return (game, table);
        }
    }
    panic!("no deal with an initial meld in 40 tries");
}

/// The next message of the kind `pick` finds, skipping others.
async fn next<T>(client: &mut Client, pick: impl Fn(ServerMessage) -> Option<T>) -> T {
    loop {
        if let Some(found) = pick(client.receive().await) {
            return found;
        }
    }
}

#[tokio::test]
async fn tiles_put_down_earn_time_once() {
    let options = RoomOptions { tile_bonus: true, ..RoomOptions::default() };
    let (TwoPlayers { mut ann, mut bob, .. }, table) = ann_can_meld(SLOW, options).await;
    let tiles = table.iter().flatten().count() as u32;
    ann.send(ClientMessage::Draft { table: table.clone() }).await;
    let clock = |m| match m {
        ServerMessage::Clock { by, added_ms, turn_ms } => Some((by, added_ms, turn_ms)),
        _ => None,
    };
    let (by, added_ms, turn_ms) = next(&mut bob, clock).await;
    assert_eq!(by, 0);
    // Every set is new and made of her tiles.
    assert_eq!(added_ms, tiles * 15_000 + table.len() as u32 * 20_000);
    assert!(turn_ms > 60_000 + added_ms - 2_000, "{turn_ms}");
    // Taken back and put down again: nothing more.
    ann.send(ClientMessage::Draft { table: vec![] }).await;
    ann.send(ClientMessage::Draft { table: table.clone() }).await;
    ann.send(ClientMessage::Play { table }).await;
    let (view, _) = bob.state().await;
    assert_eq!(view.current, 1, "no second Clock before the state");
}

#[tokio::test]
async fn taking_back_tiles_given_time_for_warns_then_stops_it() {
    let timing = Timing { unearned_warning: Duration::from_secs(1), unearned_limit: Duration::from_secs(1000), ..SLOW };
    let options = RoomOptions { tile_bonus: true, ..RoomOptions::default() };
    let (TwoPlayers { mut ann, .. }, table) = ann_can_meld(timing, options.clone()).await;
    ann.send(ClientMessage::Draft { table }).await;
    ann.send(ClientMessage::Draw).await;
    let notice = |m| match m {
        ServerMessage::Notice { message } => Some(message),
        _ => None,
    };
    assert!(next(&mut ann, notice).await.contains("counts against you"));

    let timing = Timing { unearned_limit: Duration::from_secs(1), ..SLOW };
    let (TwoPlayers { mut ann, mut bob, .. }, table) = ann_can_meld(timing, options).await;
    ann.send(ClientMessage::Draft { table: table.clone() }).await;
    ann.send(ClientMessage::Draw).await;
    assert!(next(&mut ann, notice).await.contains("No more extra time"));
    // Bob past the extra time Ann was given before, to his turn.
    while bob.state().await.0.current != 1 {}
    bob.send(ClientMessage::Draw).await;
    // Her meld again on her next turn: no extra time now.
    ann.state().await;
    ann.send(ClientMessage::Draft { table: table.clone() }).await;
    ann.send(ClientMessage::Play { table }).await;
    loop {
        match bob.receive().await {
            ServerMessage::Clock { .. } => panic!("extra time after it was stopped"),
            ServerMessage::State { view, .. } if view.current == 1 => break,
            _ => {}
        }
    }
}

#[tokio::test]
async fn the_room_chooses_the_turn_length() {
    // A turn of a minute takes 400 ms here; two minutes, 800.
    let timing = Timing { turn: Duration::from_millis(400), ..SLOW };
    let options = RoomOptions { turn_seconds: 120, ..RoomOptions::default() };
    let TwoPlayers { mut ann, .. } = two_player_game_with(timing, options).await;
    let started = tokio::time::Instant::now();
    let (view, _) = ann.state().await;
    assert_eq!(view.current, 1, "her time ran out");
    assert!(started.elapsed() >= Duration::from_millis(700), "{:?}", started.elapsed());
}

#[tokio::test]
async fn time_left_over_goes_into_the_bank() {
    let timing = Timing { turn: Duration::from_secs(2), bank: Duration::from_millis(500), bank_cap: Duration::from_millis(800), ..SLOW };
    let options = RoomOptions { time_bank: true, ..RoomOptions::default() };
    let TwoPlayers { mut ann, mut bob, .. } = two_player_game_with(timing, options).await;
    // Ann draws at once: nearly 2 s left, but the bank holds 800 ms at most.
    ann.send(ClientMessage::Draw).await;
    let ServerMessage::State { banks, .. } = next(&mut bob, |m| matches!(m, ServerMessage::State { .. }).then_some(m)).await else {
        unreachable!()
    };
    assert_eq!(banks.unwrap(), vec![800, 500]);
}

#[tokio::test]
async fn time_given_for_tiles_taken_back_is_not_banked() {
    let timing = Timing { turn: Duration::from_secs(2), bank: Duration::ZERO, bank_cap: Duration::from_secs(1000), ..SLOW };
    let options = RoomOptions { time_bank: true, tile_bonus: true, ..RoomOptions::default() };
    let (TwoPlayers { mut ann, mut bob, .. }, table) = ann_can_meld(timing, options).await;
    // A meld earns her extra time; she takes it back and draws.
    ann.send(ClientMessage::Draft { table }).await;
    ann.send(ClientMessage::Draw).await;
    let banks = next(&mut bob, |m| match m {
        ServerMessage::State { banks, .. } => banks,
        _ => None,
    })
    .await;
    // What was left of her own 2 s, not the extra time.
    assert!((1_000..=2_000).contains(&banks[0]), "{banks:?}");
}

#[tokio::test]
async fn the_valid_part_of_a_table_is_played_when_time_runs_out() {
    let timing = Timing { turn: Duration::from_millis(800), ..SLOW };
    let (TwoPlayers { mut ann, mut bob, ann_view, .. }, meld) = ann_can_meld(timing, RoomOptions::default()).await;
    // Her meld, and two more of her tiles: not a set.
    let mut left = ann_view.rack.clone();
    for tile in meld.iter().flatten() {
        left.remove(left.iter().position(|t| t == tile).unwrap());
    }
    let mut draft = meld.clone();
    draft.push(left[..2].to_vec());
    ann.send(ClientMessage::Draft { table: draft }).await;
    let (view, _) = next(&mut bob, |m| match m {
        ServerMessage::State { view, turn_ms, .. } => Some((view, turn_ms)),
        _ => None,
    })
    .await;
    assert_eq!(view.table, meld, "the meld, without the pair");
    let message = next(&mut ann, |m| match m {
        ServerMessage::Notice { message } => Some(message),
        _ => None,
    })
    .await;
    assert!(message.contains("2 tiles went back to your rack"), "{message}");
}

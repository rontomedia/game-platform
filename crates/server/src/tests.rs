//! Whole matches through real WebSockets, against a server on a free port.

use std::net::SocketAddr;
use std::time::Duration;

use engine::{PlayerView, TurnError};
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientMessage, RoomInfo, RoomOptions, ServerMessage, PROTOCOL_VERSION};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::*;

/// Long enough never to run out during a test.
const SLOW: Timing = Timing {
    turn: Duration::from_secs(60),
    absent_turn: Duration::from_secs(60),
    abandoned: Duration::from_secs(60),
};

async fn start_server(timing: Timing) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, timing));
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
            if let ServerMessage::State { view, turn_ms } = self.receive().await {
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
    let options = RoomOptions { live_turns: false };
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

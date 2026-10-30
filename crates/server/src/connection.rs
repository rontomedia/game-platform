//! One client's WebSocket, for one game's rooms (its path, `/ws/<game>`):
//! turns its messages into room commands, and the room's messages for it
//! into frames.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use platform::engine::GameEngine;
use platform::room::{Command, Connection, Out, Outbox, PlayerId, Reply};
use platform::rooms::Rooms;
use protocol::{ClientMessage, Refusal, PROTOCOL_VERSION};
use tokio::sync::mpsc;

/// Longest player name kept, in characters.
const MAX_NAME: usize = 20;

/// The room this connection has a seat in.
struct Membership<G: GameEngine> {
    room: mpsc::Sender<Command<G>>,
    player: PlayerId,
}

pub async fn run<G: GameEngine>(mut socket: WebSocket, rooms: Arc<Rooms<G>>, address: SocketAddr) {
    let id = rooms.connection_id();
    println!("{address} connected");
    let (outbox, mut inbox) = mpsc::unbounded_channel();
    let _ = outbox.send(Out::<G>::Welcome { protocol: PROTOCOL_VERSION, game: G::PROTOCOL });
    let mut membership = None;
    loop {
        // Both are cancel-safe: whichever loses the race loses nothing.
        tokio::select! {
            Some(message) = inbox.recv() => {
                if socket.send(Message::text(message.to_json())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(text))) => match ClientMessage::from_json(text.as_str()) {
                    Ok(message) => handle(message, &rooms, &mut membership, id, &outbox).await,
                    Err(e) => error::<G>(&outbox, format!("not a message: {e}")),
                },
                Some(Ok(Message::Binary(_))) => error::<G>(&outbox, "messages are JSON text"),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                // Pings are answered by axum.
                Some(Ok(_)) => {}
            },
        }
    }
    if let Some(Membership { room, player }) = membership {
        let _ = room.send(Command::Disconnect { player, connection: id }).await;
    }
    println!("{address} disconnected");
}

fn error<G: GameEngine>(outbox: &Outbox<G>, message: impl Into<String>) {
    let _ = outbox.send(Out::<G>::Error { message: message.into() });
}

fn refuse<G: GameEngine>(outbox: &Outbox<G>, refusal: Refusal) {
    let _ = outbox.send(Out::<G>::Refused { refusal });
}

async fn handle<G: GameEngine>(
    message: ClientMessage<G::Rules, G::Level, G::Request>,
    rooms: &Arc<Rooms<G>>,
    membership: &mut Option<Membership<G>>,
    id: u64,
    outbox: &Outbox<G>,
) {
    let connection = || Connection { id, outbox: outbox.clone() };
    match message {
        ClientMessage::CreateRoom { .. } | ClientMessage::JoinRoom { .. } | ClientMessage::Rejoin { .. }
            if membership.is_some() =>
        {
            refuse::<G>(outbox, Refusal::AlreadyInRoom)
        }
        ClientMessage::CreateRoom { name, options } => {
            let room = rooms.create(options.unwrap_or_default());
            let join = |reply| Command::Join { name: clean(&name), watch: false, connection: connection(), reply };
            *membership = enter(room, outbox, join).await;
        }
        ClientMessage::JoinRoom { code, name, watch } => match rooms.get(&code) {
            Some(room) => {
                let join = |reply| Command::Join { name: clean(&name), watch, connection: connection(), reply };
                *membership = enter(room, outbox, join).await
            }
            None => refuse::<G>(outbox, Refusal::NoSuchRoom { code: code.trim().to_uppercase() }),
        },
        ClientMessage::Rejoin { code, token } => match rooms.get(&code) {
            Some(room) => *membership = enter(room, outbox, |reply| Command::Rejoin { token, connection: connection(), reply }).await,
            None => refuse::<G>(outbox, Refusal::RoomClosed),
        },
        ClientMessage::Leave => {
            // Free to create or join another room.
            if let Some(Membership { room, player }) = membership.take() {
                let _ = room.send(Command::Leave { player }).await;
            }
        }
        ClientMessage::AddBot { level } => forward(membership, outbox, |player| Command::AddBot { player, level }).await,
        ClientMessage::RemoveBot { seat } => {
            forward(membership, outbox, |player| Command::RemoveBot { player, seat }).await
        }
        ClientMessage::Watch { watching } => forward(membership, outbox, |player| Command::Watch { player, watching }).await,
        ClientMessage::Start => forward(membership, outbox, |player| Command::Start { player }).await,
        ClientMessage::Pause { paused } => forward(membership, outbox, |player| Command::Pause { player, paused }).await,
        ClientMessage::Step => forward(membership, outbox, |player| Command::Step { player }).await,
        ClientMessage::Game { message } => {
            forward(membership, outbox, |player| Command::Game { player, request: message }).await
        }
    }
}

/// Asks a room for a seat; the membership if it gave one.
async fn enter<G: GameEngine>(
    room: mpsc::Sender<Command<G>>,
    outbox: &Outbox<G>,
    command: impl FnOnce(Reply) -> Command<G>,
) -> Option<Membership<G>> {
    let (reply, answer) = tokio::sync::oneshot::channel();
    if room.send(command(reply)).await.is_err() {
        refuse::<G>(outbox, Refusal::RoomClosed);
        return None;
    }
    match answer.await {
        Ok(Ok(player)) => Some(Membership { room, player }),
        Ok(Err(refusal)) => {
            refuse::<G>(outbox, refusal);
            None
        }
        Err(_) => {
            refuse::<G>(outbox, Refusal::RoomClosed);
            None
        }
    }
}

/// Passes a request to this connection's room.
async fn forward<G: GameEngine>(
    membership: &mut Option<Membership<G>>,
    outbox: &Outbox<G>,
    command: impl FnOnce(PlayerId) -> Command<G>,
) {
    let Some(Membership { room, player }) = membership else {
        return refuse::<G>(outbox, Refusal::NotInRoom);
    };
    if room.send(command(*player)).await.is_err() {
        *membership = None;
        refuse::<G>(outbox, Refusal::RoomClosed);
    }
}

/// A player name as shown to others: trimmed, not too long, never empty.
fn clean(name: &str) -> String {
    let name: String = name.trim().chars().take(MAX_NAME).collect();
    if name.is_empty() {
        "Player".into()
    } else {
        name
    }
}

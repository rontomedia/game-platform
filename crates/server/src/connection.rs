//! One client's WebSocket: turns its messages into room commands, and the
//! room's messages for it into frames.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use protocol::{ClientMessage, ServerMessage, PROTOCOL_VERSION};
use tokio::sync::{mpsc, oneshot};

use crate::room::{Command, Connection, Outbox, PlayerId};
use crate::rooms::Rooms;

/// Longest player name kept, in characters.
const MAX_NAME: usize = 20;

/// The room this connection has a seat in.
struct Membership {
    room: mpsc::Sender<Command>,
    player: PlayerId,
}

pub async fn run(mut socket: WebSocket, rooms: Arc<Rooms>, address: SocketAddr) {
    let id = rooms.connection_id();
    println!("{address} connected");
    let (outbox, mut inbox) = mpsc::unbounded_channel();
    let _ = outbox.send(ServerMessage::Welcome { protocol: PROTOCOL_VERSION });
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
                    Err(e) => error(&outbox, format!("not a message: {e}")),
                },
                Some(Ok(Message::Binary(_))) => error(&outbox, "messages are JSON text"),
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

fn error(outbox: &Outbox, message: impl Into<String>) {
    let _ = outbox.send(ServerMessage::Error { message: message.into() });
}

async fn handle(message: ClientMessage, rooms: &Arc<Rooms>, membership: &mut Option<Membership>, id: u64, outbox: &Outbox) {
    let connection = || Connection { id, outbox: outbox.clone() };
    match message {
        ClientMessage::CreateRoom { .. } | ClientMessage::JoinRoom { .. } | ClientMessage::Rejoin { .. }
            if membership.is_some() =>
        {
            error(outbox, "already in a room")
        }
        ClientMessage::CreateRoom { name, options } => {
            let room = rooms.create(options.checked());
            let join = |reply| Command::Join { name: clean(&name), watch: false, connection: connection(), reply };
            *membership = enter(room, outbox, join).await;
        }
        ClientMessage::JoinRoom { code, name, watch } => match rooms.get(&code) {
            Some(room) => {
                let join = |reply| Command::Join { name: clean(&name), watch, connection: connection(), reply };
                *membership = enter(room, outbox, join).await
            }
            None => error(outbox, format!("no room with code {}", code.trim().to_uppercase())),
        },
        ClientMessage::Rejoin { code, token } => match rooms.get(&code) {
            Some(room) => *membership = enter(room, outbox, |reply| Command::Rejoin { token, connection: connection(), reply }).await,
            None => error(outbox, "that room has closed"),
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
        ClientMessage::Play { table } => forward(membership, outbox, |player| Command::Play { player, table }).await,
        ClientMessage::Draw => forward(membership, outbox, |player| Command::Draw { player }).await,
        ClientMessage::Draft { table } => forward(membership, outbox, |player| Command::Draft { player, table }).await,
    }
}

/// Asks a room for a seat; the membership if it gave one.
async fn enter(
    room: mpsc::Sender<Command>,
    outbox: &Outbox,
    command: impl FnOnce(oneshot::Sender<Result<PlayerId, String>>) -> Command,
) -> Option<Membership> {
    let (reply, answer) = oneshot::channel();
    if room.send(command(reply)).await.is_err() {
        error(outbox, "that room has closed");
        return None;
    }
    match answer.await {
        Ok(Ok(player)) => Some(Membership { room, player }),
        Ok(Err(message)) => {
            error(outbox, message);
            None
        }
        Err(_) => {
            error(outbox, "that room has closed");
            None
        }
    }
}

/// Passes a move to this connection's room.
async fn forward(membership: &mut Option<Membership>, outbox: &Outbox, command: impl FnOnce(PlayerId) -> Command) {
    let Some(Membership { room, player }) = membership else {
        return error(outbox, "not in a room");
    };
    if room.send(command(*player)).await.is_err() {
        *membership = None;
        error(outbox, "the room has closed");
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

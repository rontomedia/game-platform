//! One client's WebSocket: turns its messages into room commands, and the
//! room's messages for it into frames.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use protocol::{ClientMessage, PROTOCOL_VERSION};
use tokio::sync::mpsc;

use crate::contest::{Contest, Discard, RummyMove};
use crate::room::{self, Connection, PlayerId, Reply};
use crate::rooms;
use crate::shared::{Out, Refusal, SharedMessage};
use crate::wire;

type Command = room::Command<Contest>;
type Outbox = room::Outbox<Contest>;
type Rooms = rooms::Rooms<Contest>;

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
    let _ = outbox.send(Out::Shared(SharedMessage::Welcome { protocol: PROTOCOL_VERSION }));
    let mut membership = None;
    loop {
        // Both are cancel-safe: whichever loses the race loses nothing.
        tokio::select! {
            Some(message) = inbox.recv() => {
                if socket.send(Message::text(wire::server_message(message).to_json())).await.is_err() {
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
    let _ = outbox.send(Out::Shared(SharedMessage::Error { message: message.into() }));
}

fn refuse(outbox: &Outbox, refusal: Refusal) {
    let _ = outbox.send(Out::Shared(SharedMessage::Refused { refusal }));
}

async fn handle(message: ClientMessage, rooms: &Arc<Rooms>, membership: &mut Option<Membership>, id: u64, outbox: &Outbox) {
    let connection = || Connection { id, outbox: outbox.clone() };
    match message {
        ClientMessage::CreateRoom { .. } | ClientMessage::JoinRoom { .. } | ClientMessage::Rejoin { .. }
            if membership.is_some() =>
        {
            refuse(outbox, Refusal::AlreadyInRoom)
        }
        ClientMessage::CreateRoom { name, options } => {
            let room = rooms.create(wire::room_options(options.checked()));
            let join = |reply| Command::Join { name: clean(&name), watch: false, connection: connection(), reply };
            *membership = enter(room, outbox, join).await;
        }
        ClientMessage::JoinRoom { code, name, watch } => match rooms.get(&code) {
            Some(room) => {
                let join = |reply| Command::Join { name: clean(&name), watch, connection: connection(), reply };
                *membership = enter(room, outbox, join).await
            }
            None => refuse(outbox, Refusal::NoSuchRoom { code: code.trim().to_uppercase() }),
        },
        ClientMessage::Rejoin { code, token } => match rooms.get(&code) {
            Some(room) => *membership = enter(room, outbox, |reply| Command::Rejoin { token, connection: connection(), reply }).await,
            None => refuse(outbox, Refusal::RoomClosed),
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
        ClientMessage::Play { table } => {
            forward(membership, outbox, |player| Command::Move { player, mv: RummyMove::Play(table) }).await
        }
        ClientMessage::Draw => forward(membership, outbox, |player| Command::Move { player, mv: RummyMove::Draw }).await,
        ClientMessage::Discard { take_discard, table, discard, finish } => {
            let mv = RummyMove::Discard(Discard { take_discard, table, discard, finish });
            forward(membership, outbox, |player| Command::Move { player, mv }).await
        }
        ClientMessage::Draft { table } => forward(membership, outbox, |player| Command::Draft { player, draft: table }).await,
        ClientMessage::DiscardNext { tiles } => forward(membership, outbox, |player| Command::Aside { player, aside: tiles }).await,
    }
}

/// Asks a room for a seat; the membership if it gave one.
async fn enter(
    room: mpsc::Sender<Command>,
    outbox: &Outbox,
    command: impl FnOnce(Reply) -> Command,
) -> Option<Membership> {
    let (reply, answer) = tokio::sync::oneshot::channel();
    if room.send(command(reply)).await.is_err() {
        refuse(outbox, Refusal::RoomClosed);
        return None;
    }
    match answer.await {
        Ok(Ok(player)) => Some(Membership { room, player }),
        Ok(Err(refusal)) => {
            refuse(outbox, refusal);
            None
        }
        Err(_) => {
            refuse(outbox, Refusal::RoomClosed);
            None
        }
    }
}

/// Passes a move to this connection's room.
async fn forward(membership: &mut Option<Membership>, outbox: &Outbox, command: impl FnOnce(PlayerId) -> Command) {
    let Some(Membership { room, player }) = membership else {
        return refuse(outbox, Refusal::NotInRoom);
    };
    if room.send(command(*player)).await.is_err() {
        *membership = None;
        refuse(outbox, Refusal::RoomClosed);
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

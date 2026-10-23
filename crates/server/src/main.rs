//! The match server. It speaks the protocol (crates/protocol) but hosts no
//! matches yet: it welcomes each client and answers what it sends with an
//! error. Rooms and games come next.

use std::net::SocketAddr;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::ConnectInfo;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use protocol::{ClientMessage, ServerMessage, PROTOCOL_VERSION};
use tokio::net::TcpListener;

/// All interfaces, so phones on the local network can connect.
const ADDRESS: &str = "0.0.0.0:3000";

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind(ADDRESS).await.unwrap_or_else(|e| panic!("can't listen on {ADDRESS}: {e}"));
    println!("Listening on ws://{ADDRESS}/ws");
    serve(listener).await;
}

async fn serve(listener: TcpListener) {
    let app = Router::new().route("/ws", get(upgrade));
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.expect("server failed");
}

async fn upgrade(ws: WebSocketUpgrade, ConnectInfo(address): ConnectInfo<SocketAddr>) -> Response {
    ws.on_upgrade(move |socket| client(socket, address))
}

async fn client(mut socket: WebSocket, address: SocketAddr) {
    println!("{address} connected");
    let mut reply = Some(ServerMessage::Welcome { protocol: PROTOCOL_VERSION });
    loop {
        if let Some(message) = reply.take() {
            if socket.send(Message::text(message.to_json())).await.is_err() {
                break;
            }
        }
        reply = match socket.recv().await {
            Some(Ok(Message::Text(text))) => Some(match ClientMessage::from_json(text.as_str()) {
                Ok(message) => answer(message),
                Err(e) => ServerMessage::Error { message: format!("not a message: {e}") },
            }),
            Some(Ok(Message::Binary(_))) => Some(ServerMessage::Error { message: "messages are JSON text".into() }),
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
            // Pings are answered by axum.
            Some(Ok(_)) => None,
        };
    }
    println!("{address} disconnected");
}

/// The reply to a message, until there are matches to play in.
fn answer(message: ClientMessage) -> ServerMessage {
    match message {
        ClientMessage::Play { .. } | ClientMessage::Draw => ServerMessage::Error { message: "not in a match yet".into() },
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpStream;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

    use super::*;

    type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

    /// A client connected to a server of its own, on a free port.
    async fn connect() -> Client {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(serve(listener));
        tokio_tungstenite::connect_async(format!("ws://{address}/ws")).await.unwrap().0
    }

    async fn receive(client: &mut Client) -> ServerMessage {
        loop {
            match client.next().await.expect("connection open").unwrap() {
                Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
                Message::Ping(_) | Message::Pong(_) => continue,
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn welcomes_with_the_protocol_version() {
        let mut client = connect().await;
        assert_eq!(receive(&mut client).await, ServerMessage::Welcome { protocol: PROTOCOL_VERSION });
    }

    #[tokio::test]
    async fn answers_messages_and_reports_bad_ones() {
        let mut client = connect().await;
        receive(&mut client).await;

        client.send(Message::text(r#"{"type":"draw"}"#)).await.unwrap();
        assert_eq!(receive(&mut client).await, ServerMessage::Error { message: "not in a match yet".into() });

        client.send(Message::text("hello")).await.unwrap();
        assert!(matches!(receive(&mut client).await, ServerMessage::Error { message } if message.starts_with("not a message")));

        client.send(Message::binary(vec![1, 2, 3])).await.unwrap();
        assert_eq!(receive(&mut client).await, ServerMessage::Error { message: "messages are JSON text".into() });

        // Still connected after all that.
        client.send(Message::text(r#"{"type":"play","table":[]}"#)).await.unwrap();
        assert!(matches!(receive(&mut client).await, ServerMessage::Error { .. }));
    }
}

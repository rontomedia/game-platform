//! The match server. For now, a connection test: a WebSocket endpoint at
//! /ws that echoes what each client sends and, unasked, sends it a tick
//! every few seconds.

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::ConnectInfo;
use axum::response::Response;
use axum::routing::get;
use axum::Router;

/// All interfaces, so phones on the local network can connect.
const ADDRESS: &str = "0.0.0.0:3000";
/// How often each client gets a message it didn't ask for.
const TICK: Duration = Duration::from_secs(3);

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ws", get(upgrade));
    let listener = tokio::net::TcpListener::bind(ADDRESS)
        .await
        .unwrap_or_else(|e| panic!("can't listen on {ADDRESS}: {e}"));
    println!("Listening on ws://{ADDRESS}/ws");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("server failed");
}

async fn upgrade(ws: WebSocketUpgrade, ConnectInfo(address): ConnectInfo<SocketAddr>) -> Response {
    ws.on_upgrade(move |socket| client(socket, address))
}

async fn client(mut socket: WebSocket, address: SocketAddr) {
    println!("{address} connected");
    let mut tick = tokio::time::interval(TICK);
    // The first tick is immediate; the welcome stands in for it.
    tick.tick().await;
    let mut ticks = 0;
    let mut reply = Some("welcome".to_owned());
    loop {
        if let Some(text) = reply.take() {
            if socket.send(Message::text(text)).await.is_err() {
                break;
            }
        }
        // Both are cancel-safe: whichever loses the race loses nothing.
        tokio::select! {
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) => {
                    println!("{address}: {}", text.as_str());
                    reply = Some(format!("echo: {}", text.as_str()));
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                // Pings are answered by axum; clients send no binary.
                Some(Ok(_)) => {}
            },
            _ = tick.tick() => {
                ticks += 1;
                reply = Some(format!("tick {ticks}"));
            }
        }
    }
    println!("{address} disconnected");
}

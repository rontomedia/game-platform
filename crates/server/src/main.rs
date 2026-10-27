//! The match server: rooms that 2 to 4 players join with a code, each
//! running one game (crates/engine) with a turn timer. Clients speak
//! crates/protocol over a WebSocket at /ws.

mod connection;
mod room;
mod rooms;
#[cfg(test)]
mod tests;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use room::{Config, Timing};
use rooms::Rooms;
use tokio::net::TcpListener;

/// All interfaces, so phones on the local network can connect.
const ADDRESS: &str = "0.0.0.0:3000";

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind(ADDRESS).await.unwrap_or_else(|e| panic!("can't listen on {ADDRESS}: {e}"));
    println!("Listening on ws://{ADDRESS}/ws");
    // A debugging aid: every player sees every rack, to judge the bots.
    let open_racks = std::env::args().any(|arg| arg == "--open-racks");
    if open_racks {
        println!("--open-racks: every player sees every rack");
    }
    serve(listener, Config { timing: Timing::default(), open_racks }).await;
}

async fn serve(listener: TcpListener, config: Config) {
    let app = Router::new().route("/ws", get(upgrade)).with_state(Arc::new(Rooms::new(config)));
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.expect("server failed");
}

async fn upgrade(
    ws: WebSocketUpgrade,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    State(rooms): State<Arc<Rooms>>,
) -> Response {
    ws.on_upgrade(move |socket| connection::run(socket, rooms, address))
}

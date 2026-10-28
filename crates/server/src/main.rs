//! The match server: rooms that 2 to 4 players join with a code, each
//! running one game (crates/engine) with a turn timer. Clients speak
//! crates/protocol over a WebSocket at /ws.

mod clock;
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
use engine::Lookahead;
use room::{Config, Timing};
use rooms::Rooms;
use tokio::net::TcpListener;

/// All interfaces, so phones on the local network can connect.
const ADDRESS: &str = "0.0.0.0:3000";

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind(ADDRESS).await.unwrap_or_else(|e| panic!("can't listen on {ADDRESS}: {e}"));
    println!("Listening on ws://{ADDRESS}/ws");
    let config = config(std::env::args().skip(1).collect());
    if config.open_racks {
        println!("--open-racks: every player sees every rack");
    }
    println!(
        "Extra Hard looks ahead {:?} for up to {:?} a move; {} bots think at once",
        config.lookahead, config.timing.bot_time, config.bot_threads
    );
    serve(listener, config).await;
}

/// The setup from the command line:
/// - `--open-racks`: every player sees every rack, to judge the bots
///   (never for real games).
/// - `--lookahead OPTIONS,GAMES,ROUNDS`: how far Extra Hard looks ahead
///   (see `engine::Lookahead`; default 1,24,2).
/// - `--bot-time MS`: the longest a bot thinks about a move (default 1000).
/// - `--bot-threads N`: how many bots think at once (default: one fewer
///   than the processor's threads, at least one).
fn config(args: Vec<String>) -> Config {
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str);
    let bad = |flag: &str| -> ! { panic!("{flag}: see `config` in crates/server/src/main.rs for what it takes") };
    let mut timing = Timing::default();
    if let Some(ms) = value("--bot-time") {
        timing.bot_time = std::time::Duration::from_millis(ms.parse().unwrap_or_else(|_| bad("--bot-time")));
    }
    let lookahead = match value("--lookahead") {
        Some(spec) => {
            let n: Vec<usize> = spec.split(',').map(|n| n.trim().parse().unwrap_or_else(|_| bad("--lookahead"))).collect();
            let [options, games, rounds] = n[..] else { bad("--lookahead") };
            Lookahead { options, games, rounds }
        }
        None => Lookahead::default(),
    };
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    let bot_threads = match value("--bot-threads") {
        Some(n) => n.parse().unwrap_or_else(|_| bad("--bot-threads")),
        None => cores.saturating_sub(1).max(1),
    };
    Config { timing, open_racks: args.iter().any(|a| a == "--open-racks"), lookahead, bot_threads }
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

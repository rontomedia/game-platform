//! The match server: rooms that players join with a code, each running
//! one game with a turn timer (crates/platform). Each game's clients speak
//! crates/protocol, the game's messages inside, over a WebSocket at
//! `/ws/<game>`: `/ws/tile-rummy` (games/tile-rummy, `contest.rs`) and
//! `/ws/grid` (games/grid, `grid.rs`). Codes are a game's own.

mod clock;
mod connection;
mod contest;
mod grid;
#[cfg(test)]
mod grid_tests;
#[cfg(test)]
mod tests;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use clock::Bonus;
use contest::{Contest, RummyConfig};
use grid::GridMatch;
use platform::engine::GameEngine;
use tile_rummy::Lookahead;
use platform::room::{self, Timing};
use platform::rooms::{bot_pool, Rooms};
use tokio::net::TcpListener;

/// How the server is set up, from the command line (see `config`).
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub timing: Timing,
    /// Every state tells every player all the racks: for watching how the
    /// bots play, never for real games.
    pub open_racks: bool,
    /// How far Extra Hard bots look ahead.
    pub lookahead: Lookahead,
    /// How many bots may think at once, across all rooms; the others wait
    /// their turn.
    pub bot_threads: usize,
    /// The games this process serves.
    pub games: Games,
}

/// Which games a server serves (`--games`), so a busy one can run on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Games {
    pub tile_rummy: bool,
    pub grid: bool,
}

impl Games {
    pub const ALL: Games = Games { tile_rummy: true, grid: true };
}

/// All interfaces, so phones on the local network can connect.
const ADDRESS: &str = "0.0.0.0:3000";

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind(ADDRESS).await.unwrap_or_else(|e| panic!("can't listen on {ADDRESS}: {e}"));
    let config = config(std::env::args().skip(1).collect());
    for (game, served) in [("tile-rummy", config.games.tile_rummy), ("grid", config.games.grid)] {
        if served {
            println!("Listening on ws://{ADDRESS}/ws/{game}");
        }
    }
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
///   (see `tile_rummy::Lookahead`; default 1,24,2).
/// - `--bot-time MS`: the longest a bot thinks about a move (default 1000).
/// - `--bot-threads N`: how many bots think at once (default: one fewer
///   than the processor's threads, at least one).
/// - `--games tile-rummy,grid`: the games served (default all).
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
    let games = match value("--games") {
        Some(list) => {
            let names: Vec<&str> = list.split(',').map(str::trim).collect();
            if names.iter().any(|n| !["tile-rummy", "grid"].contains(n)) {
                bad("--games");
            }
            Games { tile_rummy: names.contains(&"tile-rummy"), grid: names.contains(&"grid") }
        }
        None => Games::ALL,
    };
    Config { timing, open_racks: args.iter().any(|a| a == "--open-racks"), lookahead, bot_threads, games }
}

async fn serve(listener: TcpListener, config: Config) {
    let rooms = room::Config { timing: config.timing, open_racks: config.open_racks };
    // One pool of bot permits for every game's rooms.
    let thinking = bot_pool(config.bot_threads);
    let mut app = Router::new();
    if config.games.tile_rummy {
        let rummy = RummyConfig { lookahead: config.lookahead, bonus: Bonus::default() };
        app = app.merge(route(Rooms::<Contest>::new(rooms, rummy, thinking.clone()), "/ws/tile-rummy"));
    }
    if config.games.grid {
        app = app.merge(route(Rooms::<GridMatch>::new(rooms, (), thinking), "/ws/grid"));
    }
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.expect("server failed");
}

/// A game's WebSocket route, with its rooms.
fn route<G: GameEngine>(rooms: Rooms<G>, path: &str) -> Router {
    Router::new().route(path, get(upgrade::<G>)).with_state(Arc::new(rooms))
}

async fn upgrade<G: GameEngine>(
    ws: WebSocketUpgrade,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    State(rooms): State<Arc<Rooms<G>>>,
) -> Response {
    ws.on_upgrade(move |socket| connection::run(socket, rooms, address))
}

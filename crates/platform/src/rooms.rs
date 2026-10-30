//! All open rooms of a game, by code.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, Semaphore};

use crate::engine::GameEngine;
use crate::room::{self, Command, Config};
use protocol::RoomOptions;

/// Letters of room codes: no I or O, which read as 1 and 0.
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const CODE_LENGTH: usize = 4;

pub struct Rooms<G: GameEngine> {
    rooms: Mutex<HashMap<String, mpsc::Sender<Command<G>>>>,
    config: Config,
    game_config: G::Config,
    connections: AtomicU64,
    /// A permit for each bot thinking, shared by every room (and every
    /// game's rooms, given the same).
    thinking: Arc<Semaphore>,
}

impl<G: GameEngine> Rooms<G> {
    pub fn new(config: Config, game_config: G::Config, thinking: Arc<Semaphore>) -> Rooms<G> {
        Rooms { rooms: Mutex::default(), config, game_config, connections: AtomicU64::new(0), thinking }
    }

    pub fn thinking(&self) -> Arc<Semaphore> {
        self.thinking.clone()
    }

    /// Opens a room with a new code, its options brought into range.
    pub fn create(self: &Arc<Self>, options: RoomOptions<G::Rules, G::Level>) -> mpsc::Sender<Command<G>> {
        let options = options.checked_turn();
        let options = RoomOptions { stand_in: G::stand_in(options.stand_in), game: G::checked(options.game), ..options };
        let mut rooms = self.rooms.lock().unwrap();
        let code = loop {
            let code: String =
                (0..CODE_LENGTH).map(|_| CODE_LETTERS[rand::random_range(0..CODE_LETTERS.len())] as char).collect();
            if !rooms.contains_key(&code) {
                break code;
            }
        };
        let sender = room::spawn(code.clone(), self.config, self.game_config, options, self.clone());
        rooms.insert(code, sender.clone());
        sender
    }

    /// The room with this code, typed in any case.
    pub fn get(&self, code: &str) -> Option<mpsc::Sender<Command<G>>> {
        self.rooms.lock().unwrap().get(&code.trim().to_uppercase()).cloned()
    }

    pub fn remove(&self, code: &str) {
        self.rooms.lock().unwrap().remove(code);
    }

    /// A number for a new connection, unique while the server runs.
    pub fn connection_id(&self) -> u64 {
        self.connections.fetch_add(1, Ordering::Relaxed)
    }
}

/// The bots' permits: `threads` may think at once.
pub fn bot_pool(threads: usize) -> Arc<Semaphore> {
    Arc::new(Semaphore::new(threads.max(1)))
}

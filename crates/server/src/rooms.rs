//! All open rooms, by code.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use protocol::RoomOptions;
use tokio::sync::mpsc;

use crate::room::{self, Command, Config};

/// Letters of room codes: no I or O, which read as 1 and 0.
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const CODE_LENGTH: usize = 4;

pub struct Rooms {
    rooms: Mutex<HashMap<String, mpsc::Sender<Command>>>,
    config: Config,
    connections: AtomicU64,
}

impl Rooms {
    pub fn new(config: Config) -> Rooms {
        Rooms { rooms: Mutex::default(), config, connections: AtomicU64::new(0) }
    }

    /// Opens a room with a new code.
    pub fn create(self: &Arc<Self>, options: RoomOptions) -> mpsc::Sender<Command> {
        let mut rooms = self.rooms.lock().unwrap();
        let code = loop {
            let code: String =
                (0..CODE_LENGTH).map(|_| CODE_LETTERS[rand::random_range(0..CODE_LETTERS.len())] as char).collect();
            if !rooms.contains_key(&code) {
                break code;
            }
        };
        let sender = room::spawn(code.clone(), self.config, options, self.clone());
        rooms.insert(code, sender.clone());
        sender
    }

    /// The room with this code, typed in any case.
    pub fn get(&self, code: &str) -> Option<mpsc::Sender<Command>> {
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

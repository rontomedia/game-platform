//! The platform's rooms, whatever the game: each a task of its own
//! (`room`), found by code (`rooms`), running a game through the
//! `GameEngine` trait (`engine`), and saying what it does in
//! crates/protocol's shared messages, the game's inside them.

pub mod engine;
pub mod room;
pub mod rooms;
#[cfg(test)]
mod tests;

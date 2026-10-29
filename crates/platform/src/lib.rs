//! The platform's rooms, whatever the game: each a task of its own
//! (`room`), found by code (`rooms`), running a game through the
//! `GameEngine` trait (`engine`), and saying what it does in its own
//! messages (`shared`) and the game's.

pub mod engine;
pub mod room;
pub mod rooms;
pub mod shared;
#[cfg(test)]
mod tests;

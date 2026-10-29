//! The room's messages and options as crates/protocol has them: one to
//! one, so what's sent is as before. (Until the protocol's shared part
//! becomes the room's own types.)

use protocol::{RoomOptions, ServerMessage};
use tile_rummy::BotLevel;

use crate::contest::{Contest, RummyRules};
use crate::shared::{self, Out, SharedMessage};

/// A tile rummy room's options, as the room keeps them.
pub type RummyOptions = shared::RoomOptions<RummyRules, BotLevel>;

/// A client's options (already `checked()`), split into the room's and the game's.
pub fn room_options(options: RoomOptions) -> RummyOptions {
    let RoomOptions { turn_seconds, live_turns, time_bank, tile_bonus, stand_in, false_okey_marks, rules } = options;
    let game = RummyRules { rules, tile_bonus, false_okey_marks };
    shared::RoomOptions { turn_seconds, live_turns, time_bank, stand_in, game }
}

pub fn protocol_options(options: RummyOptions) -> RoomOptions {
    let shared::RoomOptions { turn_seconds, live_turns, time_bank, stand_in, game } = options;
    let RummyRules { rules, tile_bonus, false_okey_marks } = game;
    RoomOptions { turn_seconds, live_turns, time_bank, tile_bonus, stand_in, false_okey_marks, rules }
}

pub fn server_message(message: Out<Contest>) -> ServerMessage {
    let message = match message {
        Out::Game(message) => return message,
        Out::Shared(message) => message,
    };
    match message {
        SharedMessage::Welcome { protocol } => ServerMessage::Welcome { protocol },
        SharedMessage::Joined { code, token } => ServerMessage::Joined { code, token },
        SharedMessage::Room { room } => ServerMessage::Room { room: room_info(room) },
        SharedMessage::Clock { by, added_ms, turn_ms } => ServerMessage::Clock { by, added_ms, turn_ms },
        SharedMessage::Notice { notice } => ServerMessage::Notice { notice: self::notice(notice) },
        SharedMessage::Refused { refusal } => ServerMessage::Refused { refusal: self::refusal(refusal) },
        SharedMessage::Error { message } => ServerMessage::Error { message },
    }
}

fn room_info(room: shared::RoomInfo<RummyRules, BotLevel>) -> protocol::RoomInfo {
    let shared::RoomInfo { code, seats, watchers, you, watching, started, paused, options } = room;
    let seat = |shared::Seat { name, connected, bot, host }| protocol::Seat { name, connected, bot, host };
    protocol::RoomInfo {
        code,
        seats: seats.into_iter().map(seat).collect(),
        watchers: watchers.into_iter().map(seat).collect(),
        you,
        watching,
        started,
        paused,
        options: protocol_options(options),
    }
}

fn notice(notice: shared::Notice) -> protocol::Notice {
    match notice {
        shared::Notice::UnearnedWarning { so_far_ms, limit_ms } => protocol::Notice::UnearnedWarning { so_far_ms, limit_ms },
        shared::Notice::NoMoreBonus { taken_back_ms } => protocol::Notice::NoMoreBonus { taken_back_ms },
    }
}

pub fn refusal(refusal: shared::Refusal) -> protocol::Refusal {
    use protocol::Refusal as P;
    use shared::Refusal as S;
    match refusal {
        S::NoSuchRoom { code } => P::NoSuchRoom { code },
        S::RoomClosed => P::RoomClosed,
        S::RoomFull { max } => P::RoomFull { max },
        S::WatchersFull { max } => P::WatchersFull { max },
        S::GameStarted => P::GameStarted,
        S::NoSuchSeat => P::NoSuchSeat,
        S::OnlyHost => P::OnlyHost,
        S::NotABot => P::NotABot,
        S::TooFewPlayers { min } => P::TooFewPlayers { min },
        S::NoGame => P::NoGame,
        S::NotBotsOnly => P::NotBotsOnly,
        S::Watching => P::Watching,
        S::NotStarted => P::NotStarted,
        S::NotYourTiles => P::NotYourTiles,
        S::AlreadyInRoom => P::AlreadyInRoom,
        S::NotInRoom => P::NotInRoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::RuleSet;
    use serde_json::{to_value, Value};

    /// The room's own types serialize as the protocol's: what step 3 needs
    /// to make them the protocol's.
    #[test]
    fn the_rooms_types_serialize_as_the_protocols() {
        let okey = RuleSet::Okey101(tile_rummy::okey101::Okey101Rules::default());
        let options = RoomOptions { time_bank: true, stand_in: BotLevel::Hard, rules: okey, ..RoomOptions::default() };
        let ours = room_options(options.clone());
        assert_eq!(protocol_options(ours.clone()), options);
        let seat = shared::Seat { name: "Ann".into(), connected: true, bot: Some(BotLevel::Easy), host: false };
        let room = shared::RoomInfo {
            code: "ABCD".into(),
            seats: vec![seat.clone(), shared::Seat { bot: None, ..seat }],
            watchers: vec![],
            you: Some(1),
            watching: None,
            started: true,
            paused: false,
            options: ours,
        };
        let messages = [
            SharedMessage::Room { room },
            SharedMessage::Clock { by: 1, added_ms: 2, turn_ms: 3 },
            SharedMessage::Notice { notice: shared::Notice::NoMoreBonus { taken_back_ms: 9 } },
            SharedMessage::Refused { refusal: shared::Refusal::TooFewPlayers { min: 2 } },
            SharedMessage::Joined { code: "ABCD".into(), token: "t".into() },
        ];
        for message in messages {
            let theirs: Value = to_value(server_message(Out::Shared(message.clone()))).unwrap();
            assert_eq!(to_value(&message).unwrap(), theirs);
        }
    }
}

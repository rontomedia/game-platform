/**
 * The shared protocol's types (crates/protocol), generic over a game's
 * rules, bot levels and messages. Every game's WASM build declares them;
 * they're taken from tile rummy's here, as types only (nothing of it is
 * loaded).
 */
export type {
  BotLevel,
  ClientMessage,
  Notice,
  Refusal,
  RoomInfo,
  RoomOptions,
  Seat,
  ServerMessage,
} from '../games/tile-rummy/wasm/engine.js'

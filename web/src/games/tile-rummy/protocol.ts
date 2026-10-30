/**
 * Tile rummy's messages with the shared protocol's: the generic types from
 * Rust (crates/protocol) filled in with tile rummy's rules, bot levels and
 * messages (games/tile-rummy's `protocol` module).
 */
import type {
  BotLevel,
  ClientMessage,
  Notice,
  RoomInfo,
  RoomOptions,
  RummyMessage,
  RummyNotice,
  RummyRequest,
  RummyRules,
  ServerMessage,
} from './wasm/engine.js'

export type RummyRoom = RoomInfo<RummyRules, BotLevel>
export type RummyOptions = RoomOptions<RummyRules, BotLevel>
export type RummyClientMessage = ClientMessage<RummyRules, BotLevel, RummyRequest>
export type RummyServerMessage = ServerMessage<RummyRules, BotLevel, RummyMessage>

/** What a board sends: a request to the room, or one for the game (sent inside `game`). */
export type Outgoing = Exclude<RummyClientMessage, { type: 'game' }> | RummyRequest

/** A notice from the room or from the game. */
export type AnyNotice = Notice | RummyNotice

/** The room's own requests; anything else goes to the game. */
const ROOM_REQUESTS: ReadonlySet<string> = new Set([
  'createRoom',
  'joinRoom',
  'rejoin',
  'leave',
  'watch',
  'addBot',
  'removeBot',
  'start',
  'pause',
  'step',
] satisfies Exclude<RummyClientMessage['type'], 'game'>[])

/** A board's message as the server reads it. */
export function wire(message: Outgoing): RummyClientMessage {
  return ROOM_REQUESTS.has(message.type) ? (message as RummyClientMessage) : { type: 'game', message: message as RummyRequest }
}

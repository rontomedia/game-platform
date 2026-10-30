/**
 * Grid placement's messages with the shared protocol's: the generic types
 * from Rust (crates/protocol) filled in with the grid's rules, bot levels
 * and messages (games/grid's `protocol` module).
 */
import type { Outgoing } from '../../platform/useRoom.ts'
import type { BotLevel, GridRequest, GridRules, RoomInfo, RoomOptions, RuleSet } from './wasm/engine.js'

export type GridRoom = RoomInfo<GridRules, BotLevel>
export type GridOptions = RoomOptions<GridRules, BotLevel>
/** What the grid's screens send: a request to the room, or a mark (inside `game`). */
export type GridOutgoing = Outgoing<GridRules, BotLevel, GridRequest>

/** A rule set as it's played without house rules (as `GridRules::named`). */
export function named(ruleSet: RuleSet): GridRules {
  switch (ruleSet) {
    case 'gomoku':
      return { ruleSet, width: 15, height: 15, line: 5, boards: 1 }
    case 'notakto':
      return { ruleSet, width: 3, height: 3, line: 3, boards: 3 }
    case 'sos':
      return { ruleSet, width: 8, height: 8, line: 3, boards: 1 }
    case 'ultimate':
      return { ruleSet, width: 3, height: 3, line: 3, boards: 9 }
    default:
      return { ruleSet, width: 3, height: 3, line: 3, boards: 1 }
  }
}

/** The most boards Notakto is played on (`grid::MAX_BOARDS`). */
export const MAX_BOARDS = 4

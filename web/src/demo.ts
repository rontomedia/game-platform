import type { GameState } from './useGame.ts'
import type { RoomInfo, Tile } from './wasm/engine.js'

/**
 * A game in progress with no server, for working on the board's look:
 * open the page with `?demo`.
 */

const n = (color: 'black' | 'blue' | 'orange' | 'red', value: number): Tile => ({ kind: 'number', color, value })
const joker: Tile = { kind: 'joker' }

export const demoRoom: RoomInfo = {
  code: 'KQVT',
  seats: [
    { name: 'Alex', connected: true },
    { name: 'Maya', connected: true },
    { name: 'Jonas', connected: false },
    { name: 'Priya', connected: true },
  ],
  you: 0,
  host: 0,
  started: true,
}

export function demoGame(): GameState {
  return {
    version: 1,
    turnEnds: Date.now() + 42_000,
    // Maya's turn, just before this one.
    lastPlay: {
      by: 1,
      sets: [
        [n('red', 5), n('red', 6), n('red', 7), n('red', 8), n('red', 9)],
        [n('orange', 1), n('orange', 2), n('orange', 3)],
      ],
      played: 4,
      version: 1,
    },
    view: {
      you: 0,
      rules: { rackSize: 14, initialMeld: 30, jokerPenalty: 30 },
      current: 0,
      pool: 38,
      rackSizes: [15, 9, 12, 6],
      melded: [true, true, true, true],
      table: [
        [n('blue', 9), n('blue', 10), n('blue', 11), n('blue', 12)],
        [n('red', 7), n('blue', 7), n('black', 7), n('orange', 7)],
        [n('black', 3), joker, n('black', 5)],
        [n('orange', 10), n('orange', 11), n('orange', 12), n('orange', 13)],
        [n('red', 5), n('red', 6), n('red', 7), n('red', 8), n('red', 9)],
        [n('blue', 2), n('red', 2), n('black', 2)],
        [n('black', 11), n('orange', 11), n('red', 11)],
        [n('orange', 1), n('orange', 2), n('orange', 3)],
      ],
      rack: [
        n('red', 10), n('black', 6), n('blue', 4), n('orange', 5), n('blue', 13), n('red', 1), n('black', 9),
        n('orange', 8), n('blue', 6), joker, n('red', 12), n('black', 12), n('blue', 3), n('orange', 6), n('red', 4),
      ],
    },
  }
}

import type { GameState, OkeyGame, PlainOkeyGame, TurnRecord } from '../../useGame.ts'
import type { HandView, PlainView, RummyRules, Tile } from './wasm/engine.js'
import type { RummyOptions, RummyRoom } from './protocol.ts'

/**
 * A game in progress with no server, for working on the board's look:
 * open the page with `?demo`.
 */

const n = (color: 'black' | 'blue' | 'orange' | 'red', value: number): Tile => ({ kind: 'number', color, value })
const joker: Tile = { kind: 'joker' }

/** A room's options, with tile rummy's `game` as given. */
const options = (game: RummyRules = {}): RummyOptions => ({ liveTurns: true, standIn: 'medium', game })

export const demoRoom: RummyRoom = {
  code: 'KQVT',
  seats: [
    { name: 'Alex', connected: true, host: true },
    { name: 'Maya', connected: true, host: false },
    { name: 'Jonas', connected: false, host: false },
    { name: 'Priya', connected: true, bot: 'hard', host: false },
  ],
  watchers: [],
  you: 0,
  minSeats: 2,
  maxSeats: 4,
  started: true,
  paused: false,
  options: options(),
}

/** `?demo=watcher`: four bots playing, watched by their host. */
export const demoWatcherRoom: RummyRoom = {
  ...demoRoom,
  seats: [
    { name: 'Ada', connected: true, bot: 'easy', host: false },
    { name: 'Bruno', connected: true, bot: 'medium', host: false },
    { name: 'Cleo', connected: true, bot: 'hard', host: false },
    { name: 'Dev', connected: true, bot: 'hard', host: false },
  ],
  watchers: [{ name: 'Alex', connected: true, host: true }],
  you: undefined,
  watching: 0,
}

/** The demo game as someone watching sees it: no rack of their own, every rack shown. */
export function asWatcher(game: GameState): GameState {
  const open = withOpenRacks(game)
  return { ...open, lastPlay: undefined, view: { ...open.view, you: undefined, rack: [] } }
}

/** `watching`: Maya's turn, so the player watches it (with `demoDrafts`). */
export function demoGame(watching = false): GameState {
  return {
    version: 1,
    log: [],
    turnEnds: Date.now() + 42_000,
    // Maya's turn, just before this one (when watching, Maya plays now and
    // the turn before was this player's own).
    lastPlay: watching ? undefined : {
      by: 1,
      sets: [
        [n('red', 5), n('red', 6), n('red', 7), n('red', 8), n('red', 9)],
        [n('orange', 1), n('orange', 2), n('orange', 3)],
      ],
      played: 4,
      tiles: [n('red', 9), n('orange', 1), n('orange', 2), n('orange', 3)],
      version: 1,
    },
    // Drawn on this player's turn before (version 0: no flight now, just the mark).
    drawn: watching ? undefined : { tile: n('red', 4), version: 0 },
    turnSince: 1,
    view: {
      you: 0,
      rules: { rackSize: 14, initialMeld: 30, jokerPenalty: 30 },
      current: watching ? 1 : 0,
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

/**
 * Maya's table as her turn goes on (`?demo=watch`): a new set from her rack,
 * a tile on the end of a run, the orange 7 taken out of the 7s for a set of
 * its own (unfinished, as mid-turn tables are), then back to the start (she
 * reset).
 */
export function demoDrafts(): (Tile[][] | null)[] {
  const table = demoGame().view.table
  const withRun = [...table, [n('red', 10), n('red', 11), n('red', 12)]]
  const extended = withRun.map((set, i) => (i === 0 ? [...set, n('blue', 13)] : set))
  const sevens = extended[1]
  const split = [
    ...extended.map((set, i) => (i === 1 ? sevens.filter((t) => !(t.kind === 'number' && t.color === 'orange')) : set)),
    [n('orange', 7)],
  ]
  return [null, withRun, extended, split, table]
}

/**
 * `?demo=racks`: the demo game as a server started with `--open-racks`
 * shows it, every rack in view and the turns so far.
 */
export function withOpenRacks(game: GameState): GameState {
  const others: Tile[][] = [
    [n('blue', 1), n('blue', 5), n('red', 9), n('black', 13), n('orange', 4), n('red', 3), n('blue', 8), n('black', 1), n('orange', 12)],
    [n('red', 2), n('red', 13), n('black', 7), n('blue', 11), n('orange', 9), n('black', 10), joker, n('blue', 3), n('red', 6), n('orange', 2), n('black', 4), n('blue', 12)],
    [n('orange', 6), n('black', 8), n('red', 11), n('blue', 6), n('orange', 13), n('black', 2)],
  ]
  const log: TurnRecord[] = [
    { by: 2, played: [], drew: [n('blue', 12)], sets: 0, version: 1 },
    { by: 3, played: [n('black', 11), n('orange', 11), n('red', 11)], drew: [], sets: 1, version: 1 },
    { by: 0, played: [], drew: [n('red', 4)], sets: 0, version: 1 },
    { by: 1, played: [n('red', 8), n('red', 9), n('orange', 2), n('orange', 3)], drew: [], sets: 2, version: 1 },
    { by: 2, played: [], drew: [n('black', 4)], sets: 0, version: 1 },
    { by: 3, played: [], drew: [n('black', 2)], sets: 0, version: 1 },
  ].map((turn, i) => ({ ...turn, version: i + 1 }))
  return { ...game, racks: [game.view.rack, ...others], log }
}

/** `?demo=okey`: an Okey 101 room. */
export const demoOkeyRoom: RummyRoom = {
  ...demoRoom,
  options: options({ rules: { kind: 'okey101', hands: 7, progressive: false, penalties: true } }),
}

/**
 * An Okey 101 hand: the indicator blue 5, so the blue 6s are okeys; Maya
 * and Jonas have opened; it's the player's turn to draw or take Priya's 9.
 * `scores`: the hand has just ended, Maya out.
 */
export function demoOkey(scores = false): OkeyGame {
  const okey = n('blue', 6)
  const table = [
    [n('red', 10), n('red', 11), n('red', 12), n('red', 13)],
    [n('black', 8), n('orange', 8), okey],
    // The false okey: the blue 6 the okey stands for.
    [n('blue', 3), n('blue', 4), n('blue', 5), joker],
    [n('blue', 11), n('blue', 12), n('blue', 13)],
  ]
  const rack = [
    n('black', 11), n('black', 12), n('black', 13), n('orange', 11), n('orange', 12), n('orange', 13),
    n('blue', 1), n('blue', 2), okey, n('red', 7), n('red', 7), n('black', 2), n('orange', 9),
    n('red', 4), n('black', 5), n('blue', 9), n('orange', 1), n('red', 2), n('black', 9), n('red', 9), n('black', 6),
  ]
  const hand: HandView = {
    you: 0,
    rules: { hands: 7, progressive: false, penalties: true },
    indicator: n('blue', 5),
    okey,
    rack,
    table,
    rackSizes: [21, 9, 12, 20],
    opened: ['no', 'sets', 'sets', 'no'],
    openingPoints: 101,
    openingPairs: 5,
    discards: [[n('blue', 3)], [n('red', 1), n('black', 4)], [n('orange', 13)], [n('black', 9)]],
    written: [0, 0, 101, 0],
    pool: 24,
    current: 0,
    phase: 'draw',
    end: scores ? { winner: 1, scores: [202, -101, 223, 202] } : undefined,
  }
  return {
    view: { hand, played: scores ? 3 : 2, totals: scores ? [91, -147, 457, 404] : [-111, -46, 234, 202] },
    turnEnds: scores ? undefined : Date.now() + 45_000,
    nextHandAt: scores ? Date.now() + 12_000 : undefined,
    version: 1,
    turnSince: 1,
    // Drawn on the player's last turn: marked on the rack.
    drawn: n('orange', 9),
  }
}

/** `?demo=plain`: a plain Okey room. */
export const demoPlainOkeyRoom: RummyRoom = {
  ...demoRoom,
  options: options({ rules: { kind: 'okey', points: 20 } }),
}

/**
 * A plain Okey hand: the indicator red 9, so the red 10s are okeys; the
 * player has drawn and holds a winning hand but for the orange 2.
 * `scores`: Maya has just finished on the okey.
 */
export function demoPlainOkey(scores = false): PlainOkeyGame {
  const okey = n('red', 10)
  const rack = [
    n('black', 12), n('black', 13), n('black', 1), n('blue', 4), n('blue', 5), n('blue', 6), n('blue', 7),
    n('orange', 9), n('black', 9), n('blue', 9), n('red', 2), n('red', 3), okey, n('orange', 2), n('red', 4),
  ]
  const view: PlainView = {
    you: 0,
    rules: { points: 20 },
    indicator: n('red', 9),
    okey,
    rack,
    rackSizes: [15, 14, 14, 14],
    discards: [[n('blue', 13)], [n('orange', 6), n('black', 3)], [n('red', 12)], [n('orange', 11)]],
    showed: [false, false, true, false],
    pool: 31,
    current: 0,
    phase: 'play',
    points: [16, 20, 13, 9],
    end: scores
      ? { winner: 1, finish: 'sets', okeyLast: true, hand: rack.slice(0, 14), lost: [5, 1, 4, 5] }
      : undefined,
  }
  return {
    view,
    turnEnds: scores ? undefined : Date.now() + 38_000,
    nextHandAt: scores ? Date.now() + 12_000 : undefined,
    version: 1,
    turnSince: 1,
    drawn: n('orange', 9),
  }
}

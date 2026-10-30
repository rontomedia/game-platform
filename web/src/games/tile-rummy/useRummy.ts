import { useRef, useState } from 'react'

import { useRoom } from '../../platform/useRoom.ts'
import { newOrChanged, without } from './arrange.ts'
import type { RummyProblem } from './messages.ts'

import {
  gameProtocolVersion,
  protocolVersion,
  type MatchView,
  type Phase,
  type PlainView,
  type PlayerView,
  type BotLevel,
  type RummyMessage,
  type RummyNotice,
  type RummyRequest,
  type RummyRules,
  type Tile,
} from './wasm/engine.js'

export type { Status } from '../../platform/useRoom.ts'

/** The latest game state, and when its turn timer reading was taken. */
export interface GameState {
  view: PlayerView
  /** `Date.now()` when the current turn runs out, if it can. */
  turnEnds?: number
  /** Bumped on every state, so a board can drop its unsent changes. */
  version: number
  /** The last play by another player, until the table changes again. */
  lastPlay?: LastPlay
  /**
   * Every player's rack, in seat order, from a server started with
   * `--open-racks` (for judging the bots); with the turns it has seen.
   */
  racks?: Tile[][]
  log: TurnRecord[]
  /** Milliseconds left in each player's time bank, as of `turnEnds` for the current player. */
  banks?: number[]
  /** The last extra time given for tiles: to whom, how much, and when (`Date.now()`). */
  bonus?: { by: number; ms: number; at: number }
  /** The tile this player drew to end their last turn, until their next one ends. */
  drawn?: { tile: Tile; version: number }
  /** The state in which the tiles were dealt (nobody has moved yet), seen as it happened. */
  dealt?: number
  /** The state in which the current turn began. */
  turnSince: number
}

/** The latest state of an Okey 101 match. */
export interface OkeyGame {
  view: MatchView
  /** `Date.now()` when the current turn runs out, if it can. */
  turnEnds?: number
  /** Between hands: `Date.now()` when the next is dealt. */
  nextHandAt?: number
  banks?: number[]
  /** Every rack, for watchers of a game only bots play (or `--open-racks`). */
  racks?: Tile[][]
  /** Bumped on every state, so the board starts afresh from it. */
  version: number
  /** The state in which the current turn began. */
  turnSince: number
  /** The tile this player last drew from the pool, this hand (see `okeyDrawn`). */
  drawn?: Tile
}

/** The latest state of a plain Okey game: as `OkeyGame`. */
export interface PlainOkeyGame {
  view: PlainView
  turnEnds?: number
  nextHandAt?: number
  banks?: number[]
  racks?: Tile[][]
  version: number
  turnSince: number
  drawn?: Tile
}

/** A finished turn, as open racks show it. */
export interface TurnRecord {
  by: number
  /** Tiles put down from their rack. */
  played: Tile[]
  /** The tile drawn, if they drew (none: the pool was empty). */
  drew: Tile[]
  /** Sets on the table that are new or changed. */
  sets: number
  version: number
}

/** Turns kept in the log. */
const LOG_LENGTH = 60

/** The current player's table so far this turn, as they shared it. */
export interface Draft {
  by: number
  table: Tile[][]
  /** The state it belongs to: a new state ends it. */
  version: number
}

export interface LastPlay {
  /** Who played. */
  by: number
  /** The sets they made or changed, as they left them. */
  sets: Tile[][]
  /** Tiles they put down from their rack. */
  played: number
  /** Those tiles. */
  tiles: Tile[]
  /** The state it arrived with: on later states it's old news. */
  version: number
}

/**
 * What changed between two states of the table, if anything: a play by
 * the player whose turn it was (after a draw, the last play stands; after
 * your own, there's nothing to point out).
 */
function lastPlay(previous: GameState | null, view: PlayerView, version: number): LastPlay | undefined {
  if (!previous) return undefined
  const sets = newOrChanged(previous.view.table, view.table)
  const played = view.table.flat().length - previous.view.table.flat().length
  if (sets.length === 0 && played === 0) return previous.lastPlay
  const by = previous.view.current
  const tiles = without(view.table.flat(), previous.view.table.flat())
  return by === view.you ? undefined : { by, sets, played, tiles, version }
}

/**
 * What this player drew, if their turn has just ended in a draw; else what
 * they last drew, until their next turn ends.
 */
function drawn(previous: GameState | null, view: PlayerView, version: number): GameState['drawn'] {
  if (!previous) return undefined
  const theirs = previous.view.current === view.you && !previous.view.outcome
  if (!theirs || view.current === previous.view.current) return previous.drawn
  const drew = without(view.rack, previous.view.rack)
  return drew.length === 1 ? { tile: drew[0], version } : undefined
}

/** What an Okey hand shows of whose turn it is and the player's rack. */
interface OkeyTurn {
  you?: number
  current: number
  phase: Phase
  rack: Tile[]
  end?: unknown
}

/**
 * Okey: the tile this player last drew from the pool, found as their turn
 * goes from drawing to playing (taking a discard is part of the turn sent
 * whole, so it isn't one). Kept until they draw again; a new hand clears it.
 */
function okeyDrawn(previous: OkeyTurn | undefined, view: OkeyTurn, kept: Tile | undefined): Tile | undefined {
  if (!previous || previous.end) return undefined
  const drew = previous.current === view.you && view.current === view.you && previous.phase === 'draw' && view.phase === 'play'
  if (!drew) return kept
  const added = without(view.rack, previous.rack)
  return added.length === 1 ? added[0] : undefined
}

/** Whether nobody has moved yet: every rack as dealt, the table empty. */
function freshDeal(view: PlayerView): boolean {
  return view.table.length === 0 && view.rackSizes.every((size) => size === view.rules.rackSize) && !view.outcome
}

/** The turn that led from `previous` to `view`, if both show every rack. */
function turnRecord(previous: GameState | null, view: PlayerView, racks: Tile[][] | undefined, version: number) {
  if (!previous?.racks || !racks) return undefined
  // The same turn sent again (someone came back, the game was paused).
  if (view.current === previous.view.current && !view.outcome) return undefined
  const by = previous.view.current
  return {
    by,
    played: without(previous.racks[by], racks[by]),
    drew: without(racks[by], previous.racks[by]),
    sets: newOrChanged(previous.view.table, view.table).length,
    version,
  } satisfies TurnRecord
}

/** A connection to the match server for tile rummy: the room and game this tab is in. */
export function useRummy() {
  const [game, setGame] = useState<GameState | null>(null)
  const [okey, setOkey] = useState<OkeyGame | null>(null)
  const [plainOkey, setPlainOkey] = useState<PlainOkeyGame | null>(null)
  const [draft, setDraft] = useState<Draft | null>(null)
  const version = useRef(0)
  const connection = useRoom<RummyRules, BotLevel, RummyMessage, RummyRequest, RummyProblem, RummyNotice>({
    game: 'tile-rummy',
    protocol: protocolVersion(),
    gameProtocol: gameProtocolVersion(),
    onGame,
    onClock: (message) => {
      const now = Date.now()
      setGame((previous) => {
        if (!previous) return previous
        // Time already taken from the bank is spent; the turn's own time starts again.
        const overdue = previous.turnEnds === undefined ? 0 : Math.max(0, now - previous.turnEnds)
        const banks = previous.banks?.map((ms, seat) => (seat === message.by ? Math.max(0, ms - overdue) : ms))
        return { ...previous, turnEnds: now + message.turnMs, banks, bonus: { by: message.by, ms: message.addedMs, at: now } }
      })
    },
    onLeave: () => {
      setGame(null)
      setOkey(null)
      setPlainOkey(null)
      setDraft(null)
    },
  })
  const { setProblem, setNotice } = connection

  /** Tile rummy's own messages (`game`). */
  function onGame(message: RummyMessage) {
    switch (message.type) {
      case 'state': {
        const next = ++version.current
        const turnEnds = message.turnMs === undefined ? undefined : Date.now() + message.turnMs
        setGame((previous) => {
          const turn = turnRecord(previous, message.view, message.racks, next)
          const log = previous?.log ?? []
          return {
            view: message.view,
            turnEnds,
            version: next,
            lastPlay: lastPlay(previous, message.view, next),
            racks: message.racks,
            log: turn ? [...log, turn].slice(-LOG_LENGTH) : log,
            banks: message.banks,
            drawn: drawn(previous, message.view, next),
            // Not again when the same deal is sent again (someone came back).
            dealt: freshDeal(message.view) ? (previous && freshDeal(previous.view) ? previous.dealt : next) : undefined,
            turnSince:
              previous && previous.view.current === message.view.current ? previous.turnSince : next,
          }
        })
        setDraft(null)
        setProblem(null)
        break
      }
      case 'okeyState': {
        const next = ++version.current
        const now = Date.now()
        setOkey((previous) => ({
          view: message.view,
          turnEnds: message.turnMs === undefined ? undefined : now + message.turnMs,
          nextHandAt: message.nextHandMs === undefined ? undefined : now + message.nextHandMs,
          banks: message.banks,
          racks: message.racks,
          version: next,
          turnSince:
            previous && previous.view.hand.current === message.view.hand.current && !previous.view.hand.end
              ? previous.turnSince
              : next,
          drawn: okeyDrawn(previous?.view.hand, message.view.hand, previous?.drawn),
        }))
        setDraft(null)
        setProblem(null)
        break
      }
      case 'plainOkeyState': {
        const next = ++version.current
        const now = Date.now()
        setPlainOkey((previous) => ({
          view: message.view,
          turnEnds: message.turnMs === undefined ? undefined : now + message.turnMs,
          nextHandAt: message.nextHandMs === undefined ? undefined : now + message.nextHandMs,
          banks: message.banks,
          racks: message.racks,
          version: next,
          turnSince:
            previous && previous.view.current === message.view.current && !previous.view.end
              ? previous.turnSince
              : next,
          drawn: okeyDrawn(previous?.view, message.view, previous?.drawn),
        }))
        setProblem(null)
        break
      }
      case 'okeyRejected':
        setProblem({ kind: 'okey', error: message.error })
        break
      case 'draft':
        setDraft({ by: message.by, table: message.table, version: version.current })
        break
      case 'rejected':
        setProblem({ kind: 'turn', error: message.error })
        break
      case 'wrongRules':
        setProblem({ kind: 'text', message: 'a move of another rule set than the room’s' })
        break
      case 'notice':
        setNotice(message.notice)
        break
    }
  }

  return { ...connection, game, okey, plainOkey, draft }
}

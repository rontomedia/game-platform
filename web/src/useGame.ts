import { useCallback, useEffect, useRef, useState } from 'react'

import { newOrChanged, without } from './games/tile-rummy/arrange.ts'
import type { Problem } from './messages.ts'

import {
  protocolVersion,
  type ClientMessage,
  type MatchView,
  type Notice,
  type Phase,
  type PlainView,
  type PlayerView,
  type RoomInfo,
  type ServerMessage,
  type Tile,
} from './games/tile-rummy/wasm/engine.js'

export type Status = 'connecting' | 'open' | 'closed'

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

/**
 * The match server's WebSocket. In development Vite proxies /ws on the
 * page's own origin to it (see vite.config.ts); VITE_SERVER_URL overrides.
 */
export const SERVER_URL =
  import.meta.env.VITE_SERVER_URL ?? `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`

/**
 * This tab's seat: `{ code, token }` from `joined`, to take the seat back
 * after a reload or a lost connection. Per tab, so two tabs are two players.
 */
const SEAT_KEY = 'rummy.seat'

/** How long a notice from the server stays up. */
const NOTICE_MS = 15_000

/** Wait before connecting again after losing the connection. */
const RETRY_MS = 2000

interface SavedSeat {
  code: string
  token: string
}

function savedSeat(): SavedSeat | null {
  try {
    return JSON.parse(sessionStorage.getItem(SEAT_KEY) ?? 'null') as SavedSeat | null
  } catch {
    return null
  }
}

function saveSeat(seat: SavedSeat | null) {
  try {
    if (seat) sessionStorage.setItem(SEAT_KEY, JSON.stringify(seat))
    else sessionStorage.removeItem(SEAT_KEY)
  } catch {
    // Without storage, a reload just loses the seat.
  }
}

/** The server's messages, typed from Rust (crates/protocol). */
function parse(data: unknown): ServerMessage {
  try {
    return JSON.parse(String(data)) as ServerMessage
  } catch {
    return { type: 'error', message: `unreadable message from the server: ${String(data).slice(0, 80)}` }
  }
}

/** A connection to the match server: the room and game this tab is in. */
export function useGame() {
  const [status, setStatus] = useState<Status>('connecting')
  const [room, setRoom] = useState<RoomInfo | null>(null)
  const [game, setGame] = useState<GameState | null>(null)
  const [okey, setOkey] = useState<OkeyGame | null>(null)
  const [plainOkey, setPlainOkey] = useState<PlainOkeyGame | null>(null)
  const [draft, setDraft] = useState<Draft | null>(null)
  /** The last refusal or error from the server, until the next change. */
  const [problem, setProblem] = useState<Problem | null>(null)
  /** The last notice from the server, for a while (`NOTICE_MS`). */
  const [notice, setNotice] = useState<Notice | null>(null)
  useEffect(() => {
    if (notice === null) return
    const timer = setTimeout(() => setNotice(null), NOTICE_MS)
    return () => clearTimeout(timer)
  }, [notice])
  /** Set when the server speaks another protocol version than this page. */
  const [mismatch, setMismatch] = useState<number | null>(null)
  // Bumped to connect again.
  const [attempt, setAttempt] = useState(0)
  const socket = useRef<WebSocket | null>(null)
  const version = useRef(0)
  // Whether the last request was a Rejoin, whose failure means the seat is gone.
  const rejoining = useRef(false)

  const send = useCallback((message: ClientMessage) => {
    if (socket.current?.readyState !== WebSocket.OPEN) return
    socket.current.send(JSON.stringify(message))
  }, [])

  useEffect(() => {
    const ws = new WebSocket(SERVER_URL)
    socket.current = ws
    let retry: ReturnType<typeof setTimeout> | undefined
    ws.onopen = () => setStatus('open')
    ws.onclose = () => {
      setStatus('closed')
      // With a seat to go back to, keep trying.
      if (savedSeat()) {
        retry = setTimeout(() => {
          setStatus('connecting')
          setAttempt((n) => n + 1)
        }, RETRY_MS)
      }
    }
    ws.onmessage = (e) => {
      const message = parse(e.data)
      switch (message.type) {
        case 'welcome': {
          setMismatch(message.protocol === protocolVersion() ? null : message.protocol)
          const seat = savedSeat()
          if (seat) {
            rejoining.current = true
            ws.send(JSON.stringify({ type: 'rejoin', ...seat } satisfies ClientMessage))
          }
          break
        }
        case 'joined':
          rejoining.current = false
          saveSeat({ code: message.code, token: message.token })
          setProblem(null)
          break
        case 'room':
          setRoom(message.room)
          break
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
        case 'clock': {
          const now = Date.now()
          setGame((previous) => {
            if (!previous) return previous
            // Time already taken from the bank is spent; the turn's own time starts again.
            const overdue = previous.turnEnds === undefined ? 0 : Math.max(0, now - previous.turnEnds)
            const banks = previous.banks?.map((ms, seat) => (seat === message.by ? Math.max(0, ms - overdue) : ms))
            return { ...previous, turnEnds: now + message.turnMs, banks, bonus: { by: message.by, ms: message.addedMs, at: now } }
          })
          break
        }
        case 'notice':
          setNotice(message.notice)
          break
        case 'draft':
          setDraft({ by: message.by, table: message.table, version: version.current })
          break
        case 'rejected':
          setProblem({ kind: 'turn', error: message.error })
          break
        case 'refused':
        case 'error':
          if (rejoining.current) {
            // The seat is gone (the room closed); start over.
            rejoining.current = false
            saveSeat(null)
          }
          setProblem(
            message.type === 'refused'
              ? { kind: 'refused', refusal: message.refusal }
              : { kind: 'text', message: message.message },
          )
          break

      }
    }
    return () => {
      clearTimeout(retry)
      ws.onclose = null
      ws.close()
    }
  }, [attempt])

  const reconnect = useCallback(() => {
    setStatus('connecting')
    setAttempt((n) => n + 1)
  }, [])

  /**
   * Leaves the room for good (a bot takes over the seat in a game going
   * on) and starts afresh on a new connection.
   */
  const leave = useCallback(() => {
    send({ type: 'leave' })
    saveSeat(null)
    setRoom(null)
    setGame(null)
    setOkey(null)
    setPlainOkey(null)
    setDraft(null)
    setProblem(null)
    reconnect()
  }, [send, reconnect])

  return { status, room, game, okey, plainOkey, draft, problem, notice, mismatch, send, reconnect, leave }
}

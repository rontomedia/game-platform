import { useCallback, useEffect, useRef, useState } from 'react'

import { newOrChanged, without } from './arrange.ts'

import {
  protocolVersion,
  type ClientMessage,
  type PlayerView,
  type RoomInfo,
  type ServerMessage,
  type Tile,
} from './wasm/engine.js'

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
  return by === view.you ? undefined : { by, sets, played, version }
}

/** The turn that led from `previous` to `view`, if both show every rack. */
function turnRecord(previous: GameState | null, view: PlayerView, racks: Tile[][] | undefined, version: number) {
  if (!previous?.racks || !racks) return undefined
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
  const [draft, setDraft] = useState<Draft | null>(null)
  /** The last refusal or error from the server, until the next change. */
  const [problem, setProblem] = useState<string | null>(null)
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
            }
          })
          setDraft(null)
          setProblem(null)
          break
        }
        case 'draft':
          setDraft({ by: message.by, table: message.table, version: version.current })
          break
        case 'rejected':
          setProblem(message.message)
          break
        case 'error':
          if (rejoining.current) {
            // The seat is gone (the room closed); start over.
            rejoining.current = false
            saveSeat(null)
          }
          setProblem(message.message)
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

  /** Gives up this tab's seat and starts afresh on a new connection. */
  const leave = useCallback(() => {
    saveSeat(null)
    setRoom(null)
    setGame(null)
    setDraft(null)
    setProblem(null)
    reconnect()
  }, [reconnect])

  return { status, room, game, draft, problem, mismatch, send, reconnect, leave }
}

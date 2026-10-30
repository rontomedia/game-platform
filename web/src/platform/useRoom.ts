import { useCallback, useEffect, useRef, useState } from 'react'

import type { ClientMessage, Notice, Refusal, RoomInfo, ServerMessage } from './protocol.ts'

export type Status = 'connecting' | 'open' | 'closed'

/**
 * The room's own problems: a refusal, or the server's English for a
 * message it couldn't read (a bug, not for players). A game adds its own.
 */
export type RoomProblem = { kind: 'refused'; refusal: Refusal } | { kind: 'text'; message: string }

/**
 * The match server's WebSockets, one path per game (`/ws/<game>`). In
 * development Vite proxies /ws on the page's own origin to it (see
 * vite.config.ts); VITE_SERVER_URL overrides the part up to `/ws`.
 */
export const SERVER_URL: string =
  import.meta.env.VITE_SERVER_URL ?? `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`

/** How long a notice from the server stays up. */
const NOTICE_MS = 15_000

/** Wait before connecting again after losing the connection. */
const RETRY_MS = 2000

interface SavedSeat {
  code: string
  token: string
}

/**
 * This tab's seat in a game's room: `{ code, token }` from `joined`, to take
 * the seat back after a reload or a lost connection. Per tab, so two tabs
 * are two players.
 */
const seatKey = (game: string) => `seat.${game}`

function savedSeat(game: string): SavedSeat | null {
  try {
    return JSON.parse(sessionStorage.getItem(seatKey(game)) ?? 'null') as SavedSeat | null
  } catch {
    return null
  }
}

function saveSeat(game: string, seat: SavedSeat | null) {
  try {
    if (seat) sessionStorage.setItem(seatKey(game), JSON.stringify(seat))
    else sessionStorage.removeItem(seatKey(game))
  } catch {
    // Without storage, a reload just loses the seat.
  }
}

/** What a game tells the room hook about itself. */
export interface GameLink<M> {
  /** Its path under /ws, and its seats' storage key. */
  game: string
  /** The shared protocol's version this page speaks, and the game's. */
  protocol: number
  gameProtocol: number
  /** A message from the game (inside `game`). Read when it arrives, so it may change. */
  onGame: (message: M) => void
  /** The current player was given extra time. */
  onClock?: (clock: { by: number; addedMs: number; turnMs: number }) => void
  /** The player has left: the game's state is to be forgotten. */
  onLeave?: () => void
}

/** The room's own requests; anything else a board sends is the game's, sent inside `game`. */
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
] satisfies Exclude<ClientMessage<unknown, unknown, never>['type'], 'game'>[])

/** What a game's screens send: a request to the room, or one for the game. */
export type Outgoing<R, L, Request> = Exclude<ClientMessage<R, L, Request>, { type: 'game' }> | Request

/**
 * A connection to the match server for one game: the room this tab is in,
 * what's gone wrong, notices. The game's own messages go to `onGame`; the
 * game may set `problem` and `notice` too (`P`, `N`: its own kinds).
 */
export function useRoom<R, L, M, Request extends { type: string }, P = never, N = never>(link: GameLink<M>) {
  type Out = Outgoing<R, L, Request>
  const [status, setStatus] = useState<Status>('connecting')
  const [room, setRoom] = useState<RoomInfo<R, L> | null>(null)
  /** The last refusal or error, until the next change. */
  const [problem, setProblem] = useState<RoomProblem | P | null>(null)
  /** The last notice, for a while (`NOTICE_MS`). */
  const [notice, setNotice] = useState<Notice | N | null>(null)
  useEffect(() => {
    if (notice === null) return
    const timer = setTimeout(() => setNotice(null), NOTICE_MS)
    return () => clearTimeout(timer)
  }, [notice])
  /** Set when the server speaks other protocol versions than this page: "shared.game". */
  const [mismatch, setMismatch] = useState<string | null>(null)
  // Bumped to connect again.
  const [attempt, setAttempt] = useState(0)
  const socket = useRef<WebSocket | null>(null)
  // Whether the last request was a Rejoin, whose failure means the seat is gone.
  const rejoining = useRef(false)
  // The game's callbacks as of the last render.
  const latest = useRef(link)
  useEffect(() => {
    latest.current = link
  })
  const { game } = link

  const send = useCallback((message: Out) => {
    if (socket.current?.readyState !== WebSocket.OPEN) return
    const wire = ROOM_REQUESTS.has(message.type) ? message : { type: 'game', message }
    socket.current.send(JSON.stringify(wire))
  }, [])

  useEffect(() => {
    const ws = new WebSocket(`${SERVER_URL}/${game}`)
    socket.current = ws
    let retry: ReturnType<typeof setTimeout> | undefined
    ws.onopen = () => setStatus('open')
    ws.onclose = () => {
      setStatus('closed')
      // With a seat to go back to, keep trying.
      if (savedSeat(game)) {
        retry = setTimeout(() => {
          setStatus('connecting')
          setAttempt((n) => n + 1)
        }, RETRY_MS)
      }
    }
    ws.onmessage = (e) => {
      const message = parse<R, L, M>(e.data)
      const link = latest.current
      switch (message.type) {
        case 'welcome': {
          const same = message.protocol === link.protocol && message.game === link.gameProtocol
          setMismatch(same ? null : `${message.protocol}.${message.game}`)
          const seat = savedSeat(game)
          if (seat) {
            rejoining.current = true
            ws.send(JSON.stringify({ type: 'rejoin', ...seat }))
          }
          break
        }
        case 'joined':
          rejoining.current = false
          saveSeat(game, { code: message.code, token: message.token })
          setProblem(null)
          break
        case 'room':
          setRoom(message.room)
          break
        case 'clock':
          link.onClock?.(message)
          break
        case 'notice':
          setNotice(message.notice)
          break
        case 'refused':
        case 'error':
          if (rejoining.current) {
            // The seat is gone (the room closed); start over.
            rejoining.current = false
            saveSeat(game, null)
          }
          setProblem(
            message.type === 'refused'
              ? { kind: 'refused', refusal: message.refusal }
              : { kind: 'text', message: message.message },
          )
          break
        case 'game':
          link.onGame(message.message)
          break
      }
    }
    return () => {
      clearTimeout(retry)
      ws.onclose = null
      ws.close()
    }
  }, [attempt, game])

  const reconnect = useCallback(() => {
    setStatus('connecting')
    setAttempt((n) => n + 1)
  }, [])

  /**
   * Leaves the room for good (a bot takes over the seat in a game going
   * on) and starts afresh on a new connection.
   */
  const leave = useCallback(() => {
    send({ type: 'leave' } as Out)
    saveSeat(game, null)
    setRoom(null)
    setProblem(null)
    latest.current.onLeave?.()
    reconnect()
  }, [send, reconnect, game])

  return { status, room, problem, setProblem, notice, setNotice, mismatch, send, reconnect, leave }
}

/** The server's messages, typed from Rust (crates/protocol). */
function parse<R, L, M>(data: unknown): ServerMessage<R, L, M> {
  try {
    return JSON.parse(String(data)) as ServerMessage<R, L, M>
  } catch {
    return { type: 'error', message: `unreadable message from the server: ${String(data).slice(0, 80)}` }
  }
}

import { useCallback, useEffect, useRef, useState } from 'react'

import { protocolVersion, type ClientMessage, type ServerMessage } from './wasm/engine.js'

export type Status = 'connecting' | 'open' | 'closed'

/** A message sent or received, for showing the conversation. */
export type Entry = { id: number } & ({ sent: ClientMessage } | { received: ServerMessage })

/** Entries kept. */
const KEEP = 50

/**
 * The match server's WebSocket. In development Vite proxies /ws on the
 * page's own origin to it (see vite.config.ts); VITE_SERVER_URL overrides.
 */
export const SERVER_URL =
  import.meta.env.VITE_SERVER_URL ?? `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`

/**
 * The server's messages as `ServerMessage`s. The types come from Rust
 * (crates/protocol), so they match what the server sends.
 */
function parse(data: unknown): ServerMessage {
  try {
    return JSON.parse(String(data)) as ServerMessage
  } catch {
    return { type: 'error', message: `unreadable message from the server: ${String(data).slice(0, 80)}` }
  }
}

/** A connection to the match server and the messages exchanged on it. */
export function useServer() {
  const [status, setStatus] = useState<Status>('connecting')
  const [entries, setEntries] = useState<Entry[]>([])
  // Set when the server speaks another protocol version than this client.
  const [mismatch, setMismatch] = useState<number | null>(null)
  // Bumped to connect again.
  const [attempt, setAttempt] = useState(0)
  const socket = useRef<WebSocket | null>(null)
  const nextId = useRef(0)

  const add = useCallback((entry: { sent: ClientMessage } | { received: ServerMessage }) => {
    setEntries((entries) => [...entries.slice(-(KEEP - 1)), { id: nextId.current++, ...entry }])
  }, [])

  useEffect(() => {
    const ws = new WebSocket(SERVER_URL)
    socket.current = ws
    ws.onopen = () => setStatus('open')
    ws.onmessage = (e) => {
      const message = parse(e.data)
      if (message.type === 'welcome') setMismatch(message.protocol === protocolVersion() ? null : message.protocol)
      add({ received: message })
    }
    ws.onclose = () => setStatus('closed')
    return () => {
      ws.onclose = null
      ws.close()
    }
  }, [attempt, add])

  const send = useCallback(
    (message: ClientMessage) => {
      if (socket.current?.readyState !== WebSocket.OPEN) return
      socket.current.send(JSON.stringify(message))
      add({ sent: message })
    },
    [add],
  )

  const reconnect = useCallback(() => {
    setStatus('connecting')
    setAttempt((n) => n + 1)
  }, [])

  return { status, entries, mismatch, send, reconnect }
}

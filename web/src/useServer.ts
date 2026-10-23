import { useCallback, useEffect, useRef, useState } from 'react'

export type Status = 'connecting' | 'open' | 'closed'

export interface Line {
  id: number
  direction: 'sent' | 'received'
  text: string
}

/** Lines of conversation kept. */
const KEEP = 50

/**
 * The match server's WebSocket. In development Vite proxies /ws on the
 * page's own origin to it (see vite.config.ts); VITE_SERVER_URL overrides.
 */
export const SERVER_URL =
  import.meta.env.VITE_SERVER_URL ?? `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`

/** A connection to the match server and the messages exchanged on it. */
export function useServer() {
  const [status, setStatus] = useState<Status>('connecting')
  const [lines, setLines] = useState<Line[]>([])
  // Bumped to connect again.
  const [attempt, setAttempt] = useState(0)
  const socket = useRef<WebSocket | null>(null)
  const nextId = useRef(0)

  const add = useCallback((direction: Line['direction'], text: string) => {
    const line = { id: nextId.current++, direction, text }
    setLines((lines) => [...lines.slice(-(KEEP - 1)), line])
  }, [])

  useEffect(() => {
    const ws = new WebSocket(SERVER_URL)
    socket.current = ws
    ws.onopen = () => setStatus('open')
    ws.onmessage = (e) => add('received', String(e.data))
    ws.onclose = () => setStatus('closed')
    return () => {
      ws.onclose = null
      ws.close()
    }
  }, [attempt, add])

  const send = useCallback(
    (text: string) => {
      if (socket.current?.readyState !== WebSocket.OPEN) return
      socket.current.send(text)
      add('sent', text)
    },
    [add],
  )

  const reconnect = useCallback(() => {
    setStatus('connecting')
    setAttempt((n) => n + 1)
  }, [])

  return { status, lines, send, reconnect }
}

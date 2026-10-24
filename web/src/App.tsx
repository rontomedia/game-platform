import { useCallback, useState, type FormEvent } from 'react'

import { emptyArrangement, type Arrangement, type RackSort } from './arrange.ts'
import { Board } from './Board.tsx'
import { demoGame, demoRoom } from './demo.ts'
import { useSettings } from './settings.ts'
import { remember, remembered } from './storage.ts'
import { Button, inputClass, Note, Panel } from './ui.tsx'
import { SERVER_URL, useGame, type Status } from './useGame.ts'
import { protocolVersion, type ClientMessage, type RoomInfo } from './wasm/engine.js'

const statusLook: Record<Status, { label: string; dot: string }> = {
  connecting: { label: 'Connecting', dot: 'bg-amber-400 animate-pulse' },
  open: { label: 'Connected', dot: 'bg-emerald-400' },
  closed: { label: 'Disconnected', dot: 'bg-rose-500' },
}

/** `?demo`: the board with a made-up game and no server, for working on its look. */
const DEMO = new URLSearchParams(location.search).has('demo')

export default function App() {
  return DEMO ? <Demo /> : <Online />
}

function Demo() {
  const [game] = useState(demoGame)
  const [settings, changeSettings] = useSettings()
  const [arrangement, setArrangement] = useArrangement(demoRoom.code)
  return (
    <Board
      game={game}
      room={demoRoom}
      status="open"
      problem={null}
      settings={settings}
      onSettings={changeSettings}
      arrangement={arrangement}
      onArrange={setArrangement}
      send={(message) => console.info('demo, not sent:', message)}
      leave={() => location.reload()}
    />
  )
}

/** Remembered across visits: how the player last ordered their rack. */
const SORT_KEY = 'rummy.rackSort'
/** Remembered for the tab: the player's arrangement in their current room. */
const ARRANGEMENT_KEY = 'rummy.arrangement'

/**
 * The player's arrangement of rack and table in room `code`, kept across
 * turns and reloads (see arrange.ts).
 */
function useArrangement(code: string | undefined): [Arrangement, (arrangement: Arrangement) => void] {
  const [saved, setSaved] = useState(() => remembered<{ code: string; arrangement: Arrangement }>(ARRANGEMENT_KEY, 'session'))
  const arrangement =
    saved && saved.code === code ? saved.arrangement : { ...emptyArrangement, sort: remembered<RackSort>(SORT_KEY) ?? 'custom' }
  // Stable, as the board saves it in an effect.
  const save = useCallback(
    (arrangement: Arrangement) => {
      if (code === undefined) return
      const next = { code, arrangement }
      remember(ARRANGEMENT_KEY, next, 'session')
      remember(SORT_KEY, arrangement.sort)
      setSaved(next)
    },
    [code],
  )
  return [arrangement, save]
}

function Online() {
  const { status, room, game, problem, mismatch, send, reconnect, leave } = useGame()
  const [settings, changeSettings] = useSettings()
  const [arrangement, setArrangement] = useArrangement(room?.code)
  const look = statusLook[status]

  if (room?.started && game) {
    return (
      <Board
        key={game.version}
        game={game}
        room={room}
        status={status}
        problem={problem}
        settings={settings}
        onSettings={changeSettings}
        arrangement={arrangement}
        onArrange={setArrangement}
        send={send}
        leave={leave}
      />
    )
  }

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6 sm:px-6 lg:py-10">
      <header className="flex items-center justify-between gap-4">
        <h1 className="text-3xl font-bold tracking-tight">Rummy</h1>
        <span className="flex items-center gap-2 text-sm text-slate-300" title={SERVER_URL}>
          <span className={`size-2.5 rounded-full ${look.dot}`} />
          {look.label}
          {status === 'closed' && (
            <Button look="quiet" className="px-2 py-1 text-sm" onClick={reconnect}>
              Reconnect
            </Button>
          )}
        </span>
      </header>
      {mismatch !== null && (
        <p className="rounded-xl bg-rose-500/15 px-3 py-2 text-sm text-rose-200">
          The server speaks protocol {mismatch}, this page {protocolVersion()}. Reload to get the current version.
        </p>
      )}
      {!room ? (
        <Home send={send} problem={problem} ready={status === 'open'} />
      ) : (
        <Lobby room={room} problem={problem} send={send} leave={leave} />
      )}
    </div>
  )
}

/** The player's name, remembered on this device. */
const NAME_KEY = 'rummy.name'

function Home({ send, problem, ready }: { send: (m: ClientMessage) => void; problem: string | null; ready: boolean }) {
  const [name, setName] = useState(() => remembered<string>(NAME_KEY) ?? '')
  const [code, setCode] = useState('')

  const create = () => {
    remember(NAME_KEY, name.trim())
    send({ type: 'createRoom', name })
  }
  const join = (e: FormEvent) => {
    e.preventDefault()
    remember(NAME_KEY, name.trim())
    send({ type: 'joinRoom', code, name })
  }

  return (
    <div className="mx-auto w-full max-w-md">
      <Panel>
        <label className="flex flex-col gap-2">
          <span className="text-sm font-medium text-slate-300">Your name</span>
          <input value={name} onChange={(e) => setName(e.target.value)} maxLength={20} placeholder="Name" className={inputClass} />
        </label>
        <Button look="primary" className="mt-5 w-full" disabled={!ready} onClick={create}>
          Create a room
        </Button>
        <div className="my-5 flex items-center gap-3 text-sm text-slate-500">
          <span className="h-px flex-1 bg-white/10" />
          or join one
          <span className="h-px flex-1 bg-white/10" />
        </div>
        <form onSubmit={join} className="flex gap-2">
          <input
            value={code}
            onChange={(e) => setCode(e.target.value.toUpperCase())}
            maxLength={4}
            placeholder="CODE"
            aria-label="Room code"
            autoCapitalize="characters"
            autoComplete="off"
            className={`${inputClass} flex-1 text-center font-mono text-lg tracking-[0.3em]`}
          />
          <Button disabled={!ready || code.trim().length !== 4}>Join</Button>
        </form>
        {problem && (
          <div className="mt-4">
            <Note tone="bad">{problem}</Note>
          </div>
        )}
      </Panel>
    </div>
  )
}

function Lobby({
  room,
  problem,
  send,
  leave,
}: {
  room: RoomInfo
  problem: string | null
  send: (m: ClientMessage) => void
  leave: () => void
}) {
  const present = room.seats.filter((s) => s.connected).length
  const isHost = room.you === room.host
  return (
    <div className="mx-auto w-full max-w-md">
      <Panel>
        <p className="text-sm text-slate-400">Room code: share it with the other players</p>
        <p className="mt-1 font-mono text-5xl font-bold tracking-[0.2em] text-teal-300">{room.code}</p>
        <ul className="mt-6 space-y-2">
          {room.seats.map((seat, i) => (
            <li key={i} className="flex items-center gap-3 rounded-xl bg-white/5 px-3 py-2">
              <span className={`size-2 rounded-full ${seat.connected ? 'bg-emerald-400' : 'bg-slate-500'}`} />
              <span className="flex-1">
                {seat.name}
                {i === room.you && <span className="text-slate-400"> (you)</span>}
              </span>
              {i === room.host && <span className="text-xs uppercase tracking-wide text-teal-300">host</span>}
            </li>
          ))}
        </ul>
        <div className="mt-6 flex gap-2">
          <Button onClick={leave}>Leave</Button>
          {isHost ? (
            <Button look="primary" className="flex-1" disabled={present < 2} onClick={() => send({ type: 'start' })}>
              {present < 2 ? 'Waiting for players…' : `Start with ${present} players`}
            </Button>
          ) : (
            <p className="flex-1 self-center text-center text-sm text-slate-400">Waiting for the host to start…</p>
          )}
        </div>
        {problem && (
          <div className="mt-4">
            <Note tone="bad">{problem}</Note>
          </div>
        )}
      </Panel>
    </div>
  )
}

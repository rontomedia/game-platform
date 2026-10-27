import { useCallback, useEffect, useState, type FormEvent } from 'react'

import { emptyArrangement, type Arrangement, type RackSort } from './arrange.ts'
import { Board } from './Board.tsx'
import { demoDrafts, demoGame, demoRoom, withOpenRacks } from './demo.ts'
import { useSettings } from './settings.ts'
import { remember, remembered } from './storage.ts'
import { Button, inputClass, Note, Panel, Switch } from './ui.tsx'
import { SERVER_URL, useGame, type Draft, type Status } from './useGame.ts'
import { protocolVersion, type BotLevel, type ClientMessage, type RoomInfo, type RoomOptions } from './wasm/engine.js'

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

/**
 * The demo game. `?demo=watch`: it's Maya's turn instead, and she shares it
 * live, a step every few seconds. `?demo=racks`: every rack is shown, as
 * with a server started with `--open-racks`.
 */
function Demo() {
  const mode = new URLSearchParams(location.search).get('demo')
  const watch = mode === 'watch'
  const [game] = useState(() => (mode === 'racks' ? withOpenRacks(demoGame()) : demoGame(watch)))
  const [settings, changeSettings] = useSettings()
  const [arrangement, setArrangement] = useArrangement(demoRoom.code)
  const [step, setStep] = useState(0)
  useEffect(() => {
    if (!watch) return
    const timer = setInterval(() => setStep((step) => step + 1), 2500)
    return () => clearInterval(timer)
  }, [watch])
  // Made once: a new table each render would look like a new draft each time.
  const [drafts] = useState(demoDrafts)
  const table = watch ? drafts[step % drafts.length] : null
  const draft: Draft | null = table && { by: game.view.current, table, version: game.version }
  return (
    <Board
      game={game}
      room={demoRoom}
      status="open"
      problem={null}
      notice={null}
      settings={settings}
      onSettings={changeSettings}
      arrangement={arrangement}
      onArrange={setArrangement}
      draft={settings.watchLive ? draft : null}
      send={(message) => console.info('demo, not sent:', message)}
      leave={() => location.reload()}
    />
  )
}

/** Remembered across visits: how the player last ordered their rack. */
const SORT_KEY = 'rummy.rackSort'
/** Remembered for the tab: the player's arrangement in their current room. */
const ARRANGEMENT_KEY = 'rummy.arrangement.v2'

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
  const { status, room, game, draft, problem, notice, mismatch, send, reconnect, leave } = useGame()
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
        notice={notice}
        settings={settings}
        onSettings={changeSettings}
        arrangement={arrangement}
        onArrange={setArrangement}
        draft={settings.watchLive ? draft : null}
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

  const [options, setOptions] = useState<Required<RoomOptions>>(() => ({
    ...defaultOptions,
    ...remembered<RoomOptions>(OPTIONS_KEY),
  }))
  const change = (change: RoomOptions) => {
    const next = { ...options, ...change }
    remember(OPTIONS_KEY, next)
    setOptions(next)
  }

  const create = () => {
    remember(NAME_KEY, name.trim())
    send({ type: 'createRoom', name, options })
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
        <div className="mt-5">
          <span className="block text-sm font-medium text-slate-300">Time for a turn</span>
          <div className="mt-2 flex rounded-xl bg-black/30 p-1 text-sm font-semibold" role="group" aria-label="Time for a turn">
            {TURN_LENGTHS.map(([seconds, label]) => (
              <button
                key={seconds}
                onClick={() => change({ turnSeconds: seconds })}
                aria-pressed={options.turnSeconds === seconds}
                className={`flex-1 cursor-pointer rounded-lg py-1.5 transition ${
                  options.turnSeconds === seconds ? 'bg-teal-500 text-slate-950' : 'text-slate-300 hover:bg-white/10'
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
        <Option
          title="Time bank"
          detail="Each player also has 2 minutes for the whole game, used once a turn’s time runs out. Time left over from a turn is added, up to 3 minutes."
          checked={options.timeBank}
          onChange={(timeBank) => change({ timeBank })}
        />
        <Option
          title="Extra time for tiles"
          detail="15 s for each tile put down and 20 s for each set it makes or changes, once the table is a valid turn."
          checked={options.tileBonus}
          onChange={(tileBonus) => change({ tileBonus })}
        />
        <Option
          title="Show turns live"
          detail="Everyone sees the current player’s moves as they make them, as at a real table."
          checked={options.liveTurns}
          onChange={(liveTurns) => change({ liveTurns })}
        />
        <Button look="primary" className="mt-4 w-full" disabled={!ready} onClick={create}>
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

/** The room options to start from, as the server has them. */
const defaultOptions: Required<RoomOptions> = { turnSeconds: 60, liveTurns: true, timeBank: false, tileBonus: false }
/** Remembered across visits: the options the player last created a room with. */
const OPTIONS_KEY = 'rummy.roomOptions'
/** The turn lengths a room may have (the server's `TURN_SECONDS`). */
const TURN_LENGTHS: [number, string][] = [
  [30, '30 s'],
  [60, '1 min'],
  [90, '90 s'],
  [120, '2 min'],
  [180, '3 min'],
]

/** The room's rules in a sentence, for the lobby. */
function describe(options: RoomOptions): string {
  const turn = TURN_LENGTHS.find(([seconds]) => seconds === (options.turnSeconds ?? 60))?.[1] ?? '1 min'
  const parts = [`${turn} a turn`]
  if (options.timeBank) parts.push('a 2-minute time bank')
  if (options.tileBonus) parts.push('extra time for tiles')
  parts.push(options.liveTurns ? 'turns shown live' : 'only finished turns shown')
  return `${parts.join(', ')}.`.replace(/^./, (c) => c.toUpperCase())
}

function Option({
  title,
  detail,
  checked,
  onChange,
}: {
  title: string
  detail: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label className="mt-5 flex cursor-pointer items-start gap-3">
      <span className="flex-1">
        <span className="block text-sm font-medium text-slate-300">{title}</span>
        <span className="mt-0.5 block text-xs text-slate-500">{detail}</span>
      </span>
      <Switch checked={checked} onChange={onChange} />
    </label>
  )
}

/** Seats in a room (the server's limit too). */
const MAX_PLAYERS = 4

const botLabel: Record<BotLevel, string> = { easy: 'Easy bot', medium: 'Medium bot', hard: 'Hard bot' }

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
  const full = room.seats.length >= MAX_PLAYERS
  return (
    <div className="mx-auto w-full max-w-md">
      <Panel>
        <p className="text-sm text-slate-400">Room code: share it with the other players</p>
        <p className="mt-1 font-mono text-5xl font-bold tracking-[0.2em] text-teal-300">{room.code}</p>
        <p className="mt-3 text-sm text-slate-400">{describe(room.options)}</p>
        <ul className="mt-6 space-y-2">
          {room.seats.map((seat, i) => (
            <li key={i} className="flex items-center gap-3 rounded-xl bg-white/5 px-3 py-2">
              <span className={`size-2 rounded-full ${seat.connected ? 'bg-emerald-400' : 'bg-slate-500'}`} />
              <span className="flex-1">
                {seat.name}
                {i === room.you && <span className="text-slate-400"> (you)</span>}
              </span>
              {seat.bot && <span className="text-xs uppercase tracking-wide text-violet-300">{botLabel[seat.bot]}</span>}
              {seat.bot && isHost && (
                <button
                  className="-my-1 -mr-1 grid size-7 cursor-pointer place-items-center rounded-lg text-slate-400 hover:bg-white/10 hover:text-slate-100"
                  aria-label={`Remove ${seat.name}`}
                  title="Remove"
                  onClick={() => send({ type: 'removeBot', seat: i })}
                >
                  ×
                </button>
              )}
              {i === room.host && <span className="text-xs uppercase tracking-wide text-teal-300">host</span>}
            </li>
          ))}
        </ul>
        {isHost && !full && (
          <div className="mt-3 flex items-center gap-2 text-sm text-slate-400">
            <span className="flex-1">Add a computer player:</span>
            {(['easy', 'medium', 'hard'] as const).map((level) => (
              <Button key={level} look="quiet" className="px-3 py-1.5 text-sm" onClick={() => send({ type: 'addBot', level })}>
                + {botLabel[level]}
              </Button>
            ))}
          </div>
        )}
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

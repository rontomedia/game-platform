import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useRef, useState, type ReactNode } from 'react'

import { TileView } from './TileView.tsx'
import { SERVER_URL, useServer, type Entry, type Status } from './useServer.ts'
import { classifySet, protocolVersion, setPoints, type Tile } from './wasm/engine.js'

/** A tile with an identity, so the same tile can move between places. */
interface Placed {
  id: number
  tile: Tile
}

const RACK: Placed[] = (
  [
    { kind: 'number', color: 'red', value: 3 },
    { kind: 'number', color: 'red', value: 4 },
    { kind: 'number', color: 'red', value: 5 },
    { kind: 'number', color: 'blue', value: 7 },
    { kind: 'number', color: 'black', value: 7 },
    { kind: 'number', color: 'orange', value: 7 },
    { kind: 'number', color: 'red', value: 7 },
    { kind: 'joker' },
    { kind: 'number', color: 'blue', value: 12 },
    { kind: 'number', color: 'blue', value: 13 },
  ] satisfies Tile[]
).map((tile, id) => ({ id, tile }))

export default function App() {
  const [rack, setRack] = useState(RACK)
  const [set, setSet] = useState<Placed[]>([])

  const toSet = (placed: Placed) => {
    setRack((rack) => rack.filter((p) => p.id !== placed.id))
    setSet((set) => [...set, placed])
  }
  const toRack = (placed: Placed) => {
    setSet((set) => set.filter((p) => p.id !== placed.id))
    setRack((rack) => [...rack, placed].sort((a, b) => a.id - b.id))
  }

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-6 px-4 py-8 sm:px-6 lg:py-12">
      <header>
        <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">Rummy</h1>
        <p className="mt-1 text-slate-400">The Rust rules engine in the browser, and typed messages with the match server.</p>
      </header>
      <div className="grid gap-6 lg:grid-cols-[3fr_2fr]">
        <SetBuilder rack={rack} set={set} toSet={toSet} toRack={toRack} />
        <ServerPanel table={set.length ? [set.map((p) => p.tile)] : []} />
      </div>
    </div>
  )
}

function Panel({ title, aside, children }: { title: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <section className="rounded-2xl border border-white/10 bg-white/5 p-5 shadow-2xl backdrop-blur-xl sm:p-6">
      <div className="mb-4 flex items-center justify-between gap-4">
        <h2 className="text-lg font-semibold">{title}</h2>
        {aside}
      </div>
      {children}
    </section>
  )
}

/** Build a set from the rack; the engine says whether it's valid. */
function SetBuilder({
  rack,
  set,
  toSet,
  toRack,
}: {
  rack: Placed[]
  set: Placed[]
  toSet: (placed: Placed) => void
  toRack: (placed: Placed) => void
}) {
  const tiles = set.map((p) => p.tile)
  const kind = classifySet(tiles)
  const points = setPoints(tiles)

  const verdict =
    set.length === 0
      ? { text: 'Tap tiles to build a set', tone: 'text-slate-400' }
      : kind
        ? { text: `Valid ${kind}, ${points} points`, tone: 'text-emerald-300' }
        : { text: set.length < 3 ? 'A set needs 3 or more tiles' : 'Not a valid set', tone: 'text-amber-300' }

  return (
    <Panel title="Set builder" aside={<span className={`text-sm font-medium ${verdict.tone}`}>{verdict.text}</span>}>
      <div
        className={`flex min-h-28 flex-wrap items-center gap-2 rounded-xl border-2 border-dashed p-3 transition-colors sm:min-h-32 ${
          kind ? 'border-emerald-400/60 bg-emerald-400/10' : 'border-white/15 bg-black/20'
        }`}
      >
        {set.map((p) => (
          <TileView key={p.id} layoutId={`tile-${p.id}`} tile={p.tile} onClick={() => toRack(p)} />
        ))}
      </div>
      <div className="mt-4 flex min-h-24 flex-wrap gap-2 rounded-xl bg-linear-to-b from-amber-900/60 to-amber-950/80 p-3 shadow-inner ring-1 ring-amber-200/10 sm:min-h-28">
        {rack.map((p) => (
          <TileView key={p.id} layoutId={`tile-${p.id}`} tile={p.tile} onClick={() => toSet(p)} />
        ))}
      </div>
      <p className="mt-3 text-sm text-slate-400">
        The check runs in Rust (<code className="text-slate-300">crates/engine</code>) compiled to WebAssembly: the same
        code the server uses.
      </p>
    </Panel>
  )
}

const statusLook: Record<Status, { label: string; dot: string }> = {
  connecting: { label: 'Connecting', dot: 'bg-amber-400 animate-pulse' },
  open: { label: 'Connected', dot: 'bg-emerald-400' },
  closed: { label: 'Disconnected', dot: 'bg-rose-500' },
}

/** One line of the conversation; the full message is in its tooltip. */
function describe(entry: Entry): string {
  if ('sent' in entry) {
    const m = entry.sent
    if (m.type === 'draw') return 'draw'
    const sets = m.table.length
    return `play ${m.table.flat().length} tiles in ${sets} set${sets === 1 ? '' : 's'}`
  }
  const m = entry.received
  switch (m.type) {
    case 'welcome':
      return `welcome, protocol ${m.protocol}`
    case 'state':
      return `state: ${m.view.rack.length} tiles on your rack`
    case 'rejected':
    case 'error':
      return `${m.type}: ${m.message}`
  }
}

/** Messages with the match server. It hosts no matches yet, so moves get an error. */
function ServerPanel({ table }: { table: Tile[][] }) {
  const { status, entries, mismatch, send, reconnect } = useServer()
  const log = useRef<HTMLOListElement>(null)

  useEffect(() => {
    log.current?.scrollTo({ top: log.current.scrollHeight, behavior: 'smooth' })
  }, [entries])

  const look = statusLook[status]
  const button =
    'flex-1 cursor-pointer rounded-xl px-4 py-2.5 font-semibold transition disabled:cursor-not-allowed disabled:opacity-40'
  return (
    <Panel
      title="Match server"
      aside={
        <span className="flex items-center gap-2 text-sm text-slate-300" title={SERVER_URL}>
          <span className={`size-2.5 rounded-full ${look.dot}`} />
          {look.label}
        </span>
      }
    >
      {mismatch !== null && (
        <p className="mb-3 rounded-xl bg-rose-500/15 px-3 py-2 text-sm text-rose-200">
          The server speaks protocol {mismatch}, this page {protocolVersion()}. Reload to get the current version.
        </p>
      )}
      <ol ref={log} className="h-64 space-y-1.5 overflow-y-auto rounded-xl bg-black/30 p-3 font-mono text-sm lg:h-72">
        <AnimatePresence initial={false}>
          {entries.map((entry) => {
            const sent = 'sent' in entry
            const failed = !sent && (entry.received.type === 'error' || entry.received.type === 'rejected')
            return (
              <motion.li
                key={entry.id}
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                title={JSON.stringify(sent ? entry.sent : entry.received)}
                className={sent ? 'text-sky-300' : failed ? 'text-amber-300' : 'text-emerald-300'}
              >
                <span className="mr-2 text-slate-500">{sent ? '→' : '←'}</span>
                {describe(entry)}
              </motion.li>
            )
          })}
        </AnimatePresence>
      </ol>
      {status === 'closed' ? (
        <button onClick={reconnect} className={`mt-3 w-full bg-white/10 hover:bg-white/15 ${button}`}>
          Reconnect
        </button>
      ) : (
        <div className="mt-3 flex gap-2">
          <button
            onClick={() => send({ type: 'play', table })}
            disabled={status !== 'open' || table.length === 0}
            className={`bg-teal-500 text-slate-950 hover:bg-teal-400 ${button}`}
          >
            Play the set
          </button>
          <button
            onClick={() => send({ type: 'draw' })}
            disabled={status !== 'open'}
            className={`bg-white/10 hover:bg-white/15 ${button}`}
          >
            Draw
          </button>
        </div>
      )}
    </Panel>
  )
}

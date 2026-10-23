import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'

import { TileView } from './TileView.tsx'
import { SERVER_URL, useServer, type Status } from './useServer.ts'
import { classifySet, type Tile } from './wasm/engine.js'

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
  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-6 px-4 py-8 sm:px-6 lg:py-12">
      <header>
        <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">Rummy</h1>
        <p className="mt-1 text-slate-400">First version: the Rust rules engine in the browser, and the match server.</p>
      </header>
      <div className="grid gap-6 lg:grid-cols-[3fr_2fr]">
        <SetBuilder />
        <ServerPanel />
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
function SetBuilder() {
  const [rack, setRack] = useState(RACK)
  const [set, setSet] = useState<Placed[]>([])
  const kind = classifySet(set.map((p) => p.tile))

  const toSet = (placed: Placed) => {
    setRack((rack) => rack.filter((p) => p.id !== placed.id))
    setSet((set) => [...set, placed])
  }
  const toRack = (placed: Placed) => {
    setSet((set) => set.filter((p) => p.id !== placed.id))
    setRack((rack) => [...rack, placed].sort((a, b) => a.id - b.id))
  }

  const verdict =
    set.length === 0
      ? { text: 'Tap tiles to build a set', tone: 'text-slate-400' }
      : kind
        ? { text: kind === 'run' ? 'Valid run' : 'Valid group', tone: 'text-emerald-300' }
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
        code the server will use.
      </p>
    </Panel>
  )
}

const statusLook: Record<Status, { label: string; dot: string }> = {
  connecting: { label: 'Connecting', dot: 'bg-amber-400 animate-pulse' },
  open: { label: 'Connected', dot: 'bg-emerald-400' },
  closed: { label: 'Disconnected', dot: 'bg-rose-500' },
}

/** Messages with the match server: it echoes, and ticks unasked. */
function ServerPanel() {
  const { status, lines, send, reconnect } = useServer()
  const [draft, setDraft] = useState('')
  const log = useRef<HTMLOListElement>(null)

  useEffect(() => {
    log.current?.scrollTo({ top: log.current.scrollHeight, behavior: 'smooth' })
  }, [lines])

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!draft.trim()) return
    send(draft.trim())
    setDraft('')
  }

  const look = statusLook[status]
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
      <ol ref={log} className="h-64 space-y-1.5 overflow-y-auto rounded-xl bg-black/30 p-3 font-mono text-sm lg:h-72">
        <AnimatePresence initial={false}>
          {lines.map((line) => (
            <motion.li
              key={line.id}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              className={line.direction === 'sent' ? 'text-sky-300' : 'text-emerald-300'}
            >
              <span className="mr-2 text-slate-500">{line.direction === 'sent' ? '→' : '←'}</span>
              {line.text}
            </motion.li>
          ))}
        </AnimatePresence>
      </ol>
      {status === 'closed' ? (
        <button
          onClick={reconnect}
          className="mt-3 w-full cursor-pointer rounded-xl bg-white/10 px-4 py-2.5 font-medium transition hover:bg-white/15"
        >
          Reconnect
        </button>
      ) : (
        <form onSubmit={submit} className="mt-3 flex gap-2">
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Say something to the server"
            aria-label="Message"
            className="min-w-0 flex-1 rounded-xl border border-white/10 bg-black/30 px-3 py-2.5 placeholder:text-slate-500 focus:border-teal-400/60 focus:outline-none"
          />
          <button
            disabled={status !== 'open' || !draft.trim()}
            className="cursor-pointer rounded-xl bg-teal-500 px-4 py-2.5 font-semibold text-slate-950 transition hover:bg-teal-400 disabled:cursor-not-allowed disabled:opacity-40"
          >
            Send
          </button>
        </form>
      )}
    </Panel>
  )
}

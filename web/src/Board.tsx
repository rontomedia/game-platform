import { useEffect, useState } from 'react'

import { TileView } from './TileView.tsx'
import { Button, Note, Panel } from './ui.tsx'
import type { GameState } from './useGame.ts'
import { checkTurn, classifySet, type ClientMessage, type RoomInfo, type Tile } from './wasm/engine.js'

/**
 * The game, playable but plain: tap tiles from the rack or the table to pick
 * them up, then put them down as a new set or at either end of a set. The
 * real board (dragging, polish) replaces this.
 */

/** A tile with an identity, so the same tile can move between places. */
interface Placed {
  id: number
  tile: Tile
  fromTable: boolean
}

function place(view: GameState['view']) {
  let id = 0
  const table = view.table.map((set) => set.map((tile) => ({ id: id++, tile, fromTable: true })))
  const rack = view.rack.map((tile) => ({ id: id++, tile, fromTable: false }))
  return { table, rack }
}

const tilesOf = (placed: Placed[]) => placed.map((p) => p.tile)

/** Seconds until `ends`, ticking. */
function useSecondsLeft(ends: number | undefined) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 250)
    return () => clearInterval(timer)
  }, [])
  return ends === undefined ? undefined : Math.max(0, Math.ceil((ends - now) / 1000))
}

/** Give it `key={game.version}`: a new state from the server drops unsent changes. */
export function Board({
  game,
  room,
  problem,
  send,
  leave,
}: {
  game: GameState
  room: RoomInfo
  problem: string | null
  send: (message: ClientMessage) => void
  leave: () => void
}) {
  const { view } = game
  const [start] = useState(() => place(view))
  const [table, setTable] = useState(start.table)
  const [rack, setRack] = useState(start.rack)
  const [held, setHeld] = useState<Placed[]>([])
  const secondsLeft = useSecondsLeft(game.turnEnds)

  const myTurn = view.current === view.you && !view.outcome
  const after = table.map(tilesOf)
  const changed = held.length > 0 || JSON.stringify(after) !== JSON.stringify(view.table)
  const verdict = myTurn && changed && held.length === 0
    ? checkTurn({ before: view.table, after, rack: view.rack, melded: view.melded[view.you], rules: view.rules })
    : null

  const reset = () => {
    setTable(start.table)
    setRack(start.rack)
    setHeld([])
  }
  const pickFromRack = (p: Placed) => {
    setRack((rack) => rack.filter((r) => r.id !== p.id))
    setHeld((held) => [...held, p])
  }
  const pickFromTable = (p: Placed) => {
    setTable((table) => table.map((set) => set.filter((t) => t.id !== p.id)).filter((set) => set.length > 0))
    setHeld((held) => [...held, p])
  }
  // Tiles from the table can't go to the rack; Reset undoes everything.
  const putBack = (p: Placed) => {
    if (p.fromTable) return
    setHeld((held) => held.filter((h) => h.id !== p.id))
    setRack((rack) => [...rack, p].sort((a, b) => a.id - b.id))
  }
  const putDown = (where: { newSet: true } | { set: number; atStart: boolean }) => {
    setTable((table) => {
      if ('newSet' in where) return [...table, held]
      return table.map((set, i) =>
        i !== where.set ? set : where.atStart ? [...held, ...set] : [...set, ...held],
      )
    })
    setHeld([])
  }

  const name = (seat: number) => room.seats[seat]?.name ?? `Player ${seat + 1}`
  const status = ((): { tone: 'neutral' | 'good' | 'bad'; text: string } => {
    if (problem) return { tone: 'bad', text: problem }
    if (view.outcome) return { tone: 'neutral', text: 'The game is over.' }
    if (!myTurn) return { tone: 'neutral', text: `${name(view.current)} is playing…` }
    if (held.length > 0) {
      return { tone: 'neutral', text: 'Put the tiles you picked up on the table: as a new set, or at the end of one.' }
    }
    if (verdict?.verdict === 'rejected') return { tone: 'bad', text: verdict.message }
    if (verdict?.verdict === 'allowed') return { tone: 'good', text: `Ready: ${verdict.played.length} tiles from your rack.` }
    if (view.melded[view.you]) return { tone: 'neutral', text: 'Your turn: play, or draw.' }
    return {
      tone: 'neutral',
      text: `Your turn: your first sets must be worth ${view.rules.initialMeld} points, from your rack only. Or draw.`,
    }
  })()

  const layout = (p: Placed) => `${game.version}-${p.id}`

  return (
    <div className="flex flex-col gap-4">
      <Players view={view} room={room} secondsLeft={secondsLeft} />

      {view.outcome && <Results view={view} name={name} leave={leave} />}

      <Panel title="Table" aside={<span className="text-sm text-slate-400">{view.pool} tiles to draw</span>}>
        <div className="flex min-h-24 flex-col gap-3">
          {table.length === 0 && <p className="text-slate-500">No sets yet.</p>}
          {table.map((set, i) => {
            const valid = classifySet(tilesOf(set)) !== undefined
            return (
              <div key={i} className="flex items-center gap-2">
                {held.length > 0 && (
                  <Button look="quiet" className="px-2" onClick={() => putDown({ set: i, atStart: true })} aria-label="Put at the start">
                    ⇤
                  </Button>
                )}
                <div className={`flex flex-wrap gap-1.5 rounded-xl p-2 ring-1 ${valid ? 'ring-white/10' : 'bg-amber-400/10 ring-amber-400/50'}`}>
                  {set.map((p) => (
                    <TileView key={p.id} layoutId={layout(p)} tile={p.tile} onClick={myTurn ? () => pickFromTable(p) : undefined} />
                  ))}
                </div>
                {held.length > 0 && (
                  <Button look="quiet" className="px-2" onClick={() => putDown({ set: i, atStart: false })} aria-label="Put at the end">
                    ⇥
                  </Button>
                )}
              </div>
            )
          })}
        </div>
      </Panel>

      {held.length > 0 && (
        <Panel title="Picked up" aside={<Button look="primary" onClick={() => putDown({ newSet: true })}>New set</Button>}>
          <div className="flex flex-wrap gap-1.5">
            {held.map((p) => (
              <TileView key={p.id} layoutId={layout(p)} tile={p.tile} onClick={() => putBack(p)} />
            ))}
          </div>
        </Panel>
      )}

      <Panel title="Your rack">
        <div className="flex min-h-24 flex-wrap gap-1.5 rounded-xl bg-linear-to-b from-amber-900/60 to-amber-950/80 p-3 shadow-inner ring-1 ring-amber-200/10">
          {rack.map((p) => (
            <TileView key={p.id} layoutId={layout(p)} tile={p.tile} onClick={myTurn ? () => pickFromRack(p) : undefined} />
          ))}
        </div>
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
          <Note tone={status.tone}>{status.text}</Note>
          {myTurn && (
            <div className="flex gap-2">
              <Button onClick={reset} disabled={!changed}>Reset</Button>
              <Button onClick={() => send({ type: 'draw' })}>Draw</Button>
              <Button look="primary" disabled={verdict?.verdict !== 'allowed'} onClick={() => send({ type: 'play', table: after })}>
                End turn
              </Button>
            </div>
          )}
        </div>
      </Panel>
    </div>
  )
}

function Players({ view, room, secondsLeft }: { view: GameState['view']; room: RoomInfo; secondsLeft?: number }) {
  return (
    <div className="flex flex-wrap gap-2">
      {view.rackSizes.map((size, seat) => {
        const current = seat === view.current && !view.outcome
        const seatInfo = room.seats[seat]
        return (
          <div
            key={seat}
            className={`flex items-center gap-2 rounded-xl px-3 py-2 ring-1 transition ${
              current ? 'bg-teal-400/15 ring-teal-400/60' : 'bg-white/5 ring-white/10'
            }`}
          >
            <span className={`size-2 rounded-full ${seatInfo?.connected ? 'bg-emerald-400' : 'bg-slate-500'}`} />
            <span className="font-medium">
              {seatInfo?.name ?? `Player ${seat + 1}`}
              {seat === view.you && <span className="text-slate-400"> (you)</span>}
            </span>
            <span className="text-sm text-slate-400">{size} tiles</span>
            {current && secondsLeft !== undefined && (
              <span className={`text-sm tabular-nums ${secondsLeft <= 10 ? 'text-amber-300' : 'text-teal-300'}`}>{secondsLeft}s</span>
            )}
          </div>
        )
      })}
    </div>
  )
}

function Results({ view, name, leave }: { view: GameState['view']; name: (seat: number) => string; leave: () => void }) {
  const outcome = view.outcome!
  const order = view.rackSizes.map((_, seat) => seat).sort((a, b) => outcome.scores[b] - outcome.scores[a])
  return (
    <Panel title={outcome.winners.includes(view.you) ? 'You won!' : `${outcome.winners.map(name).join(' and ')} won`}>
      <ol className="space-y-1">
        {order.map((seat) => (
          <li key={seat} className="flex justify-between gap-4">
            <span>{name(seat)}</span>
            <span className={`tabular-nums ${outcome.scores[seat] >= 0 ? 'text-emerald-300' : 'text-rose-300'}`}>
              {outcome.scores[seat] > 0 ? '+' : ''}
              {outcome.scores[seat]}
            </span>
          </li>
        ))}
      </ol>
      <Button className="mt-4" onClick={leave}>Leave the room</Button>
    </Panel>
  )
}

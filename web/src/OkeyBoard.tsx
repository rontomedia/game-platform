import {
  closestCenter,
  DndContext,
  DragOverlay,
  KeyboardSensor,
  MouseSensor,
  pointerWithin,
  TouchSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core'
import { arrayMove, rectSortingStrategy, SortableContext, sortableKeyboardCoordinates, useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { arrangeRack, compareTiles, type RackSort } from './arrange.ts'
import { TopBar, WatchBar, type Clock } from './Board.tsx'
import { noticeText, okeyError, problemText, type Problem } from './messages.ts'
import type { Settings } from './settings.ts'
import { remember, remembered } from './storage.ts'
import { TileFace } from './TileView.tsx'
import { Button } from './ui.tsx'
import type { Draft, OkeyGame, Status } from './useGame.ts'
import { useNow } from './useNow.ts'
import {
  checkOkeyTurn,
  okeyMeldKind,
  okeyProgress,
  type ClientMessage,
  type HandView,
  type Notice,
  type RoomInfo,
  type Tile,
  type Turn,
} from './wasm/engine.js'

/**
 * The Okey 101 board: players along the top; the indicator, the pool and
 * everyone's discards above the table; the rack at the bottom with the
 * spot to discard on. A turn is built here and sent whole when a tile is
 * dropped on the discard spot (see engine okey101::Turn): draw first (the
 * button, or the pool), or drag the previous player's discard in; lay sets
 * on the table or add tiles to the ends of its sets. The table's sets
 * can't be rearranged, so only tiles put down this turn move again.
 */

/** A tile where the board shows it, with an id for dragging. */
interface Placed {
  id: string
  tile: Tile
}

const RACK = 'rack'
const NEW_SET = 'new-set'
const DISCARD = 'discard'
/** The previous player's discard, to take. */
const OFFER = 'offer'
/** The discard once taken, wherever it is. */
const TAKEN = 'taken'
const setTarget = (index: number) => `set-${index}`
const isSetTarget = (id: string) => id.startsWith('set-')

const collisions: CollisionDetection = (args) => {
  const within = pointerWithin(args)
  return within.length > 0 ? within : closestCenter(args)
}

/** Remembered for the tab: the player's rack order in their current room. */
const RACK_KEY = 'rummy.okeyRack'

/** The rack in the player's order: as they last left it, new tiles at the end. */
function orderedRack(code: string, rack: Tile[], sort: RackSort): Placed[] {
  const saved = remembered<{ code: string; tiles: Tile[] }>(RACK_KEY, 'session')
  const previous = saved && saved.code === code ? saved.tiles : []
  const [first, second] = arrangeRack([previous, []], sort, rack)
  return [...first, ...second].map((tile, i) => ({ id: `r${i}`, tile }))
}

const tilesOf = (placed: Placed[]) => placed.map((p) => p.tile)
const same = (a: Tile[][], b: Tile[][]) => JSON.stringify(a) === JSON.stringify(b)

/** Give it `key={okey.version}`: a new state starts the turn afresh. */
export function OkeyBoard({
  okey,
  room,
  status,
  problem,
  notice,
  settings,
  onSettings,
  draft,
  send,
  leave,
}: {
  okey: OkeyGame
  room: RoomInfo
  status: Status
  problem: Problem | null
  notice: Notice | null
  settings: Settings
  onSettings: (change: Partial<Settings>) => void
  draft: Draft | null
  send: (message: ClientMessage) => void
  leave: () => void
}) {
  const { t } = useTranslation()
  const { view } = okey
  const hand = view.hand
  const you = hand.you
  const watching = you === undefined
  const over = view.winners !== undefined
  const myTurn = !watching && hand.current === you && !hand.end && !room.paused
  const players = hand.rackSizes.length
  const previous = (hand.current + players - 1) % players
  const offer = hand.discards[previous]?.at(-1)
  const canTake = myTurn && hand.phase === 'draw' && offer !== undefined && JSON.stringify(offer) !== JSON.stringify(hand.okey)

  const [sort, setSort] = useState<RackSort>(() => remembered<RackSort>('rummy.rackSort') ?? 'custom')
  const [rack, setRack] = useState(() => orderedRack(room.code, hand.rack, sort))
  const [sets, setSets] = useState<Placed[][]>(() =>
    hand.table.map((set, s) => set.map((tile, i) => ({ id: `t${s}-${i}`, tile }))),
  )
  const [taken, setTaken] = useState(false)
  /** A turn refused here, before it was sent. */
  const [refused, setRefused] = useState<Problem | null>(null)
  const [dragging, setDragging] = useState<Placed | null>(null)

  // The rack order outlives the state.
  useEffect(() => {
    remember(RACK_KEY, { code: room.code, tiles: tilesOf(rack.filter((p) => p.id !== TAKEN)) }, 'session')
  }, [rack, room.code])

  const table = sets.map(tilesOf)
  const changed = !same(table, hand.table) || taken
  // Shared after each change on the player's turn, as in the classic game.
  const sent = useRef(JSON.stringify(hand.table))
  useEffect(() => {
    const json = JSON.stringify(table)
    if (!myTurn || json === sent.current) return
    sent.current = json
    send({ type: 'draft', table })
  })

  const canTable = myTurn && (hand.phase === 'play' || taken)
  const fixed = useMemo(() => new Set(hand.table.flatMap((set, s) => set.map((_, i) => `t${s}-${i}`))), [hand.table])

  const turnOf = (discard: Tile): Turn => ({ takeDiscard: taken, table, discard })
  // The table as a turn, whichever tile is discarded: for the note.
  const verdict = canTable && rack.length > 0 ? checkOkeyTurn(hand, turnOf(rack[rack.length - 1].tile)) : undefined
  const progress = myTurn && hand.opened[you!] === 'no' ? okeyProgress({ view: hand, table }) : undefined

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    // A short hold, so a swipe still scrolls.
    useSensor(TouchSensor, { activationConstraint: { delay: 150, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  /** Where a tile is: the rack, a set (and its place), or the offer. */
  const find = (id: string): { in: 'rack'; at: number } | { in: 'set'; set: number; at: number } | { in: 'offer' } | null => {
    if (id === OFFER) return { in: 'offer' }
    const at = rack.findIndex((p) => p.id === id)
    if (at >= 0) return { in: 'rack', at }
    for (let s = 0; s < sets.length; s++) {
      const i = sets[s].findIndex((p) => p.id === id)
      if (i >= 0) return { in: 'set', set: s, at: i }
    }
    return null
  }

  const placedOf = (id: string): Placed | undefined => {
    if (id === OFFER) return offer && { id: TAKEN, tile: offer }
    return rack.find((p) => p.id === id) ?? sets.flat().find((p) => p.id === id)
  }

  /** The board without tile `id`: taken out of the rack or its set (a new set left empty goes). */
  const without = (id: string): { rack: Placed[]; sets: Placed[][] } => ({
    rack: rack.filter((p) => p.id !== id),
    sets: sets
      .map((set) => set.filter((p) => p.id !== id))
      .filter((set, s) => set.length > 0 || s < hand.table.length),
  })

  const discard = (placed: Placed) => {
    const left = without(placed.id)
    const turn: Turn = { takeDiscard: taken, table: left.sets.map(tilesOf), discard: placed.tile }
    const check = checkOkeyTurn(hand, turn)
    if (check.verdict === 'rejected') return setRefused({ kind: 'okey', error: check.error })
    setRack(left.rack)
    setSets(left.sets)
    send({ type: 'discard', ...turn })
  }

  const onDragStart = ({ active }: DragStartEvent) => {
    setRefused(null)
    setDragging(placedOf(String(active.id)) ?? null)
  }

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    setDragging(null)
    if (!over) return
    const id = String(active.id)
    const to = String(over.id)
    const from = find(id)
    const placed = placedOf(id)
    if (!from || !placed) return

    // The discard taken goes back.
    if (to === OFFER) {
      if (id !== TAKEN) return
      const left = without(TAKEN)
      setRack(left.rack)
      setSets(left.sets)
      setTaken(false)
      return
    }
    if (to === DISCARD) {
      if (from.in === 'offer' || !(myTurn && (hand.phase === 'play' || taken))) return
      return discard(placed)
    }
    // Within the rack: reorder.
    const overRack = to === RACK || rack.some((p) => p.id === to)
    if (overRack) {
      if (from.in === 'rack') {
        const target = to === RACK ? rack.length - 1 : rack.findIndex((p) => p.id === to)
        setSort('custom')
        return setRack(arrayMove(rack, from.at, target))
      }
      if (from.in === 'set' && fixed.has(id)) return
      if (from.in === 'offer') setTaken(true)
      const left = from.in === 'offer' ? { rack, sets } : without(id)
      const target = to === RACK ? left.rack.length : left.rack.findIndex((p) => p.id === to)
      const next = [...left.rack]
      next.splice(target < 0 ? next.length : target, 0, placed)
      setRack(next)
      setSets(left.sets)
      return
    }
    // Onto the table: only on the player's turn, and only tiles of this turn.
    if (!myTurn || (from.in === 'set' && fixed.has(id))) return
    if (from.in === 'offer') {
      if (hand.phase !== 'draw') return
    } else if (!canTable) {
      return
    }
    const left = from.in === 'offer' ? { rack, sets } : without(id)
    if (from.in === 'offer') setTaken(true)
    if (to === NEW_SET) {
      setRack(left.rack)
      setSets([...left.sets, [placed]])
      return
    }
    if (!isSetTarget(to)) return
    const s = Number(to.slice('set-'.length))
    const set = left.sets[s]
    if (!set) return
    const tiles = tilesOf(set)
    // At the end it fits, the nearer one if both do; else in an okey's place,
    // taking the okey back to the rack; else at the nearer end, to fix.
    const translated = active.rect.current.translated
    const nearFront = translated !== null && translated.left + translated.width / 2 < over.rect.left + over.rect.width / 2
    const fits = (candidate: Tile[]) => okeyMeldKind(hand.okey, candidate) !== undefined
    const atFront = [placed, ...set]
    const atBack = [...set, placed]
    const okeyAt = tiles.findIndex((tile, i) => {
      if (JSON.stringify(tile) !== JSON.stringify(hand.okey) || !fixed.has(set[i].id)) return false
      return fits(tiles.map((other, j) => (j === i ? placed.tile : other)))
    })
    let nextSet: Placed[]
    let nextRack = left.rack
    if (fits(tilesOf(nearFront ? atFront : atBack))) nextSet = nearFront ? atFront : atBack
    else if (fits(tilesOf(nearFront ? atBack : atFront))) nextSet = nearFront ? atBack : atFront
    else if (okeyAt >= 0 && s < hand.table.length && hand.opened[you!] !== 'no') {
      nextSet = set.map((p, i) => (i === okeyAt ? placed : p))
      nextRack = [...left.rack, { id: `okey-${set[okeyAt].id}`, tile: set[okeyAt].tile }]
    } else nextSet = nearFront ? atFront : atBack
    setRack(nextRack)
    setSets(left.sets.map((other, i) => (i === s ? nextSet : other)))
  }

  const reset = () => {
    setRack(orderedRack(room.code, hand.rack, sort))
    setSets(hand.table.map((set, s) => set.map((tile, i) => ({ id: `t${s}-${i}`, tile }))))
    setTaken(false)
    setRefused(null)
  }

  const sortRack = (by: 'number' | 'color') => {
    setSort(by)
    remember('rummy.rackSort', by)
    setRack([...rack].sort((a, b) => compareTiles(a.tile, b.tile, by)))
  }

  const name = (seat: number) => room.seats[seat]?.name ?? t('board.player', { n: seat + 1 })

  const now = useNow()
  const clock = (seat: number): Clock => {
    const current = seat === hand.current && !hand.end
    const overdue = okey.turnEnds === undefined ? 0 : Math.max(0, now - okey.turnEnds)
    const turn = current && okey.turnEnds !== undefined ? Math.max(0, Math.ceil((okey.turnEnds - now) / 1000)) : undefined
    const bankMs = okey.banks?.[seat]
    const bank = bankMs === undefined ? undefined : Math.max(0, Math.ceil((current ? bankMs - overdue : bankMs) / 1000))
    return { turn, bank }
  }

  // Someone else's turn, shared live: their table so far.
  const liveTable = !myTurn && draft && draft.by === hand.current && !hand.end ? draft.table : null
  const shown: Placed[][] = myTurn
    ? sets
    : (liveTable ?? hand.table).map((set, s) => set.map((tile, i) => ({ id: `v${s}-${i}`, tile })))

  const note = ((): { tone: 'neutral' | 'good' | 'bad'; text: string } => {
    const trouble = refused ?? problem
    if (trouble) return { tone: 'bad', text: problemText(t, trouble) }
    if (notice) return { tone: 'bad', text: noticeText(t, notice) }
    if (over) return { tone: 'neutral', text: t('board.over') }
    if (hand.end) return { tone: 'neutral', text: t('okey.handOver') }
    if (room.paused) return { tone: 'neutral', text: t('board.paused', { name: name(hand.current) }) }
    if (!myTurn) {
      const playing = t('board.playing', { name: name(hand.current) })
      return { tone: 'neutral', text: watching ? t('board.sentences', { first: t('board.youreWatching'), then: playing }) : playing }
    }
    if (hand.phase === 'draw' && !taken) {
      return { tone: 'neutral', text: canTake ? t('okey.yourTurnDrawOrTake', { name: name(previous) }) : t('okey.yourTurnDraw') }
    }
    if (taken && changed && verdict?.verdict === 'rejected' && verdict.error.kind === 'discardNotUsed') {
      return { tone: 'bad', text: t('okey.takenMustBeUsed', { name: name(previous) }) }
    }
    if (changed && verdict?.verdict === 'rejected') return { tone: 'bad', text: okeyError(t, verdict.error) }
    if (progress && (progress.points > 0 || progress.pairs > 0)) {
      return progress.pairs > 0
        ? { tone: 'neutral', text: t('okey.openingPairs', { pairs: progress.pairs, needed: hand.openingPairs }) }
        : { tone: 'neutral', text: t('okey.openingPoints', { points: progress.points, needed: hand.openingPoints }) }
    }
    if (changed && verdict?.verdict === 'allowed') return { tone: 'good', text: t('okey.ready') }
    return { tone: 'neutral', text: hand.opened[you!] === 'no' ? t('okey.yourTurnFirst', { points: hand.openingPoints }) : t('okey.yourTurn') }
  })()
  const noteColor = { neutral: 'text-slate-300', good: 'text-emerald-300', bad: 'text-amber-300' }[note.tone]

  const handNumber = Math.min(hand.rules.hands ?? view.played + 1, view.played + (hand.end ? 0 : 1))

  return (
    <DndContext sensors={sensors} collisionDetection={collisions} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
      <div className="flex h-dvh flex-col gap-3 p-2 sm:gap-4 sm:p-4 short:gap-2 short:p-2">
        <TopBar
          room={room}
          // Between hands nobody is playing.
          view={{ rackSizes: hand.rackSizes, you, current: hand.end ? -1 : hand.current, pool: hand.pool, outcome: over || undefined }}
          clock={clock}
          status={status}
          settings={settings}
          onSettings={onSettings}
          leave={leave}
        />

        <div className="relative flex min-h-0 flex-1 flex-col">
          {/* The playing surface stays left to right in every language: runs read 1 2 3. */}
          <main
            dir="ltr"
            className="felt relative flex min-h-0 flex-1 flex-col gap-3 overflow-auto rounded-3xl p-3 [scrollbar-color:rgb(255_255_255/0.15)_transparent] [scrollbar-width:thin] sm:p-5 short:rounded-2xl short:p-2"
          >
            <div className="flex flex-wrap items-start gap-3">
              <Middle
                hand={hand}
                handNumber={handNumber}
                canDraw={myTurn && hand.phase === 'draw' && !taken}
                onDraw={() => send({ type: 'draw' })}
              />
              <div className="flex min-w-0 flex-1 flex-wrap justify-end gap-2">
                {hand.discards.map((pile, seat) => (
                  <DiscardPile
                    key={seat}
                    name={name(seat)}
                    pile={pile}
                    offered={seat === previous && canTake && !taken}
                    returnable={seat === previous && taken}
                  />
                ))}
              </div>
            </div>

            <div className="flex flex-1 flex-wrap content-start items-start gap-x-4 gap-y-3">
              {shown.length === 0 && !canTable && (
                <p dir="auto" className="w-full py-6 text-center text-emerald-100/40">
                  {t('okey.emptyTable')}
                </p>
              )}
              {shown.map((set, s) => (
                <TableSet
                  key={s}
                  index={s}
                  tiles={set}
                  okey={hand.okey}
                  droppable={canTable || (canTake && !taken)}
                  draggable={myTurn ? (id) => !fixed.has(id) : () => false}
                  fresh={myTurn && (s >= hand.table.length || !same([tilesOf(set)], [hand.table[s]]))}
                  live={liveTable !== null && (s >= hand.table.length || !same([tilesOf(set)], [hand.table[s] ?? []]))}
                />
              ))}
              {(canTable || (canTake && !taken)) && <NewSet />}
            </div>
            <AnimatePresence>
              {hand.end && !over && <HandScores okey={okey} name={name} now={now} />}
              {over && <MatchScores okey={okey} name={name} leave={leave} />}
            </AnimatePresence>
          </main>
        </div>

        {watching ? (
          <WatchBar room={room} over={over} note={note.text} noteColor={noteColor} send={send} />
        ) : (
          <section className={`tray rounded-3xl p-3 sm:p-4 short:rounded-2xl short:p-2 ${myTurn && okey.turnSince === okey.version ? 'turn-cue' : ''}`}>
            <div className="mb-3 flex flex-wrap items-center gap-x-4 gap-y-2 short:mb-2">
              <p className={`min-w-0 flex-1 text-sm font-medium short:truncate ${noteColor}`}>{note.text}</p>
              <div className="flex gap-1 rounded-xl bg-black/30 p-1 text-xs font-semibold" role="group" aria-label={t('board.sort')}>
                {(['number', 'color'] as const).map((by) => (
                  <button
                    key={by}
                    onClick={() => sortRack(by)}
                    aria-pressed={sort === by}
                    className={`cursor-pointer rounded-lg px-2 py-1 ${sort === by ? 'bg-white/15 text-slate-100' : 'text-slate-400 hover:text-slate-200'}`}
                  >
                    {by === 'number' ? t('board.sortNumber') : t('board.sortColor')}
                  </button>
                ))}
              </div>
              <div className={`flex gap-2 ${myTurn ? '' : 'invisible'}`} aria-hidden={!myTurn}>
                <Button look="quiet" onClick={reset} disabled={!myTurn || !changed}>
                  {t('board.reset')}
                </Button>
                <Button onClick={() => send({ type: 'draw' })} disabled={!myTurn || hand.phase !== 'draw' || taken}>
                  {t('board.draw')}
                </Button>
              </div>
            </div>
            <div className="flex items-stretch gap-3">
              <Rack tiles={rack} sortable={!watching} okey={hand.okey} />
              <DiscardSpot active={myTurn && (hand.phase === 'play' || taken)} />
            </div>
          </section>
        )}
      </div>

      <DragOverlay dropAnimation={{ duration: 180, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' }}>
        {dragging ? <TileFace tile={dragging.tile} size="rack" lifted /> : null}
      </DragOverlay>
    </DndContext>
  )
}

/** The indicator, the okey it picks, the pool to draw from, and the hand's number. */
function Middle({ hand, handNumber, canDraw, onDraw }: { hand: HandView; handNumber: number; canDraw: boolean; onDraw: () => void }) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-3 rounded-2xl bg-black/20 p-2 pe-3">
      <Labelled label={t('okey.indicator')}>
        <TileFace tile={hand.indicator} size="table" />
      </Labelled>
      <Labelled label={t('okey.okey')}>
        <div className="rounded-md ring-2 ring-amber-300/80">
          <TileFace tile={hand.okey} size="table" />
        </div>
      </Labelled>
      <Labelled label={t('board.draw')}>
        <button
          onClick={onDraw}
          disabled={!canDraw}
          data-pool
          title={t('board.pool', { count: hand.pool })}
          className={`relative grid h-[3.25rem] w-9 place-items-center rounded-md bg-linear-to-b from-stone-100 to-stone-300 text-sm font-bold tabular-nums text-slate-700 shadow sm:h-16 sm:w-11 short:h-11 short:w-8 ${
            canDraw ? 'cursor-pointer ring-2 ring-teal-300 hover:brightness-105' : 'cursor-default opacity-80'
          }`}
        >
          {hand.pool}
        </button>
      </Labelled>
      <p className="ms-1 text-xs leading-tight text-emerald-100/70">
        {t('okey.hand', { n: handNumber, total: hand.rules.hands ?? handNumber })}
      </p>
    </div>
  )
}

function Labelled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-1">
      {children}
      <span className="max-w-16 truncate text-[0.65rem] uppercase tracking-wide text-emerald-100/60">{label}</span>
    </div>
  )
}

/** A player's discards: the last on top, and how many. The previous player's may be taken. */
function DiscardPile({ name, pile, offered, returnable }: { name: string; pile: Tile[]; offered: boolean; returnable: boolean }) {
  const { t } = useTranslation()
  const top = pile.at(-1)
  const drag = useDraggable({ id: OFFER, disabled: !offered })
  const drop = useDroppable({ id: OFFER, disabled: !returnable })
  const ref = offered ? drag.setNodeRef : returnable ? drop.setNodeRef : undefined
  return (
    <div className="flex flex-col items-center gap-1" title={t('okey.discardsOf', { name })}>
      <div
        ref={ref}
        {...(offered ? { ...drag.attributes, ...drag.listeners } : {})}
        className={`grid h-[3.25rem] w-9 place-items-center rounded-md sm:h-16 sm:w-11 short:h-11 short:w-8 ${
          top && !(offered && drag.isDragging) && !returnable ? '' : 'border border-dashed border-white/20'
        } ${offered ? 'cursor-grab ring-2 ring-teal-300' : ''} ${returnable && drop.isOver ? 'bg-white/10' : ''}`}
        style={offered && drag.transform ? { opacity: 0.3 } : undefined}
      >
        {top && !(offered && drag.isDragging) && !returnable && <TileFace tile={top} size="table" />}
      </div>
      <span className="max-w-16 truncate text-[0.65rem] text-emerald-100/70">
        {name} · {pile.length}
      </span>
    </div>
  )
}

/** A set on the table: tiles may join it at either end; this turn's tiles move again. */
function TableSet({
  index,
  tiles,
  okey,
  droppable,
  draggable,
  fresh,
  live,
}: {
  index: number
  tiles: Placed[]
  okey: Tile
  droppable: boolean
  draggable: (id: string) => boolean
  fresh: boolean
  live: boolean
}) {
  const { setNodeRef, isOver } = useDroppable({ id: setTarget(index), disabled: !droppable })
  const valid = okeyMeldKind(okey, tilesOf(tiles)) !== undefined
  const ring = !valid ? 'ring-2 ring-amber-400' : fresh ? 'ring-2 ring-emerald-300/80' : live ? 'ring-2 ring-sky-300/70 animate-pulse' : ''
  return (
    <div
      ref={setNodeRef}
      className={`flex items-center gap-1 rounded-lg p-1 transition ${ring} ${isOver ? 'bg-white/15' : ''}`}
    >
      {tiles.map((placed) => (
        <TableTile key={placed.id} placed={placed} draggable={draggable(placed.id)} okey={okey} />
      ))}
    </div>
  )
}

function TableTile({ placed, draggable, okey }: { placed: Placed; draggable: boolean; okey: Tile }) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: placed.id, disabled: !draggable })
  const isOkey = JSON.stringify(placed.tile) === JSON.stringify(okey)
  return (
    <div
      ref={setNodeRef}
      {...(draggable ? { ...attributes, ...listeners } : {})}
      className={`${draggable ? 'cursor-grab' : ''} ${isDragging ? 'opacity-30' : ''} ${isOkey ? 'rounded-md ring-2 ring-amber-300/70' : ''}`}
    >
      <TileFace tile={placed.tile} size="table" />
    </div>
  )
}

/** Where a tile dropped starts a new set. */
function NewSet() {
  const { t } = useTranslation()
  const { setNodeRef, isOver } = useDroppable({ id: NEW_SET })
  return (
    <div
      ref={setNodeRef}
      dir="auto"
      className={`grid h-[3.75rem] min-w-32 place-items-center rounded-lg border-2 border-dashed px-3 text-center text-xs text-emerald-100/60 sm:h-[4.5rem] short:h-12 ${
        isOver ? 'border-emerald-200/80 bg-emerald-200/10' : 'border-emerald-100/25'
      }`}
    >
      {t('okey.newSet')}
    </div>
  )
}

/** The rack: one row that wraps, sorted by dragging. */
function Rack({ tiles, sortable, okey }: { tiles: Placed[]; sortable: boolean; okey: Tile }) {
  const { setNodeRef } = useDroppable({ id: RACK })
  return (
    <SortableContext items={tiles.map((p) => p.id)} strategy={rectSortingStrategy} disabled={!sortable}>
      <div ref={setNodeRef} dir="ltr" className="flex min-h-16 min-w-0 flex-1 flex-wrap content-start gap-1.5 sm:min-h-[4.75rem] short:min-h-12">
        {tiles.map((placed) => (
          <RackTile key={placed.id} placed={placed} okey={okey} />
        ))}
      </div>
    </SortableContext>
  )
}

function RackTile({ placed, okey }: { placed: Placed; okey: Tile }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: placed.id })
  const { t } = useTranslation()
  const isOkey = JSON.stringify(placed.tile) === JSON.stringify(okey)
  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      {...attributes}
      {...listeners}
      className={`cursor-grab touch-none rounded-lg ${isDragging ? 'opacity-30' : ''} ${
        placed.id === TAKEN ? 'ring-2 ring-teal-300' : isOkey ? 'ring-2 ring-amber-300/70' : ''
      }`}
      title={placed.id === TAKEN ? t('okey.takenTile') : undefined}
    >
      <TileFace tile={placed.tile} size="rack" />
    </div>
  )
}

/** Where a tile dropped is discarded, ending the turn. */
function DiscardSpot({ active }: { active: boolean }) {
  const { t } = useTranslation()
  const { setNodeRef, isOver } = useDroppable({ id: DISCARD, disabled: !active })
  return (
    <div
      ref={setNodeRef}
      dir="auto"
      className={`grid w-20 shrink-0 place-items-center rounded-2xl border-2 border-dashed p-2 text-center text-xs transition sm:w-28 ${
        active
          ? isOver
            ? 'border-amber-200 bg-amber-200/15 text-amber-100'
            : 'border-amber-200/50 text-amber-100/80'
          : 'border-white/10 text-slate-500'
      }`}
    >
      {t('okey.discardHere')}
    </div>
  )
}

/** A hand's scores, until the next is dealt. */
function HandScores({ okey, name, now }: { okey: OkeyGame; name: (seat: number) => string; now: number }) {
  const { t } = useTranslation()
  const { view } = okey
  const end = view.hand.end!
  const title = end.void
    ? t('okey.void')
    : end.winner === undefined
      ? t('okey.poolRanOut')
      : end.winner === view.hand.you
        ? t('okey.youWentOut')
        : t('okey.wentOut', { name: name(end.winner) })
  const seconds = okey.nextHandAt === undefined ? undefined : Math.max(0, Math.ceil((okey.nextHandAt - now) / 1000))
  return (
    <Overlay>
      <h2 className="text-2xl font-bold">{title}</h2>
      <ScoreTable seats={view.hand.rackSizes.map((_, seat) => seat)} name={name} hand={end.scores} totals={view.totals} you={view.hand.you} />
      <p className="mt-4 text-sm text-slate-400">
        {t('okey.hand', { n: view.played, total: view.hand.rules.hands ?? view.played })}
        {seconds !== undefined && ` · ${t('okey.nextHand', { n: seconds })}`}
      </p>
    </Overlay>
  )
}

/** The match's result: lowest total wins. */
function MatchScores({ okey, name, leave }: { okey: OkeyGame; name: (seat: number) => string; leave: () => void }) {
  const { t, i18n } = useTranslation()
  const { view } = okey
  const winners = view.winners ?? []
  const you = view.hand.you
  const seats = view.totals.map((_, seat) => seat).sort((a, b) => view.totals[a] - view.totals[b])
  return (
    <Overlay>
      <h2 className="text-2xl font-bold">
        {you !== undefined && winners.includes(you)
          ? t('results.youWon')
          : t('results.won', {
              names: new Intl.ListFormat(i18n.language, { type: 'conjunction' }).format(winners.map(name)),
              count: winners.length,
            })}
      </h2>
      <p className="mt-1 text-sm text-slate-400">{t('okey.lowestWins')}</p>
      <ScoreTable seats={seats} name={name} hand={view.hand.end?.scores} totals={view.totals} you={you} />
      <Button className="mt-6 w-full" onClick={leave}>
        {t('results.leave')}
      </Button>
    </Overlay>
  )
}

function Overlay({ children }: { children: ReactNode }) {
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="absolute inset-0 z-20 grid place-items-center bg-black/40 p-4 backdrop-blur-sm"
    >
      <motion.div
        dir="auto"
        initial={{ scale: 0.92, y: 12 }}
        animate={{ scale: 1, y: 0 }}
        className="w-full max-w-md rounded-3xl bg-slate-900/90 p-6 shadow-2xl ring-1 ring-white/10"
      >
        {children}
      </motion.div>
    </motion.div>
  )
}

/** Each player's points for the hand and in all; penalty points, so lower is better. */
function ScoreTable({
  seats,
  name,
  hand,
  totals,
  you,
}: {
  seats: number[]
  name: (seat: number) => string
  hand?: number[]
  totals: number[]
  you?: number
}) {
  const { t } = useTranslation()
  const tone = (n: number) => (n < 0 ? 'text-emerald-300' : n > 0 ? 'text-rose-300' : 'text-slate-300')
  return (
    <table className="mt-4 w-full text-sm">
      <thead>
        <tr className="text-xs uppercase tracking-wide text-slate-500">
          <th className="pb-2 text-start font-medium" />
          {hand && <th className="pb-2 text-end font-medium">{t('okey.thisHand')}</th>}
          <th className="pb-2 text-end font-medium">{t('okey.total')}</th>
        </tr>
      </thead>
      <tbody>
        {seats.map((seat) => (
          <tr key={seat} className="border-t border-white/5">
            <td className="py-1.5">
              {name(seat)}
              {seat === you && <span className="text-slate-400"> · {t('board.you')}</span>}
            </td>
            {hand && <td className={`py-1.5 text-end font-semibold tabular-nums ${tone(hand[seat])}`}>{hand[seat]}</td>}
            <td className={`py-1.5 text-end font-semibold tabular-nums ${tone(totals[seat])}`}>{totals[seat]}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

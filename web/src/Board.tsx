import {
  closestCenter,
  DndContext,
  DragOverlay,
  KeyboardSensor,
  MouseSensor,
  pointerWithin,
  TouchSensor,
  useDroppable,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type DragOverEvent,
  type DragStartEvent,
} from '@dnd-kit/core'
import {
  arrayMove,
  horizontalListSortingStrategy,
  rectSortingStrategy,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useRef, useState } from 'react'

import { arrangeRack, arrangeTable, compareTiles, splitRows, tileName, type Arrangement, type RackSort } from './arrange.ts'
import type { Settings } from './settings.ts'
import { SettingsMenu } from './SettingsMenu.tsx'
import { TileFace, type TileSize } from './TileView.tsx'
import { Button } from './ui.tsx'
import type { GameState, Status } from './useGame.ts'
import { checkTurn, classifySet, type ClientMessage, type RoomInfo, type Tile } from './wasm/engine.js'

/**
 * The game screen: players along the top, the table in the middle, the rack
 * at the bottom. Everything moves by dragging (dnd-kit): rack tiles any
 * time, to rearrange; tiles onto and around the table on your turn; whole
 * sets any time, to order the table as you like. The arrangement is yours
 * alone and outlives the turn (see arrange.ts).
 */

/** A tile with an identity, so the same tile can move between places. */
interface Placed {
  id: string
  tile: Tile
  fromTable: boolean
}

/**
 * Where the tiles are: `holders` by id (`rack-0`, `rack-1`, `set-<key>`),
 * and the table's sets by key, in the order shown.
 */
interface Layout {
  holders: Record<string, Placed[]>
  sets: number[]
}

const RACK_ROWS = ['rack-0', 'rack-1'] as const
const isRack = (holder: string) => holder.startsWith('rack-')
const setHolder = (key: number) => `set-${key}`
/** The drag id of a whole set (its holder is the drop target for tiles). */
const setItem = (key: number) => `move-set-${key}`
const isSetItem = (id: string) => id.startsWith('move-set-')
const NEW_SET = 'new-set'

const tilesOf = (placed: Placed[]) => placed.map((p) => p.tile)

function lay(view: GameState['view'], arrangement: Arrangement, rackStyle: Settings['rackStyle']): Layout {
  let id = 0
  const place = (tile: Tile, fromTable: boolean) => ({ id: `tile-${id++}`, tile, fromTable })
  const holders: Layout['holders'] = {}
  const sets: number[] = []
  for (const set of arrangeTable(arrangement.table, view.table)) {
    holders[setHolder(sets.length)] = set.map((tile) => place(tile, true))
    sets.push(sets.length)
  }
  const rows = arrangeRack(arrangement.rows, arrangement.sort, view.rack)
  RACK_ROWS.forEach((row, i) => (holders[row] = rows[i].map((tile) => place(tile, false))))
  return restyle({ holders, sets }, rackStyle)
}

/** The rack's tiles in one row (flat) or two (classic). */
function restyle(layout: Layout, rackStyle: Settings['rackStyle']): Layout {
  const all = [...layout.holders['rack-0'], ...layout.holders['rack-1']]
  const rows =
    rackStyle === 'flat'
      ? [all, []]
      : layout.holders['rack-1'].length === 0
        ? splitRows(all)
        : [layout.holders['rack-0'], layout.holders['rack-1']]
  return { ...layout, holders: { ...layout.holders, 'rack-0': rows[0], 'rack-1': rows[1] } }
}

/** The holder a tile is in, or the holder (or new-set spot) `id` names. */
function holderOf(layout: Layout, id: string): string | undefined {
  if (id in layout.holders || id === NEW_SET) return id
  if (isSetItem(id)) return setHolder(Number(id.slice('move-set-'.length)))
  return Object.keys(layout.holders).find((holder) => layout.holders[holder].some((p) => p.id === id))
}

function dropEmptySets(layout: Layout): Layout {
  const sets = layout.sets.filter((key) => layout.holders[setHolder(key)].length > 0)
  if (sets.length === layout.sets.length) return layout
  const holders = { ...layout.holders }
  for (const key of layout.sets) if (!sets.includes(key)) delete holders[setHolder(key)]
  return { holders, sets }
}

/** A table to compare regardless of the order of its sets. */
const canonical = (sets: Tile[][]) => sets.map((s) => JSON.stringify(s)).sort().join('|')

/**
 * Tiles are dropped where the pointer is; a set only among sets. Tiles
 * before the holders they're in, so dropping on a tile places beside it.
 */
const collisions: CollisionDetection = (args) => {
  const movingSet = isSetItem(String(args.active.id))
  const droppableContainers = args.droppableContainers.filter((c) => isSetItem(String(c.id)) === movingSet)
  const within = pointerWithin({ ...args, droppableContainers })
  if (within.length > 0) {
    const isTile = (id: unknown) => Number(String(id).startsWith('tile-'))
    return [...within].sort((a, b) => isTile(b.id) - isTile(a.id))
  }
  return closestCenter({ ...args, droppableContainers })
}

/** The turn length the timer ring shows a fraction of. */
const TURN_SECONDS = 60

/** Seconds until `ends`, ticking. */
function useSecondsLeft(ends: number | undefined) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 250)
    return () => clearInterval(timer)
  }, [])
  return ends === undefined ? undefined : Math.max(0, Math.ceil((ends - now) / 1000))
}

/** Give it `key={game.version}`: a new state from the server starts afresh from `arrangement`. */
export function Board({
  game,
  room,
  status,
  problem,
  settings,
  onSettings,
  arrangement,
  onArrange,
  send,
  leave,
}: {
  game: GameState
  room: RoomInfo
  status: Status
  problem: string | null
  settings: Settings
  onSettings: (change: Partial<Settings>) => void
  arrangement: Arrangement
  onArrange: (arrangement: Arrangement) => void
  send: (message: ClientMessage) => void
  leave: () => void
}) {
  const { view } = game
  const myTurn = view.current === view.you && !view.outcome
  const [start] = useState(() => lay(view, arrangement, settings.rackStyle))
  const [layout, setLayout] = useState(start)
  const [sort, setSort] = useState<RackSort>(arrangement.sort)
  // What's being dragged (a tile or a set's id) and the holder it came from.
  const [dragging, setDragging] = useState<{ id: string; from?: string } | null>(null)
  // The layout when a drag began, to go back to if it's cancelled.
  const beforeDrag = useRef<Layout | null>(null)
  const nextSet = useRef(start.sets.length)
  const secondsLeft = useSecondsLeft(game.turnEnds)

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    // A short hold, so a swipe still scrolls.
    useSensor(TouchSensor, { activationConstraint: { delay: 150, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  // Keep the arrangement for the next state from the server.
  useEffect(() => {
    onArrange({
      rows: [tilesOf(layout.holders['rack-0']), tilesOf(layout.holders['rack-1'])],
      sort,
      table: layout.sets.map((key) => tilesOf(layout.holders[setHolder(key)])),
    })
  }, [layout, sort, onArrange])

  const table = layout.sets.map((key) => tilesOf(layout.holders[setHolder(key)])).filter((set) => set.length > 0)
  const changed = canonical(table) !== canonical(view.table)
  const verdict = myTurn && changed
    ? checkTurn({ before: view.table, after: table, rack: view.rack, melded: view.melded[view.you], rules: view.rules })
    : null

  /** Whether `tile` may go into `holder` (or the new-set spot). */
  const mayGo = (tile: Placed, holder: string) => (isRack(holder) ? !tile.fromTable : myTurn)

  const onDragStart = ({ active }: DragStartEvent) => {
    beforeDrag.current = layout
    setDragging({ id: String(active.id), from: holderOf(layout, String(active.id)) })
  }

  // A tile crossing into another holder moves there as it goes, so the
  // tiles around it make room.
  const onDragOver = ({ active, over }: DragOverEvent) => {
    const id = String(active.id)
    if (isSetItem(id) || !over) return
    const overId = String(over.id)
    const translated = active.rect.current.translated
    setLayout((l) => {
      const from = holderOf(l, id)
      const to = holderOf(l, overId)
      if (!from || !to || to === NEW_SET || from === to) return l
      const moving = l.holders[from].find((p) => p.id === id)!
      if (!mayGo(moving, to)) return l
      const target = l.holders[to]
      const overIndex = target.findIndex((p) => p.id === overId)
      const pastMiddle = translated !== null && translated.left + translated.width / 2 > over.rect.left + over.rect.width / 2
      const index = overIndex < 0 ? target.length : overIndex + (pastMiddle ? 1 : 0)
      return {
        ...l,
        holders: {
          ...l.holders,
          [from]: l.holders[from].filter((p) => p.id !== id),
          [to]: [...target.slice(0, index), moving, ...target.slice(index)],
        },
      }
    })
  }

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    setDragging(null)
    const id = String(active.id)
    if (!over) return onDragCancel()
    const overId = String(over.id)
    if (isSetItem(id)) {
      setLayout((l) => {
        const from = l.sets.findIndex((key) => setItem(key) === id)
        const to = l.sets.findIndex((key) => setItem(key) === overId)
        return to < 0 || from === to ? l : { ...l, sets: arrayMove(l.sets, from, to) }
      })
      return
    }
    const startedIn = dragging?.from
    setLayout((l) => {
      const from = holderOf(l, id)
      const to = holderOf(l, overId)
      if (!from || !to) return l
      const moving = l.holders[from].find((p) => p.id === id)!
      if (to === NEW_SET) {
        if (!mayGo(moving, NEW_SET)) return l
        const key = nextSet.current++
        return dropEmptySets({
          holders: { ...l.holders, [from]: l.holders[from].filter((p) => p.id !== id), [setHolder(key)]: [moving] },
          sets: [...l.sets, key],
        })
      }
      if (from !== to) return dropEmptySets(l)
      const items = l.holders[from]
      const oldIndex = items.findIndex((p) => p.id === id)
      const newIndex = items.findIndex((p) => p.id === overId)
      const moved = newIndex < 0 || newIndex === oldIndex ? items : arrayMove(items, oldIndex, newIndex)
      return dropEmptySets({ ...l, holders: { ...l.holders, [from]: moved } })
    })
    // Moving rack tiles by hand ends any sort order.
    if ((startedIn && isRack(startedIn)) || isRack(holderOf(layout, overId) ?? '')) setSort('custom')
  }

  const onDragCancel = () => {
    setDragging(null)
    if (beforeDrag.current) setLayout(beforeDrag.current)
  }

  const sortRack = (by: 'number' | 'color') => {
    setSort(by)
    setLayout((l) => {
      const all = [...l.holders['rack-0'], ...l.holders['rack-1']].sort((a, b) => compareTiles(a.tile, b.tile, by))
      const rows = settings.rackStyle === 'flat' ? [all, []] : splitRows(all)
      return { ...l, holders: { ...l.holders, 'rack-0': rows[0], 'rack-1': rows[1] } }
    })
  }

  const changeSettings = (change: Partial<Settings>) => {
    if (change.rackStyle) {
      const style = change.rackStyle
      setLayout((l) => restyle(l, style))
    }
    onSettings(change)
  }

  const name = (seat: number) => room.seats[seat]?.name ?? `Player ${seat + 1}`
  const note = ((): { tone: 'neutral' | 'good' | 'bad'; text: string } => {
    if (problem) return { tone: 'bad', text: problem }
    if (view.outcome) return { tone: 'neutral', text: 'The game is over.' }
    if (!myTurn) return { tone: 'neutral', text: `${name(view.current)} is playing.` }
    if (verdict?.verdict === 'rejected') return { tone: 'bad', text: verdict.message }
    if (verdict?.verdict === 'allowed') {
      const n = verdict.played.length
      return { tone: 'good', text: `Ready: ${n} tile${n === 1 ? '' : 's'} from your rack.` }
    }
    if (view.melded[view.you]) return { tone: 'neutral', text: 'Your turn: drag tiles onto the table, or draw.' }
    return { tone: 'neutral', text: `Your turn. Your first sets must be worth ${view.rules.initialMeld}, from your rack alone.` }
  })()
  const noteColor = { neutral: 'text-slate-300', good: 'text-emerald-300', bad: 'text-amber-300' }[note.tone]

  // What the pointer carries.
  const draggedTile =
    dragging && !isSetItem(dragging.id) ? Object.values(layout.holders).flat().find((p) => p.id === dragging.id) : undefined
  const draggedSet = dragging && isSetItem(dragging.id) ? layout.holders[holderOf(layout, dragging.id)!] : undefined
  const flat = settings.rackStyle === 'flat'

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={collisions}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}
      onDragCancel={onDragCancel}
    >
      <div className="flex h-dvh flex-col gap-3 p-2 sm:gap-4 sm:p-4 short:gap-2 short:p-2">
        <TopBar room={room} view={view} secondsLeft={secondsLeft} status={status} settings={settings} onSettings={changeSettings} />

        <main className="felt relative min-h-0 flex-1 overflow-y-auto rounded-3xl p-4 sm:p-6 short:rounded-2xl short:p-3">
          <span className="absolute right-4 top-3 rounded-full bg-black/25 px-2.5 py-1 text-xs font-medium text-emerald-100/70 sm:right-6 sm:top-4">
            {view.pool} in the pool
          </span>
          {layout.sets.length === 0 && !myTurn ? (
            <p className="grid h-full place-items-center text-center text-emerald-100/40">The table is empty. The first meld opens it.</p>
          ) : (
            <SortableContext items={layout.sets.map(setItem)} strategy={rectSortingStrategy}>
              <div className="flex flex-wrap content-start items-center gap-x-5 gap-y-4 pt-6 sm:gap-x-7 sm:gap-y-5 short:gap-x-4 short:gap-y-2 short:pt-5">
                {layout.sets.map((key) => (
                  <TableSet key={key} setKey={key} tiles={layout.holders[setHolder(key)]} myTurn={myTurn} />
                ))}
                {myTurn && <NewSetSpot active={draggedTile !== undefined} />}
              </div>
            </SortableContext>
          )}
          <AnimatePresence>{view.outcome && <Results view={view} name={name} leave={leave} />}</AnimatePresence>
        </main>

        <section className={flat ? 'tray rounded-3xl p-3 sm:p-4 short:rounded-2xl short:p-2' : ''}>
          <div className={`flex flex-wrap items-center gap-x-4 gap-y-2 ${flat ? 'mb-3 short:mb-2' : 'mb-2 px-1'}`}>
            <p className={`min-w-0 flex-1 text-sm font-medium short:truncate ${noteColor}`}>{note.text}</p>
            <SortButtons sort={sort} onSort={sortRack} />
            {myTurn && (
              <div className="flex gap-2">
                <Button look="quiet" onClick={() => setLayout(start)} disabled={!changed}>
                  Reset
                </Button>
                <Button onClick={() => send({ type: 'draw' })}>Draw</Button>
                <Button look="primary" disabled={verdict?.verdict !== 'allowed'} onClick={() => send({ type: 'play', table })}>
                  End turn
                </Button>
              </div>
            )}
          </div>
          {flat ? (
            <RackRow id="rack-0" tiles={layout.holders['rack-0']} wrap />
          ) : (
            <div className="rack-classic space-y-1.5 rounded-2xl px-2 pb-1.5 pt-2 sm:px-3 short:space-y-1 short:pt-1.5">
              <RackRow id="rack-0" tiles={layout.holders['rack-0']} />
              <RackRow id="rack-1" tiles={layout.holders['rack-1']} />
            </div>
          )}
        </section>
      </div>

      <DragOverlay dropAnimation={{ duration: 180, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' }}>
        {draggedTile ? (
          <TileFace tile={draggedTile.tile} size={dragging?.from && isRack(dragging.from) ? (flat ? 'rack' : 'ledge') : 'table'} lifted />
        ) : draggedSet ? (
          <div className="flex gap-1 p-1">
            {draggedSet.map((p) => (
              <TileFace key={p.id} tile={p.tile} size="table" lifted />
            ))}
          </div>
        ) : null}
      </DragOverlay>
    </DndContext>
  )
}

function SortableTile({ placed, size, draggable }: { placed: Placed; size: TileSize; draggable: boolean }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: placed.id,
    disabled: { draggable: !draggable, droppable: false },
  })
  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Translate.toString(transform), transition }}
      {...attributes}
      {...listeners}
      aria-label={tileName(placed.tile)}
      aria-roledescription="tile"
      className={`touch-manipulation rounded-md outline-offset-2 focus-visible:outline-2 focus-visible:outline-teal-300 ${
        draggable ? 'cursor-grab active:cursor-grabbing' : 'cursor-default'
      } ${isDragging ? 'opacity-30' : ''}`}
    >
      <TileFace tile={placed.tile} size={size} />
    </div>
  )
}

function TableSet({ setKey, tiles, myTurn }: { setKey: number; tiles: Placed[]; myTurn: boolean }) {
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({
    id: setItem(setKey),
  })
  const { setNodeRef: setHolderRef } = useDroppable({ id: setHolder(setKey) })
  const valid = classifySet(tilesOf(tiles)) !== undefined
  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Translate.toString(transform), transition }}
      className={`flex items-stretch rounded-lg transition-shadow ${isDragging ? 'opacity-30' : ''} ${
        valid ? '' : 'bg-amber-400/15 shadow-[0_0_0_2px_rgb(251_191_36/0.7),0_0_24px_rgb(251_191_36/0.25)]'
      }`}
    >
      <button
        ref={setActivatorNodeRef}
        {...attributes}
        {...listeners}
        aria-label="Move this set"
        aria-roledescription="set"
        className="grid w-4 cursor-grab touch-manipulation place-items-center rounded-l-lg text-emerald-100/25 transition hover:bg-white/5 hover:text-emerald-100/70 active:cursor-grabbing"
      >
        <svg viewBox="0 0 6 16" className="h-4 w-1.5" fill="currentColor" aria-hidden>
          {[2, 8, 14].map((y) => [1, 5].map((x) => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.1" />))}
        </svg>
      </button>
      <SortableContext items={tiles.map((p) => p.id)} strategy={horizontalListSortingStrategy}>
        {/* Reaching into the gap after the set, so a tile dropped just past its end joins it. */}
        <div ref={setHolderRef} className="-mr-5 flex gap-1 py-1 pr-6">
          {tiles.map((p) => (
            <SortableTile key={p.id} placed={p} size="table" draggable={myTurn} />
          ))}
        </div>
      </SortableContext>
    </div>
  )
}

/** Where a dropped tile starts a set of its own. */
function NewSetSpot({ active }: { active: boolean }) {
  const { setNodeRef, isOver } = useDroppable({ id: NEW_SET })
  return (
    <div
      ref={setNodeRef}
      className={`grid h-14 min-w-24 place-items-center rounded-lg border-2 border-dashed px-3 text-xs font-semibold uppercase tracking-wider transition sm:h-16 short:h-11 ${
        isOver
          ? 'border-emerald-200 bg-emerald-200/15 text-emerald-50'
          : active
            ? 'border-emerald-200/50 text-emerald-100/70'
            : 'border-emerald-200/20 text-emerald-100/35'
      }`}
    >
      New set
    </div>
  )
}

function RackRow({ id, tiles, wrap = false }: { id: string; tiles: Placed[]; wrap?: boolean }) {
  const { setNodeRef } = useDroppable({ id })
  const row = (
    <SortableContext items={tiles.map((p) => p.id)} strategy={wrap ? rectSortingStrategy : horizontalListSortingStrategy}>
      <div
        ref={setNodeRef}
        className={
          wrap
            ? 'flex min-h-16 flex-wrap gap-1.5 sm:min-h-[4.75rem] sm:gap-2 short:min-h-12 short:flex-nowrap short:overflow-x-auto short:pb-1'
            : 'flex min-h-14 gap-1 overflow-x-auto px-0.5 pb-0.5 [scrollbar-width:thin] sm:min-h-[4.75rem] sm:gap-1.5 sm:px-1 short:min-h-12'
        }
      >
        {tiles.map((p) => (
          <SortableTile key={p.id} placed={p} size={wrap ? 'rack' : 'ledge'} draggable />
        ))}
      </div>
    </SortableContext>
  )
  return wrap ? row : <div className="rack-ledge">{row}</div>
}

function SortButtons({ sort, onSort }: { sort: RackSort; onSort: (by: 'number' | 'color') => void }) {
  const options: ['number' | 'color', string][] = [
    ['number', '1 2 3'],
    ['color', 'Colour'],
  ]
  return (
    <div className="flex rounded-xl bg-black/30 p-0.5 text-xs font-semibold" role="group" aria-label="Sort the rack">
      {options.map(([by, label]) => (
        <button
          key={by}
          onClick={() => onSort(by)}
          aria-pressed={sort === by}
          className={`cursor-pointer rounded-[0.6rem] px-2.5 py-1.5 transition ${
            sort === by ? 'bg-amber-100/90 text-stone-900' : 'text-amber-100/70 hover:text-amber-50'
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  )
}

/** One colour per seat, for avatars. */
const seatColors = ['from-teal-400 to-cyan-600', 'from-violet-400 to-fuchsia-600', 'from-amber-300 to-orange-500', 'from-rose-400 to-pink-600']

function TopBar({
  room,
  view,
  secondsLeft,
  status,
  settings,
  onSettings,
}: {
  room: RoomInfo
  view: GameState['view']
  secondsLeft?: number
  status: Status
  settings: Settings
  onSettings: (change: Partial<Settings>) => void
}) {
  return (
    <header className="flex items-center gap-3">
      <div className="hidden shrink-0 items-baseline gap-2 sm:flex short:hidden">
        <span className="text-lg font-bold tracking-tight">Rummy</span>
        <span className="font-mono text-xs tracking-widest text-slate-500">{room.code}</span>
      </div>
      <div className="flex min-w-0 flex-1 gap-2 overflow-x-auto sm:justify-center">
        {view.rackSizes.map((size, seat) => (
          <PlayerChip
            key={seat}
            seat={seat}
            name={room.seats[seat]?.name ?? `Player ${seat + 1}`}
            you={seat === view.you}
            connected={room.seats[seat]?.connected ?? false}
            tiles={size}
            secondsLeft={seat === view.current && !view.outcome ? secondsLeft : undefined}
            current={seat === view.current && !view.outcome}
          />
        ))}
      </div>
      <span
        className={`size-2.5 shrink-0 rounded-full ${status === 'open' ? 'bg-emerald-400' : status === 'connecting' ? 'animate-pulse bg-amber-400' : 'bg-rose-500'}`}
        title={status === 'open' ? 'Connected' : status === 'connecting' ? 'Connecting' : 'Disconnected'}
      />
      <SettingsMenu settings={settings} onChange={onSettings} />
    </header>
  )
}

function PlayerChip({
  seat,
  name,
  you,
  connected,
  tiles,
  current,
  secondsLeft,
}: {
  seat: number
  name: string
  you: boolean
  connected: boolean
  tiles: number
  current: boolean
  secondsLeft?: number
}) {
  const fraction = secondsLeft === undefined ? 0 : Math.min(1, secondsLeft / TURN_SECONDS)
  const urgent = secondsLeft !== undefined && secondsLeft <= 10
  return (
    <div
      title={`${name}: ${tiles} tiles${connected ? '' : ', away'}`}
      className={`flex shrink-0 items-center gap-2.5 rounded-2xl p-1.5 ring-1 transition sm:pr-3 short:pr-1.5 ${
        current ? 'bg-white/10 ring-teal-300/50' : 'bg-white/[0.04] ring-white/10'
      } ${connected ? '' : 'opacity-50'}`}
    >
      <div className="relative size-9">
        {current && (
          <svg viewBox="0 0 36 36" className="absolute -inset-1 size-11 -rotate-90" aria-hidden>
            <circle cx="18" cy="18" r="16.5" fill="none" stroke="rgb(255 255 255 / 0.1)" strokeWidth="2" />
            <circle
              cx="18"
              cy="18"
              r="16.5"
              fill="none"
              stroke={urgent ? '#fbbf24' : '#5eead4'}
              strokeWidth="2"
              strokeLinecap="round"
              strokeDasharray={`${fraction * 103.7} 103.7`}
              className="transition-[stroke-dasharray] duration-300"
            />
          </svg>
        )}
        <div
          className={`grid size-9 place-items-center rounded-full bg-linear-to-br text-sm font-bold text-slate-950 ${seatColors[seat % seatColors.length]}`}
        >
          {name.slice(0, 1).toUpperCase()}
        </div>
        {/* On small screens the chip is just the avatar: the tile count rides on it. */}
        <span className="absolute -bottom-1 -right-1.5 hidden min-w-5 rounded-full bg-slate-900 px-1 text-center text-[0.7rem] font-semibold tabular-nums text-slate-200 ring-1 ring-white/15 max-sm:block short:block">
          {tiles}
        </span>
      </div>
      <div className="leading-tight max-sm:hidden short:hidden">
        <div className="max-w-28 truncate text-sm font-semibold">
          {name}
          {you && <span className="font-normal text-slate-400"> · you</span>}
        </div>
        <div className="text-xs text-slate-400">
          {current && secondsLeft !== undefined ? (
            <span className={`tabular-nums ${urgent ? 'text-amber-300' : 'text-teal-300'}`}>{secondsLeft}s · </span>
          ) : null}
          {tiles} tiles{connected ? '' : ' · away'}
        </div>
      </div>
    </div>
  )
}

function Results({ view, name, leave }: { view: GameState['view']; name: (seat: number) => string; leave: () => void }) {
  const outcome = view.outcome!
  const order = view.rackSizes.map((_, seat) => seat).sort((a, b) => outcome.scores[b] - outcome.scores[a])
  const won = outcome.winners.includes(view.you)
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      className="absolute inset-0 grid place-items-center bg-black/40 p-4 backdrop-blur-sm"
    >
      <motion.div
        initial={{ scale: 0.92, y: 12 }}
        animate={{ scale: 1, y: 0 }}
        className="w-full max-w-sm rounded-3xl bg-slate-900/90 p-6 shadow-2xl ring-1 ring-white/10"
      >
        <h2 className="text-2xl font-bold">{won ? 'You won!' : `${outcome.winners.map(name).join(' and ')} won`}</h2>
        <ol className="mt-4 space-y-2">
          {order.map((seat) => (
            <li key={seat} className="flex justify-between gap-4">
              <span>{name(seat)}</span>
              <span className={`font-semibold tabular-nums ${outcome.scores[seat] >= 0 ? 'text-emerald-300' : 'text-rose-300'}`}>
                {outcome.scores[seat] > 0 ? '+' : ''}
                {outcome.scores[seat]}
              </span>
            </li>
          ))}
        </ol>
        <Button className="mt-6 w-full" onClick={leave}>
          Leave the room
        </Button>
      </motion.div>
    </motion.div>
  )
}

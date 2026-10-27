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
  type DragMoveEvent,
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
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react'

import {
  arrangeRack,
  arrangeTable,
  compareTiles,
  joinEnd,
  keepPlans,
  newOrChanged,
  sortTiles,
  splitRows,
  tileName,
  without,
  type Arrangement,
  type RackSort,
} from './arrange.ts'
import { footprint, gridWithout, resolve, type Cell } from './grid.ts'
import type { Settings } from './settings.ts'
import { SettingsMenu } from './SettingsMenu.tsx'
import { remember, remembered } from './storage.ts'
import { TileFace, type TileSize } from './TileView.tsx'
import { Button } from './ui.tsx'
import type { Draft, GameState, Status, TurnRecord } from './useGame.ts'
import { checkTurn, classifySet, type ClientMessage, type RoomInfo, type Tile } from './wasm/engine.js'

/**
 * The game screen: players along the top, the table in the middle, the rack
 * at the bottom. Everything moves by dragging (dnd-kit): rack tiles any
 * time, to rearrange; tiles onto and around the table on your turn; whole
 * sets any time, anywhere on the table's grid (see grid.ts). The
 * arrangement is yours alone and outlives the turn (see arrange.ts).
 */

/** A tile with an identity, so the same tile can move between places. */
interface Placed {
  id: string
  tile: Tile
  fromTable: boolean
}

/**
 * Where the tiles are: `holders` by id (`rack-0`, `rack-1`, `set-<key>`,
 * `plan-<key>`); the table's sets by key, and where the player put each on
 * the grid; and the sets the player is planning from their rack.
 */
interface Layout {
  holders: Record<string, Placed[]>
  sets: number[]
  at: Record<number, Cell | undefined>
  plans: number[]
}

const RACK_ROWS = ['rack-0', 'rack-1'] as const
const isRack = (holder: string) => holder.startsWith('rack-')
const setHolder = (key: number) => `set-${key}`
const planHolder = (key: number) => `plan-${key}`
const isPlan = (holder: string) => holder.startsWith('plan-')
/** Holders of the player's own tiles: the rack and their plans. */
const isOwn = (holder: string) => isRack(holder) || isPlan(holder)
/** The drag id of a whole set (its holder is the drop target for tiles). */
const setItem = (key: number) => `move-set-${key}`
const isSetItem = (id: string) => id.startsWith('move-set-')
const setKeyOf = (item: string) => Number(item.slice('move-set-'.length))
/** The table itself: a tile dropped on empty space starts a set there. */
const NEW_SET = 'new-set'
/** The planning area: a tile dropped there starts a plan. */
const NEW_PLAN = 'new-plan'

const tilesOf = (placed: Placed[]) => placed.map((p) => p.tile)

function lay(view: GameState['view'], arrangement: Arrangement, rackStyle: Settings['rackStyle']): Layout {
  let id = 0
  const place = (tile: Tile, fromTable: boolean) => ({ id: `tile-${id++}`, tile, fromTable })
  const holders: Layout['holders'] = {}
  const sets: number[] = []
  const at: Layout['at'] = {}
  for (const set of arrangeTable(arrangement.table, view.table)) {
    const key = sets.length
    holders[setHolder(key)] = set.tiles.map((tile) => place(tile, true))
    at[key] = set.at
    sets.push(key)
  }
  const plans = keepPlans(arrangement.plans ?? [], view.rack)
  plans.forEach((plan, key) => (holders[planHolder(key)] = plan.map((tile) => place(tile, false))))
  const rows = arrangeRack(arrangement.rows, arrangement.sort, without(view.rack, plans.flat()))
  RACK_ROWS.forEach((row, i) => (holders[row] = rows[i].map((tile) => place(tile, false))))
  return restyle({ holders, sets, at, plans: plans.map((_, key) => key) }, rackStyle)
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

/** The holder a tile is in, or the holder (or the table) `id` names. */
function holderOf(layout: Layout, id: string): string | undefined {
  if (id in layout.holders || id === NEW_SET || id === NEW_PLAN) return id
  if (isSetItem(id)) return setHolder(setKeyOf(id))
  return Object.keys(layout.holders).find((holder) => layout.holders[holder].some((p) => p.id === id))
}

/** The layout without sets or plans left empty. */
function dropEmptySets(layout: Layout): Layout {
  const sets = layout.sets.filter((key) => layout.holders[setHolder(key)].length > 0)
  const plans = layout.plans.filter((key) => layout.holders[planHolder(key)].length > 0)
  if (sets.length === layout.sets.length && plans.length === layout.plans.length) return layout
  const holders = { ...layout.holders }
  const at = { ...layout.at }
  for (const key of layout.sets) {
    if (sets.includes(key)) continue
    delete holders[setHolder(key)]
    delete at[key]
  }
  for (const key of layout.plans) if (!plans.includes(key)) delete holders[planHolder(key)]
  return { holders, sets, at, plans }
}

/** `tile` moved from holder `from` to an end of holder `to`. */
function moveTile(l: Layout, tile: Placed, from: string, to: string, end: 'start' | 'end' = 'end'): Layout {
  const rest = l.holders[to] ?? []
  return {
    ...l,
    holders: {
      ...l.holders,
      [from]: l.holders[from].filter((p) => p.id !== tile.id),
      [to]: end === 'end' ? [...rest, tile] : [tile, ...rest],
    },
  }
}

/** Ids for the tiles of sets that arrive in another player's draft. */
let liveTileIds = 0

/**
 * The layout with the table replaced by `table` (another player's draft, or
 * the state again), arranged as this player has it. Sets that didn't change
 * keep their key and tiles, so they stay put on screen (even mid-drag).
 */
function withTable(l: Layout, table: Tile[][]): Layout {
  const previous = l.sets.map((key) => ({ key, tiles: tilesOf(l.holders[setHolder(key)]), at: l.at[key] }))
  const holders: Layout['holders'] = Object.fromEntries(Object.entries(l.holders).filter(([holder]) => !holder.startsWith('set-')))
  const sets: number[] = []
  const at: Layout['at'] = {}
  let nextKey = Math.max(-1, ...l.sets) + 1
  for (const set of arrangeTable(previous, table)) {
    const kept = previous.findIndex(
      (p) => JSON.stringify(p.tiles) === JSON.stringify(set.tiles) && p.at?.row === set.at?.row && p.at?.col === set.at?.col,
    )
    const key = kept >= 0 ? previous.splice(kept, 1)[0].key : nextKey++
    holders[setHolder(key)] =
      kept >= 0 ? l.holders[setHolder(key)] : set.tiles.map((tile) => ({ id: `live-${liveTileIds++}`, tile, fromTable: true }))
    at[key] = set.at
    sets.push(key)
  }
  return { holders, sets, at, plans: l.plans }
}

/** A table to compare regardless of the order of its sets. */
const canonical = (sets: Tile[][]) => sets.map((s) => JSON.stringify(s)).sort().join('|')

/** Where a dragged tile lands when several things are under the pointer: the most specific. */
const dropRank = (id: string) =>
  id.startsWith('tile-') ? 0 : id.startsWith('set-') || isPlan(id) ? 1 : isRack(id) ? 2 : 3

const collisions: CollisionDetection = (args) => {
  // A set lands wherever it's let go (see onDragMove), not on anything.
  if (isSetItem(String(args.active.id))) return []
  const within = pointerWithin(args)
  if (within.length > 0) return [...within].sort((a, b) => dropRank(String(a.id)) - dropRank(String(b.id)))
  return closestCenter(args)
}

/** The size of a table cell, and how many fit across (set in index.css, `.table-grid`). */
interface Metrics {
  cellW: number
  cellH: number
  columns: number
}

/** A dragged set's (or a new set's) landing place, shown as an outline. */
interface Target {
  cell: Cell
  width: number
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
  draft,
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
  /** The current player's table so far, if they share it and this player watches. */
  draft: Draft | null
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
  const [target, setTarget] = useState<Target | null>(null)
  // The set the player just dropped: it's already where the pointer left
  // it, so it appears there rather than gliding over from where it was.
  const [dropped, setDropped] = useState<number | null>(null)
  // The layout when a drag began, to go back to if it's cancelled.
  const beforeDrag = useRef<Layout | null>(null)
  const nextSet = useRef(start.sets.length)
  const nextPlan = useRef(start.plans.length)
  // The set the player last changed on the table, and among their plans: a
  // double-clicked tile joins it if it fits.
  const lastSet = useRef<number | null>(null)
  const lastPlan = useRef<number | null>(null)
  const tableGrid = useRef<HTMLDivElement | null>(null)
  const [metrics, setMetrics] = useState<Metrics | null>(null)
  const secondsLeft = useSecondsLeft(game.turnEnds)

  // Another player's turn, as they share it: their draft in place of the
  // table, until the next state (or the draft goes away).
  const liveTable = !myTurn && draft && draft.version === game.version ? draft.table : null
  const [shownLive, setShownLive] = useState<Tile[][] | null>(null)
  if (liveTable !== shownLive) {
    setShownLive(liveTable)
    setLayout((l) => withTable(l, liveTable ?? view.table))
  }
  // Watching: tiles new to the table in a draft glide to their places like
  // dragged ones, from where they were on the table or, from the rack, from
  // the player's avatar. Where every tile was is noted after each render
  // (the tile's wrapper, which never moves; its face is what's animated).
  const tilesWere = useRef(new Map<string, { tile: string; rect: DOMRect }>())
  const glidedFor = useRef<Tile[][] | null>(null)
  useLayoutEffect(() => {
    const grid = tableGrid.current
    if (myTurn || !grid) return
    const now = [...grid.querySelectorAll<HTMLElement>('[data-tile-id]')].map((element) => ({
      element,
      id: element.dataset.tileId!,
      tile: element.dataset.tile!,
      rect: element.getBoundingClientRect(),
    }))
    const before = tilesWere.current
    tilesWere.current = new Map(now.map(({ id, tile, rect }) => [id, { tile, rect }]))
    if (shownLive === glidedFor.current) return
    glidedFor.current = shownLive
    if (!shownLive || matchMedia('(prefers-reduced-motion: reduce)').matches) return

    // Tiles that left their place, to be found again elsewhere.
    const present = new Set(now.map(({ id }) => id))
    const left = [...before].filter(([id]) => !present.has(id)).map(([, was]) => was)
    const avatar = document.querySelector(`[data-seat="${view.current}"]`)?.getBoundingClientRect()
    let order = 0
    for (const { element, id, tile, rect } of now) {
      if (before.has(id)) continue
      const centre = (r: DOMRect) => [r.left + r.width / 2, r.top + r.height / 2]
      const [x, y] = centre(rect)
      // The nearest tile like it that left its place; else the rack.
      let from: number[] | undefined
      let best = Infinity
      left.forEach((was, i) => {
        if (was.tile !== tile) return
        const [wx, wy] = centre(was.rect)
        const distance = Math.hypot(wx - x, wy - y)
        if (distance < best) [best, from] = [distance, [wx, wy, i]]
      })
      if (from) left.splice(from[2], 1)
      else if (avatar) from = centre(avatar)
      if (!from || Math.hypot(from[0] - x, from[1] - y) < 2) continue
      const face = element.firstElementChild as HTMLElement
      const set = element.closest<HTMLElement>('[data-state]')
      // Above the other sets while it travels.
      if (set) set.style.zIndex = '20'
      const lifted = 'scale(1.08) rotate(-3deg)'
      face
        .animate(
          [
            { transform: `translate(${from[0] - x}px, ${from[1] - y}px) ${lifted}`, offset: 0 },
            { transform: `translate(0, 0) ${lifted}`, offset: 0.8 },
            { transform: 'none', offset: 1 },
          ],
          { duration: 650, delay: order++ * 140, easing: 'cubic-bezier(0.3, 0.7, 0.2, 1)', fill: 'backwards' },
        )
        .finished.then(() => set && (set.style.zIndex = ''))
        .catch(() => {})
    }
  })

  // The sets they've made or changed so far.
  const liveSets = useMemo(
    () => new Set(liveTable ? newOrChanged(view.table, liveTable).map((set) => JSON.stringify(set)) : []),
    [liveTable, view.table],
  )

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    // A short hold, so a swipe still scrolls.
    useSensor(TouchSensor, { activationConstraint: { delay: 150, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  // The table's cell size and width, again whenever it's resized.
  useLayoutEffect(() => {
    const grid = tableGrid.current
    if (!grid) return
    const measure = () => {
      const style = getComputedStyle(grid)
      const cellW = parseFloat(style.getPropertyValue('--cell-w'))
      const cellH = parseFloat(style.getPropertyValue('--cell-h'))
      const columns = Math.max(4, Math.floor(grid.clientWidth / cellW))
      setMetrics((m) => (m && m.cellW === cellW && m.cellH === cellH && m.columns === columns ? m : { cellW, cellH, columns }))
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(grid)
    return () => observer.disconnect()
  }, [])

  // Each set's tiles at the start of the turn, to tell which the player changed.
  const startTiles = useMemo(
    () => new Map(start.sets.map((key) => [key, JSON.stringify(tilesOf(start.holders[setHolder(key)]))])),
    [start],
  )

  // The sets another player made or changed on their last turn, as they are now.
  const recent = useMemo(() => {
    const left = (game.lastPlay?.sets ?? []).map((set) => JSON.stringify(set))
    return new Set(
      start.sets.filter((key) => {
        const i = left.indexOf(startTiles.get(key)!)
        if (i >= 0) left.splice(i, 1)
        return i >= 0
      }),
    )
  }, [game.lastPlay, start, startTiles])
  const recentBy = game.lastPlay && {
    seat: game.lastPlay.by,
    name: room.seats[game.lastPlay.by]?.name ?? `Player ${game.lastPlay.by + 1}`,
    // Flash when it's news; afterwards a quiet outline until the table changes.
    fresh: game.lastPlay.version === game.version,
  }

  const widths = useMemo(
    () => new Map(layout.sets.map((key) => [key, footprint(layout.holders[setHolder(key)].length)])),
    [layout],
  )
  const shown = useMemo(
    () => resolve(layout.sets.map((key) => ({ key, width: widths.get(key)!, at: layout.at[key] })), metrics?.columns ?? 12),
    [layout, widths, metrics],
  )

  /**
   * Every set's place: where the player put it, or for a set new to them,
   * where it's shown now. Pinned when a set moves, so the others don't.
   */
  const pinned = (l: Layout): Layout['at'] =>
    Object.fromEntries(l.sets.map((key) => [key, l.at[key] ?? shown.get(key)]))

  // Keep the arrangement for the next state from the server. A set new to
  // this player keeps the place it was first shown in (once the table's
  // width is known).
  useEffect(() => {
    onArrange({
      rows: [tilesOf(layout.holders['rack-0']), tilesOf(layout.holders['rack-1'])],
      sort,
      table: layout.sets.map((key) => ({
        tiles: tilesOf(layout.holders[setHolder(key)]),
        at: layout.at[key] ?? (metrics ? shown.get(key) : undefined),
      })),
      plans: layout.plans.map((key) => tilesOf(layout.holders[planHolder(key)])),
    })
  }, [layout, sort, onArrange, metrics, shown])

  const table = layout.sets.map((key) => tilesOf(layout.holders[setHolder(key)])).filter((set) => set.length > 0)
  const changed = canonical(table) !== canonical(view.table)
  const verdict = myTurn && changed
    ? checkTurn({ before: view.table, after: table, rack: view.rack, melded: view.melded[view.you], rules: view.rules })
    : null

  // On this player's turn, in a room that shares turns live: the table after
  // each drop, for the others to watch.
  const shareLive = myTurn && room.options.liveTurns
  const shared = useRef(canonical(view.table))
  useEffect(() => {
    if (!shareLive || dragging) return
    const now = canonical(table)
    if (now === shared.current) return
    shared.current = now
    send({ type: 'draft', table })
  }, [shareLive, dragging, table, send])

  /** Whether `tile` may go into `holder` (or onto the table as a new set). */
  const mayGo = (tile: Placed, holder: string) => (isOwn(holder) || holder === NEW_PLAN ? !tile.fromTable : myTurn)

  /**
   * Where a set `width` wide lands if let go at `cell`: there if it's free,
   * else the nearest free place in that row, else nowhere.
   */
  const landing = (cell: Cell, width: number, except?: number): Target | null => {
    if (!metrics) return null
    const grid = gridWithout(shown, widths, metrics.columns, except)
    const free = grid.fits(cell, width) ? cell : grid.nearestInRow(cell, width)
    return free && { cell: free, width }
  }

  const onDragStart = ({ active }: DragStartEvent) => {
    beforeDrag.current = layout
    setDropped(null)
    setDragging({ id: String(active.id), from: holderOf(layout, String(active.id)) })
  }

  // Where a set, or a tile over empty table, would land: shown as an outline.
  const onDragMove = ({ active, over }: DragMoveEvent) => {
    const id = String(active.id)
    const rect = active.rect.current.translated
    const grid = tableGrid.current
    if (!rect || !grid || !metrics) return
    const box = grid.getBoundingClientRect()
    const row = Math.round((rect.top - box.top) / metrics.cellH)
    const col = Math.round((rect.left - box.left) / metrics.cellW)
    let next: Target | null = null
    if (isSetItem(id)) {
      const key = setKeyOf(id)
      next = landing({ row, col }, widths.get(key)!, key)
    } else if (myTurn && over?.id === NEW_SET) {
      // The tile will be the set's first, after its grip.
      next = landing({ row, col: col - 1 }, footprint(1))
    }
    setTarget((t) =>
      t && next && t.width === next.width && t.cell.row === next.cell.row && t.cell.col === next.cell.col ? t : next,
    )
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
      if (!from || !to || to === NEW_SET || to === NEW_PLAN || from === to) return l
      const moving = l.holders[from].find((p) => p.id === id)!
      if (!mayGo(moving, to)) return l
      const items = l.holders[to]
      const overIndex = items.findIndex((p) => p.id === overId)
      const pastMiddle = translated !== null && translated.left + translated.width / 2 > over.rect.left + over.rect.width / 2
      const index = overIndex < 0 ? items.length : overIndex + (pastMiddle ? 1 : 0)
      return {
        ...l,
        holders: {
          ...l.holders,
          [from]: l.holders[from].filter((p) => p.id !== id),
          [to]: [...items.slice(0, index), moving, ...items.slice(index)],
        },
      }
    })
  }

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    const id = String(active.id)
    const landed = target
    setDragging(null)
    setTarget(null)
    if (isSetItem(id)) {
      if (landed) {
        setDropped(setKeyOf(id))
        setLayout((l) => ({ ...l, at: { ...pinned(l), [setKeyOf(id)]: landed.cell } }))
        // Once it's drawn in its new place, it may glide again (on a resize, say).
        setTimeout(() => setDropped(null), 100)
      }
      return
    }
    if (!over) return cancel()
    const overId = String(over.id)
    setLayout((l) => {
      const from = holderOf(l, id)
      const to = holderOf(l, overId)
      if (!from || !to) return l
      const moving = l.holders[from].find((p) => p.id === id)!
      if (to === NEW_SET) {
        if (!mayGo(moving, NEW_SET) || !landed) return beforeDrag.current ?? l
        const key = nextSet.current++
        lastSet.current = key
        return dropEmptySets({
          ...moveTile(l, moving, from, setHolder(key)),
          sets: [...l.sets, key],
          at: { ...pinned(l), [key]: landed.cell },
        })
      }
      if (to === NEW_PLAN) {
        if (!mayGo(moving, NEW_PLAN)) return beforeDrag.current ?? l
        const key = nextPlan.current++
        lastPlan.current = key
        return dropEmptySets({ ...moveTile(l, moving, from, planHolder(key)), plans: [...l.plans, key] })
      }
      if (to.startsWith('set-')) lastSet.current = Number(to.slice('set-'.length))
      if (isPlan(to)) lastPlan.current = Number(to.slice('plan-'.length))
      if (from !== to) return dropEmptySets(l)
      const items = l.holders[from]
      const oldIndex = items.findIndex((p) => p.id === id)
      const newIndex = items.findIndex((p) => p.id === overId)
      const moved = newIndex < 0 || newIndex === oldIndex ? items : arrayMove(items, oldIndex, newIndex)
      return dropEmptySets({ ...l, holders: { ...l.holders, [from]: moved } })
    })
    // Placing tiles in the rack by hand ends any sort order.
    if (isRack(holderOf(layout, overId) ?? '')) setSort('custom')
  }

  /** Where a set `width` wide goes when not dragged there: the first free place from the last row on. */
  const freeCell = (width: number) => {
    if (!metrics) return null
    const lastRow = Math.max(0, ...[...shown.values()].map((cell) => cell.row))
    return gridWithout(shown, widths, metrics.columns).firstFree(width, lastRow)
  }

  /**
   * A double-clicked tile. From the rack: onto the table on the player's
   * turn, else into a plan; either way it joins the set last changed there
   * if it fits, else starts a new one. From a plan: on the player's turn the
   * whole plan goes onto the table, else the tile goes back to the rack.
   */
  const quickMove = (tile: Placed) => {
    const from = holderOf(layout, tile.id)
    if (!from || view.outcome) return
    if (isPlan(from)) {
      if (myTurn) return playPlan(from)
      return setLayout((l) => dropEmptySets(moveTile(l, tile, from, RACK_ROWS[flat ? 0 : 1])))
    }
    if (!isRack(from)) return
    const last = myTurn
      ? lastSet.current !== null && layout.sets.includes(lastSet.current)
        ? lastSet.current
        : null
      : lastPlan.current !== null && layout.plans.includes(lastPlan.current)
        ? lastPlan.current
        : (layout.plans.at(-1) ?? null)
    const holder = myTurn ? setHolder : planHolder
    const end = last === null ? null : joinEnd(tilesOf(layout.holders[holder(last)]), tile.tile)
    if (last !== null && end) {
      if (myTurn) lastSet.current = last
      else lastPlan.current = last
      return setLayout((l) => moveTile(l, tile, from, holder(last), end))
    }
    if (!myTurn) {
      const key = nextPlan.current++
      lastPlan.current = key
      return setLayout((l) => ({ ...moveTile(l, tile, from, planHolder(key)), plans: [...l.plans, key] }))
    }
    const cell = freeCell(footprint(1))
    if (!cell) return
    const key = nextSet.current++
    lastSet.current = key
    setLayout((l) => ({ ...moveTile(l, tile, from, setHolder(key)), sets: [...l.sets, key], at: { ...pinned(l), [key]: cell } }))
  }

  /** A plan put on the table as a new set, on the player's turn. */
  const playPlan = (plan: string) => {
    const tiles = layout.holders[plan]
    const cell = myTurn && tiles ? freeCell(footprint(tiles.length)) : null
    if (!cell) return
    const key = nextSet.current++
    lastSet.current = key
    setLayout((l) =>
      dropEmptySets({
        ...l,
        holders: { ...l.holders, [plan]: [], [setHolder(key)]: l.holders[plan] },
        sets: [...l.sets, key],
        at: { ...pinned(l), [key]: cell },
      }),
    )
  }

  const cancel = () => {
    setDragging(null)
    setTarget(null)
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
    if (!myTurn) {
      const placed = liveTable ? liveTable.flat().length - view.table.flat().length : 0
      return {
        tone: 'neutral',
        text:
          liveTable && (placed > 0 || liveSets.size > 0)
            ? `${name(view.current)} is playing: ${placed} tile${placed === 1 ? '' : 's'} from their rack so far.`
          : `${name(view.current)} is playing.`,
      }
    }
    if (verdict?.verdict === 'rejected') return { tone: 'bad', text: verdict.message }
    if (verdict?.verdict === 'allowed') {
      const n = verdict.played.length
      return { tone: 'good', text: `Ready: ${n} tile${n === 1 ? '' : 's'} from your rack.` }
    }
    // What the last player did, if it's news.
    const news =
      recentBy?.fresh && game.lastPlay
        ? `${recentBy.name} played ${game.lastPlay.played} tile${game.lastPlay.played === 1 ? '' : 's'}. `
        : ''
    if (view.melded[view.you]) return { tone: 'neutral', text: `${news}Your turn: drag tiles onto the table (or double-click them), or draw.` }
    return {
      tone: 'neutral',
      text: `${news}Your turn. Your first sets must be worth ${view.rules.initialMeld}, from your rack alone.`,
    }
  })()
  const noteColor = { neutral: 'text-slate-300', good: 'text-emerald-300', bad: 'text-amber-300' }[note.tone]

  // What the pointer carries.
  const draggedTile =
    dragging && !isSetItem(dragging.id) ? Object.values(layout.holders).flat().find((p) => p.id === dragging.id) : undefined
  const draggedSet = dragging && isSetItem(dragging.id) ? layout.holders[holderOf(layout, dragging.id)!] : undefined
  const flat = settings.rackStyle === 'flat'
  // A row of room below the last, to move a set into a new one.
  const rows = Math.max(0, ...[...shown.values()].map((cell) => cell.row + 1)) + 1

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={collisions}
      onDragStart={onDragStart}
      onDragMove={onDragMove}
      onDragOver={onDragOver}
      onDragEnd={onDragEnd}
      onDragCancel={cancel}
    >
      <div className="flex h-dvh flex-col gap-3 p-2 sm:gap-4 sm:p-4 short:gap-2 short:p-2">
        <TopBar
          room={room}
          view={view}
          secondsLeft={secondsLeft}
          status={status}
          settings={settings}
          onSettings={changeSettings}
        />

        {game.racks && <OpenRacks code={room.code} game={game} racks={game.racks} name={name} />}

        <main className="felt relative min-h-0 flex-1 overflow-auto rounded-3xl p-3 [scrollbar-color:rgb(255_255_255/0.15)_transparent] [scrollbar-width:thin] sm:p-6 short:rounded-2xl short:p-3">
          <TableGrid gridRef={tableGrid} droppable={myTurn} height={metrics ? rows * metrics.cellH : undefined}>
            {layout.sets.length === 0 && (
              <p className="pointer-events-none absolute inset-0 grid place-items-center text-center text-emerald-100/40">
                {myTurn ? 'Drop tiles anywhere on the table to start a set.' : 'The table is empty. The first meld opens it.'}
              </p>
            )}
            {target && metrics && (
              <div
                className="pointer-events-none absolute rounded-lg border-2 border-dashed border-emerald-200/60 bg-emerald-200/10 transition-[left,top,width] duration-100"
                style={{
                  left: target.cell.col * metrics.cellW,
                  top: target.cell.row * metrics.cellH,
                  width: target.width * metrics.cellW - 4,
                  height: metrics.cellH - 14,
                }}
              />
            )}
            {metrics &&
              layout.sets.map((key) => (
                <TableSet
                  key={key}
                  setKey={key}
                  tiles={layout.holders[setHolder(key)]}
                  at={shown.get(key)!}
                  metrics={metrics}
                  myTurn={myTurn}
                  glide={key !== dropped}
                  changed={myTurn && startTiles.get(key) !== JSON.stringify(tilesOf(layout.holders[setHolder(key)]))}
                  // Once the current player changes the table live, last turn's marks give way.
                  recentBy={recent.has(key) && liveSets.size === 0 ? recentBy : undefined}
                  liveBy={
                    liveTable && liveSets.has(JSON.stringify(tilesOf(layout.holders[setHolder(key)])))
                      ? { seat: view.current, name: name(view.current) }
                      : undefined
                  }
                />
              ))}
          </TableGrid>
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
          <Plans
            plans={layout.plans.map((key) => ({ holder: planHolder(key), tiles: layout.holders[planHolder(key)] }))}
            myTurn={myTurn}
            onQuick={quickMove}
            onPlay={playPlan}
          />
          {flat ? (
            <RackRow id="rack-0" tiles={layout.holders['rack-0']} wrap onPlay={quickMove} />
          ) : (
            <div className="rack-classic space-y-1.5 rounded-2xl px-2 pb-1.5 pt-2 sm:px-3 short:space-y-1 short:pt-1.5">
              <RackRow id="rack-0" tiles={layout.holders['rack-0']} onPlay={quickMove} />
              <RackRow id="rack-1" tiles={layout.holders['rack-1']} onPlay={quickMove} />
            </div>
          )}
        </section>
      </div>

      <DragOverlay dropAnimation={draggedSet ? null : { duration: 180, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' }}>
        {draggedTile ? (
          <TileFace
            tile={draggedTile.tile}
            size={dragging?.from && isRack(dragging.from) ? (flat ? 'rack' : 'ledge') : 'table'}
            lifted
          />
        ) : draggedSet ? (
          <div className="flex items-center gap-1 pl-4">
            {draggedSet.map((p) => (
              <TileFace key={p.id} tile={p.tile} size="table" lifted />
            ))}
          </div>
        ) : null}
      </DragOverlay>
    </DndContext>
  )
}

function SortableTile({
  placed,
  size,
  draggable,
  onDoubleClick,
}: {
  placed: Placed
  size: TileSize
  draggable: boolean
  onDoubleClick?: () => void
}) {
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
      onDoubleClick={onDoubleClick}
      aria-label={tileName(placed.tile)}
      aria-roledescription="tile"
      data-tile-id={placed.id}
      data-tile={tileName(placed.tile)}
      className={`touch-manipulation rounded-md outline-offset-2 focus-visible:outline-2 focus-visible:outline-teal-300 ${
        draggable ? 'cursor-grab active:cursor-grabbing' : 'cursor-default'
      } ${isDragging ? 'opacity-30' : ''}`}
    >
      <TileFace tile={placed.tile} size={size} />
    </div>
  )
}

/**
 * The table's grid. Its own component, as a drop target must be inside the
 * DndContext (which Board renders): a tile dropped on empty space starts a
 * set there.
 */
function TableGrid({
  gridRef,
  droppable,
  height,
  children,
}: {
  gridRef: RefObject<HTMLDivElement | null>
  droppable: boolean
  height?: number
  children: ReactNode
}) {
  const { setNodeRef } = useDroppable({ id: NEW_SET, disabled: !droppable })
  const ref = useCallback(
    (element: HTMLDivElement | null) => {
      gridRef.current = element
      setNodeRef(element)
    },
    [gridRef, setNodeRef],
  )
  return (
    <div ref={ref} className="table-grid relative" style={{ height, minHeight: '100%' }}>
      {children}
    </div>
  )
}

/** A set on the table, at its cell; dragged by its grip, anywhere. */
function TableSet({
  setKey,
  tiles,
  at,
  metrics,
  myTurn,
  glide,
  changed,
  recentBy,
  liveBy,
}: {
  setKey: number
  tiles: Placed[]
  at: Cell
  metrics: Metrics
  myTurn: boolean
  /** Slide to a new place (moved for the player) rather than jump (dropped by them). */
  glide: boolean
  /** New this turn, or its tiles changed (moving the set doesn't count). */
  changed: boolean
  /** Who made or changed it on their last turn, if another player did. */
  recentBy?: { seat: number; name: string; fresh: boolean }
  /** Who is changing it right now, on their turn (live turns). */
  liveBy?: { seat: number; name: string }
}) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: setItem(setKey) })
  const { setNodeRef: setHolderRef } = useDroppable({ id: setHolder(setKey) })
  // Another player's set in the making (it may be invalid for now); else
  // invalid first (it's the one to fix), then this turn's own changes, then
  // another player's last.
  const state = liveBy
    ? 'live'
    : classifySet(tilesOf(tiles)) === undefined
      ? 'invalid'
      : changed
        ? 'changed'
        : recentBy
          ? 'recent'
          : 'same'
  const by = liveBy ?? recentBy
  const look = {
    live: 'live-set',
    invalid: 'bg-amber-400/15 shadow-[0_0_0_2px_rgb(251_191_36/0.7),0_0_24px_rgb(251_191_36/0.25)]',
    changed: 'bg-emerald-400/15 shadow-[0_0_0_2px_rgb(52_211_153/0.75),0_0_24px_rgb(52_211_153/0.25)]',
    recent: recentBy?.fresh ? 'recent-flash' : 'recent-quiet',
    same: '',
  }[state]
  return (
    <div
      ref={setNodeRef}
      data-state={state}
      style={{ left: at.col * metrics.cellW, top: at.row * metrics.cellH }}
      className={`absolute flex items-stretch rounded-lg duration-200 ${
        glide ? 'transition-[left,top,box-shadow,background-color]' : 'transition-[box-shadow,background-color]'
      } ${isDragging ? 'opacity-30' : ''} ${look}`}
    >
      {(state === 'recent' || state === 'live') && by && (
        <span
          title={state === 'live' ? `${by.name} is changing this` : `${by.name}'s last turn`}
          className={`pointer-events-none absolute -left-2 -top-2 z-10 grid size-5 place-items-center rounded-full bg-linear-to-br text-[0.65rem] font-bold text-slate-950 ring-2 ring-felt ${seatColors[by.seat % seatColors.length]}`}
        >
          {by.name.slice(0, 1).toUpperCase()}
        </span>
      )}
      <button
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
        {/* Reaching a little past the set, so a tile dropped just after its end joins it. */}
        <div ref={setHolderRef} className="-mr-5 flex gap-1 py-1 pr-6">
          {tiles.map((p) => (
            <SortableTile key={p.id} placed={p} size="table" draggable={myTurn} />
          ))}
        </div>
      </SortableContext>
    </div>
  )
}

function RackRow({
  id,
  tiles,
  wrap = false,
  onPlay,
}: {
  id: string
  tiles: Placed[]
  wrap?: boolean
  /** A double-clicked tile (see `quickMove`). */
  onPlay?: (tile: Placed) => void
}) {
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
          <SortableTile
            key={p.id}
            placed={p}
            size={wrap ? 'rack' : 'ledge'}
            draggable
            onDoubleClick={onPlay && (() => onPlay(p))}
          />
        ))}
      </div>
    </SortableContext>
  )
  return wrap ? row : <div className="rack-ledge">{row}</div>
}

/**
 * Sets the player is planning from their rack, at the top of the tray: built
 * any time by dragging or double-clicking, put on the table on their turn.
 * Its own component, as drop targets must be inside the DndContext.
 */
function Plans({
  plans,
  myTurn,
  onQuick,
  onPlay,
}: {
  plans: { holder: string; tiles: Placed[] }[]
  myTurn: boolean
  onQuick: (tile: Placed) => void
  onPlay: (holder: string) => void
}) {
  const { setNodeRef, isOver } = useDroppable({ id: NEW_PLAN })
  return (
    <div
      ref={setNodeRef}
      className={`mb-2 flex min-h-11 flex-wrap items-center gap-2 rounded-2xl border border-dashed px-2 py-1.5 transition short:mb-1.5 ${
        isOver ? 'border-amber-100/50 bg-amber-100/10' : 'border-amber-100/15'
      }`}
    >
      {plans.length === 0 && (
        <p className="pointer-events-none px-1 text-xs text-amber-100/40">
          Plan sets here: drag tiles in, or double-click them when it isn’t your turn.
        </p>
      )}
      {plans.map((plan) => (
        <Plan key={plan.holder} {...plan} myTurn={myTurn} onQuick={onQuick} onPlay={onPlay} />
      ))}
    </div>
  )
}

function Plan({
  holder,
  tiles,
  myTurn,
  onQuick,
  onPlay,
}: {
  holder: string
  tiles: Placed[]
  myTurn: boolean
  onQuick: (tile: Placed) => void
  onPlay: (holder: string) => void
}) {
  const { setNodeRef } = useDroppable({ id: holder })
  const valid = classifySet(tilesOf(tiles)) !== undefined
  return (
    <SortableContext items={tiles.map((p) => p.id)} strategy={horizontalListSortingStrategy}>
      <div
        ref={setNodeRef}
        title={valid ? 'A valid set' : 'Not a set yet'}
        className={`flex items-center gap-1 rounded-xl p-1 ring-1 ${valid ? 'bg-emerald-300/10 ring-emerald-300/50' : 'ring-amber-100/20'}`}
      >
        {tiles.map((p) => (
          <SortableTile key={p.id} placed={p} size="table" draggable onDoubleClick={() => onQuick(p)} />
        ))}
        {myTurn && (
          <button
            className="ml-0.5 grid size-7 cursor-pointer place-items-center rounded-lg text-amber-100/70 hover:bg-white/10 hover:text-amber-50"
            title="Put it on the table"
            aria-label="Put it on the table"
            onClick={() => onPlay(holder)}
          >
            ↑
          </button>
        )}
      </div>
    </SortableContext>
  )
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
      <div className="hidden shrink-0 items-baseline gap-2 md:flex short:hidden">
        <span className="text-lg font-bold tracking-tight">Rummy</span>
        <span className="font-mono text-xs tracking-widest text-slate-500">{room.code}</span>
      </div>
      <div className="flex min-w-0 flex-1 gap-2 overflow-x-auto [scrollbar-width:none] md:justify-center">
        {view.rackSizes.map((size, seat) => (
          <PlayerChip
            key={seat}
            seat={seat}
            name={room.seats[seat]?.name ?? `Player ${seat + 1}`}
            you={seat === view.you}
            connected={room.seats[seat]?.connected ?? false}
            bot={room.seats[seat]?.bot !== undefined}
            tiles={size}
            secondsLeft={seat === view.current && !view.outcome ? secondsLeft : undefined}
            current={seat === view.current && !view.outcome}
          />
        ))}
      </div>
      <span
        className="flex shrink-0 items-center gap-1.5 text-sm font-medium tabular-nums text-slate-300"
        title={`${view.pool} tiles left to draw`}
      >
        <span className="h-4 w-3 rounded-[3px] bg-linear-to-b from-stone-100 to-stone-300 shadow-sm" aria-hidden />
        {view.pool}
      </span>
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
  bot,
  tiles,
  current,
  secondsLeft,
}: {
  seat: number
  name: string
  you: boolean
  connected: boolean
  bot: boolean
  tiles: number
  current: boolean
  secondsLeft?: number
}) {
  const fraction = secondsLeft === undefined ? 0 : Math.min(1, secondsLeft / TURN_SECONDS)
  const urgent = secondsLeft !== undefined && secondsLeft <= 10
  return (
    <div
      title={`${name}: ${tiles} tiles${connected ? '' : ', away'}`}
      data-seat={seat}
      className={`flex shrink-0 items-center gap-2.5 rounded-2xl p-1.5 ring-1 transition md:pr-3 short:pr-1.5 ${
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
        <span className="absolute -bottom-1 -right-1.5 hidden min-w-5 rounded-full bg-slate-900 px-1 text-center text-[0.7rem] font-semibold tabular-nums text-slate-200 ring-1 ring-white/15 max-md:block short:block">
          {tiles}
        </span>
      </div>
      <div className="leading-tight max-md:hidden short:hidden">
        <div className="max-w-28 truncate text-sm font-semibold">
          {name}
          {you && <span className="font-normal text-slate-400"> · you</span>}
          {bot && <span className="font-normal text-violet-300"> · bot</span>}
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

/** Whether the open racks panel is folded away, for the tab. */
const OPEN_RACKS_KEY = 'rummy.openRacksFolded'
/** How the player ordered the others' open racks in a room, for the tab. */
const OPEN_ORDERS_KEY = 'rummy.openRackOrders'

/** An open rack's order: sorted, or as the player dragged it ('custom'). */
interface RackOrder {
  sort: RackSort
  tiles: Tile[]
}

/** The orders of the others' racks in room `code`, by seat; kept across states. */
function useRackOrders(code: string): [Record<number, RackOrder>, (seat: number, order: RackOrder) => void] {
  const [saved, setSaved] = useState(() => {
    const saved = remembered<{ code: string; orders: Record<number, RackOrder> }>(OPEN_ORDERS_KEY, 'session')
    return saved?.code === code ? saved.orders : {}
  })
  const change = (seat: number, order: RackOrder) => {
    const orders = { ...saved, [seat]: order }
    remember(OPEN_ORDERS_KEY, { code, orders }, 'session')
    setSaved(orders)
  }
  return [saved, change]
}

/**
 * Everyone else's rack and the turns so far, when the server shows every
 * rack (`--open-racks`): for judging how the bots play. The racks can be
 * sorted and reordered by dragging, to think through their moves.
 */
function OpenRacks({
  code,
  game,
  racks,
  name,
}: {
  code: string
  game: GameState
  racks: Tile[][]
  name: (seat: number) => string
}) {
  const [folded, setFolded] = useState(() => remembered<boolean>(OPEN_RACKS_KEY, 'session') ?? false)
  const fold = (folded: boolean) => {
    remember(OPEN_RACKS_KEY, folded, 'session')
    setFolded(folded)
  }
  const [orders, setOrder] = useRackOrders(code)
  const { view } = game
  const others = racks.map((_, seat) => seat).filter((seat) => seat !== view.you)
  return (
    <section className="shrink-0 rounded-2xl bg-white/[0.04] px-3 py-2 ring-1 ring-white/10">
      <div className="flex items-center gap-2 text-xs text-slate-400">
        <span className="font-semibold uppercase tracking-wide text-violet-300">Open racks</span>
        <span className="flex-1 max-sm:hidden">Debugging: every rack is shown. Sort or drag them to think their moves through.</span>
        <Button look="quiet" className="ml-auto px-2 py-0.5 text-xs" onClick={() => fold(!folded)}>
          {folded ? 'Show' : 'Hide'}
        </Button>
      </div>
      {!folded && (
        <div className="mt-2 flex max-h-52 gap-4 max-lg:flex-col max-lg:overflow-auto">
          <ul className="min-w-0 space-y-1.5 overflow-auto lg:flex-1">
            {others.map((seat) => {
              const order = orders[seat] ?? { sort: 'color', tiles: [] }
              return (
                <OpenRack
                  key={seat}
                  seat={seat}
                  name={name(seat)}
                  tiles={arrangeRack([order.tiles, []], order.sort, racks[seat]).flat()}
                  sort={order.sort}
                  drew={game.log.findLast((turn) => turn.by === seat)?.drew ?? []}
                  onSort={(by) => setOrder(seat, { sort: by, tiles: sortTiles(racks[seat], by) })}
                  onReorder={(tiles) => setOrder(seat, { sort: 'custom', tiles })}
                />
              )
            })}
          </ul>
          <ol className="min-w-0 space-y-1 overflow-auto text-xs text-slate-300 lg:w-[28rem]">
            {game.log.length === 0 && <li className="text-slate-500">Turns appear here as they're played.</li>}
            {[...game.log].reverse().map((turn) => (
              <TurnLine key={turn.version} turn={turn} name={name(turn.by)} />
            ))}
          </ol>
        </div>
      )}
    </section>
  )
}

/**
 * Another player's open rack, in the player's order. Its own DndContext, so
 * its tiles only move within it, apart from the board's.
 */
function OpenRack({
  seat,
  name,
  tiles,
  sort,
  drew,
  onSort,
  onReorder,
}: {
  seat: number
  name: string
  tiles: Tile[]
  sort: RackSort
  /** The tiles they drew last turn, to mark. */
  drew: Tile[]
  onSort: (by: 'number' | 'color') => void
  onReorder: (tiles: Tile[]) => void
}) {
  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    useSensor(TouchSensor, { activationConstraint: { delay: 150, tolerance: 8 } }),
  )
  // Ids by place: the order only changes when a drag ends.
  const ids = tiles.map((_, i) => `open-${seat}-${i}`)
  const fresh = new Set<number>()
  for (const drawn of drew) {
    const at = tiles.findIndex((tile, i) => !fresh.has(i) && tileName(tile) === tileName(drawn))
    if (at >= 0) fresh.add(at)
  }
  const onDragEnd = ({ active, over }: DragEndEvent) => {
    const from = ids.indexOf(String(active.id))
    const to = over ? ids.indexOf(String(over.id)) : -1
    if (from >= 0 && to >= 0 && from !== to) onReorder(arrayMove(tiles, from, to))
  }
  return (
    <li className="flex items-center gap-2">
      <span
        className={`grid size-6 shrink-0 place-items-center rounded-full bg-linear-to-br text-[0.7rem] font-bold text-slate-950 ${seatColors[seat % seatColors.length]}`}
        title={name}
      >
        {name.slice(0, 1).toUpperCase()}
      </span>
      <div className="-mx-1 shrink-0 scale-90">
        <SortButtons sort={sort} onSort={onSort} />
      </div>
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
        <SortableContext items={ids} strategy={rectSortingStrategy}>
          <div className="flex min-w-0 flex-1 flex-wrap gap-0.5">
            {tiles.map((tile, i) => (
              <OpenTile key={ids[i]} id={ids[i]} tile={tile} fresh={fresh.has(i)} />
            ))}
          </div>
        </SortableContext>
      </DndContext>
    </li>
  )
}

function OpenTile({ id, tile, fresh }: { id: string; tile: Tile; fresh: boolean }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id })
  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Translate.toString(transform), transition }}
      {...attributes}
      {...listeners}
      aria-label={tileName(tile)}
      aria-roledescription="tile"
      title={fresh ? 'Drawn last turn' : undefined}
      className={`cursor-grab touch-manipulation rounded-[4px] active:cursor-grabbing ${fresh ? 'ring-2 ring-teal-300' : ''} ${
        isDragging ? 'relative z-10 opacity-80' : ''
      }`}
    >
      <TileFace tile={tile} size="mini" />
    </div>
  )
}

function TurnLine({ turn, name }: { turn: TurnRecord; name: string }) {
  if (turn.played.length === 0) {
    return (
      <li className="flex items-center gap-1.5 text-slate-400">
        <span className="font-semibold text-slate-300">{name}</span>
        {turn.drew.length > 0 ? 'drew' : 'passed (the pool is empty)'}
        {turn.drew.map((tile, i) => (
          <TileFace key={i} tile={tile} size="mini" />
        ))}
      </li>
    )
  }
  return (
    <li className="flex flex-wrap items-center gap-1.5">
      <span className="font-semibold">{name}</span>
      played {turn.played.length}
      <span className="flex gap-0.5">
        {sortTiles(turn.played, 'color').map((tile, i) => (
          <TileFace key={i} tile={tile} size="mini" />
        ))}
      </span>
      <span className="text-slate-400">
        · {turn.sets} set{turn.sets === 1 ? '' : 's'} new or changed
      </span>
    </li>
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

import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  MouseSensor,
  TouchSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragMoveEvent,
  type DragStartEvent,
} from '@dnd-kit/core'
import { arrayMove, rectSortingStrategy, SortableContext, sortableKeyboardCoordinates, useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { compareTiles, type RackSort } from './arrange.ts'
import { TableGrid, TopBar, WatchBar, type Clock } from './Board.tsx'
import { footprint, gridWithout, resolve, type Cell } from './grid.ts'
import { noticeText, okeyError, problemText, type Problem } from './messages.ts'
import type { Settings } from './settings.ts'
import {
  collisions,
  DISCARD,
  FalseOkeyMark,
  isPlanTarget,
  isSetItem,
  isSetTarget,
  joinNew,
  NEW_PLAN,
  enqueue,
  NEXT,
  NEW_SET,
  OFFER,
  okeySet,
  planTarget,
  rackAndPlans,
  RACK,
  same,
  sameTile,
  type SavedRack,
  setItem,
  setTarget,
  TAKEN,
  tilesOf,
  type Placed,
} from './okeyParts.ts'
import { remember, remembered } from './storage.ts'
import { TileFace, type TileSize } from './TileView.tsx'
import { Button } from './ui.tsx'
import type { Draft, OkeyGame, Status } from './useGame.ts'
import { useNow } from './useNow.ts'
import { useTableMetrics, type Metrics } from './useTableMetrics.ts'
import {
  checkOkeyTurn,
  okeyMeldKind,
  okeyProgress,
  type ClientMessage,
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

/** Remembered for the tab: the player's rack order and plans in their current room. */
const RACK_KEY = 'rummy.okeyRack'
/** Remembered for the tab: where the player put the table's sets, by their place in it, this hand. */
const TABLE_KEY = 'rummy.okeyTable'

/** The board's tiles: the rack, the plans, and the table's sets (new ones left empty keep their place). */
interface Tiles {
  rack: Placed[]
  plans: Placed[][]
  sets: Placed[][]
  /** The tiles set aside to discard next, first to go first. */
  parked: Placed[]
}

/** `tile` put into `list` at its front or back. */
const insertAt = (list: Placed[], tile: Placed, front: boolean) => (front ? [tile, ...list] : [...list, tile])

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
  const canTake = myTurn && hand.phase === 'draw' && offer !== undefined && !sameTile(offer, hand.okey)
  const opened = !watching && hand.opened[you] !== 'no'

  const [sort, setSort] = useState<RackSort>(() => remembered<RackSort>('rummy.rackSort') ?? 'custom')
  // Plans and the tile set aside belong to one hand.
  const handKey = `${view.played}:${JSON.stringify(hand.indicator)}`
  const [tiles, setTiles] = useState<Tiles>(() => ({
    ...rackAndPlans(RACK_KEY, room.code, handKey, hand.rack, sort),
    sets: hand.table.map((set, s) => set.map((tile, i) => ({ id: `t${s}-${i}`, tile }))),
  }))
  const { rack, plans, sets, parked } = tiles
  /** Sets the board's tiles, telling the server when the tile set aside to discard next changes. */
  const update = (next: Tiles) => {
    if (!watching && !same([tilesOf(next.parked)], [tilesOf(parked)])) send({ type: 'discardNext', tiles: tilesOf(next.parked) })
    setTiles(next)
  }
  /** Where the player put each set, by its place on the table; unset, wherever it's shown. */
  const [at, setAt] = useState<(Cell | undefined)[]>(() => {
    const saved = remembered<{ code: string; played: number; at: (Cell | null)[] }>(TABLE_KEY, 'session')
    return saved && saved.code === room.code && saved.played === view.played ? saved.at.map((cell) => cell ?? undefined) : []
  })
  const [taken, setTaken] = useState(false)
  /** A turn refused here, before it was sent. */
  const [refused, setRefused] = useState<Problem | null>(null)
  const [dragging, setDragging] = useState<{ placed?: Placed; set?: number } | null>(null)
  const [target, setTarget] = useState<{ cell: Cell; width: number } | null>(null)
  /** The tile last put on the table, by drag or double-click: a double-clicked tile joins its set if it fits. */
  const lastPlaced = useRef<string | null>(null)
  /** The plan last changed, which a double-clicked tile joins if it fits. */
  const lastPlan = useRef<number | null>(null)
  /** Where the player last changed a set: on the table or in their plans. */
  const lastIn = useRef<'table' | 'plans'>('table')
  const touchTable = (id: string | null) => {
    lastPlaced.current = id
    lastIn.current = 'table'
  }
  const touchPlan = (plan: number) => {
    lastPlan.current = plan
    lastIn.current = 'plans'
  }
  const tableGrid = useRef<HTMLDivElement | null>(null)
  const metrics = useTableMetrics(tableGrid)

  // A set left empty this turn keeps its place in `sets`, so the others keep theirs; it isn't played.
  const kept = (all: Placed[][]) => all.filter((set) => set.length > 0)
  const table = kept(sets).map(tilesOf)
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
  const held = [...rack, ...plans.flat(), ...parked]
  const verdict = canTable && held.length > 0 ? checkOkeyTurn(hand, turnOf(held[held.length - 1].tile)) : undefined
  const progress = myTurn && !opened ? okeyProgress({ view: hand, table }) : undefined
  // What the valid plans would add up to if laid down now, before opening.
  const planned = watching || opened || plans.length === 0
    ? undefined
    : okeyProgress({ view: hand, table: [...hand.table, ...plans.map(tilesOf).filter((plan) => okeySet(hand.okey, plan))] })
  const plannedNote =
    planned && (planned.points > 0 || planned.pairs > 0)
      ? planned.pairs > planned.points
        ? t('okey.plannedPairs', { pairs: planned.pairs, needed: hand.openingPairs })
        : t('okey.plannedPoints', { points: planned.points, needed: hand.openingPoints })
      : undefined
  // The tile this player last drew, marked wherever they keep it.
  const drawnId = okey.drawn && held.findLast((p) => sameTile(p.tile, okey.drawn!))?.id

  // Someone else's turn, shared live: their table so far. The table only
  // grows at its end, so a set keeps its place (and its index) throughout.
  const liveTable = !myTurn && draft && draft.by === hand.current && !hand.end ? draft.table : null
  const shown: Placed[][] = myTurn
    ? sets
    : (liveTable ?? hand.table).map((set, s) => set.map((tile, i) => ({ id: `v${s}-${i}`, tile })))
  const widths = shown.map((set) => footprint(set.length))
  const cells = resolve(
    shown.flatMap((set, s) => (set.length > 0 ? [{ key: s, width: widths[s], at: at[s] }] : [])),
    metrics?.columns ?? 12,
  )
  /** Every shown set's place: where the player put it, else where it's shown now. */
  const pinned = () => shown.map((_, s) => at[s] ?? cells.get(s))

  // The rack order, plans, the tile set aside and set places outlive the
  // state. A set new to this player keeps the place it was first shown in.
  useEffect(() => {
    const saved: SavedRack = {
      code: room.code,
      tiles: tilesOf(rack.filter((p) => p.id !== TAKEN)),
      hand: handKey,
      plans: plans.map(tilesOf),
      next: tilesOf(parked),
    }
    remember(RACK_KEY, saved, 'session')
  }, [rack, plans, parked, room.code, handKey])
  const places = JSON.stringify(shown.flatMap((set, s) => (set.length > 0 ? [at[s] ?? cells.get(s) ?? null] : [])))
  useEffect(() => {
    if (metrics) remember(TABLE_KEY, { code: room.code, played: view.played, at: JSON.parse(places) }, 'session')
  }, [metrics, places, room.code, view.played])

  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    // A short hold, so a swipe still scrolls.
    useSensor(TouchSensor, { activationConstraint: { delay: 150, tolerance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  type Where =
    | { in: 'rack'; at: number }
    | { in: 'plan'; plan: number; at: number }
    | { in: 'set'; set: number; at: number }
    | { in: 'offer' }
    | { in: 'parked' }
  /** Where a tile is: the rack, a plan, a set (and its place), set aside, or the offer. */
  const find = (id: string, where: Tiles = tiles): Where | null => {
    if (id === OFFER) return { in: 'offer' }
    if (where.parked.some((p) => p.id === id)) return { in: 'parked' }
    const at = where.rack.findIndex((p) => p.id === id)
    if (at >= 0) return { in: 'rack', at }
    for (const [plan, tiles] of where.plans.entries()) {
      const i = tiles.findIndex((p) => p.id === id)
      if (i >= 0) return { in: 'plan', plan, at: i }
    }
    for (const [set, tiles] of where.sets.entries()) {
      const i = tiles.findIndex((p) => p.id === id)
      if (i >= 0) return { in: 'set', set, at: i }
    }
    return null
  }

  const placedOf = (id: string): Placed | undefined => {
    if (id === OFFER) return offer && { id: TAKEN, tile: offer }
    return [...held, ...sets.flat()].find((p) => p.id === id)
  }

  /** The board without tile `id` (a plan left empty goes; a set keeps its place). */
  const without = (id: string): Tiles => ({
    rack: rack.filter((p) => p.id !== id),
    plans: plans.map((plan) => plan.filter((p) => p.id !== id)).filter((plan) => plan.length > 0),
    sets: sets.map((set) => set.filter((p) => p.id !== id)),
    parked: parked.filter((p) => p.id !== id),
  })

  /** Where a set `width` wide lands if let go at `cell`: there if it's free, else the nearest free place in that row. */
  const landing = (cell: Cell, width: number, except?: number) => {
    if (!metrics) return null
    const shownCells = new Map([...cells].filter(([key]) => key !== except))
    const grid = gridWithout(shownCells, new Map(widths.map((w, s) => [s, w])), metrics.columns)
    const free = grid.fits(cell, width) ? cell : grid.nearestInRow(cell, width)
    return free && { cell: free, width }
  }

  /** Where a set `width` wide goes when not dragged there: the first free place from the last row on. */
  const freeCell = (width: number) => {
    if (!metrics) return undefined
    const lastRow = Math.max(0, ...[...cells.values()].map((cell) => cell.row))
    return gridWithout(cells, new Map(widths.map((w, s) => [s, w])), metrics.columns).firstFree(width, lastRow)
  }

  /** `next`, with a new set of `set` added at `cell` (if known). */
  const withNewSet = (next: Tiles, set: Placed[], cell?: Cell) => {
    setAt([...pinned(), cell])
    return { ...next, sets: [...next.sets, set] }
  }

  const fits = (candidate: Placed[]) => okeyMeldKind(hand.okey, tilesOf(candidate)) !== undefined

  /** A plan put on the table whole, as a new set, once the player may lay tiles down. */
  const playPlan = (plan: number) => {
    const set = plans[plan]
    if (!canTable || !set) return
    touchTable(set[set.length - 1].id)
    update(withNewSet({ ...tiles, plans: plans.filter((_, i) => i !== plan) }, set, freeCell(footprint(set.length))))
  }

  /**
   * A double-clicked tile. From the rack, once the player may lay tiles
   * down: to where they last changed a set, the table or their plans. On
   * the table it joins the set last changed if it fits (a set already
   * there only at an end; a new one as the classic board does, or as a
   * pair), else starts a new set there; the discard taken always goes to
   * the table. At other times into the plan last changed if it fits, else
   * a new plan. A planned tile: its whole plan onto the table when it may
   * go, else back to the rack. This turn's tile on the table: back to the
   * rack.
   */
  const quickMove = (placed: Placed) => {
    const from = find(placed.id)
    if (watching || hand.end || !from || from.in === 'offer') return
    const left = without(placed.id)
    if (from.in === 'parked') return update({ ...left, rack: [...left.rack, placed] })
    if (from.in === 'plan') {
      if (canTable) return playPlan(from.plan)
      return update({ ...left, rack: [...left.rack, placed] })
    }
    if (from.in === 'set') {
      if (!canTable || fixed.has(placed.id)) return
      return update({ ...left, rack: [...left.rack, placed] })
    }
    const planned = lastPlan.current !== null && plans[lastPlan.current] !== undefined
    if (placed.id !== TAKEN && (!canTable || (lastIn.current === 'plans' && planned))) {
      const p = lastPlan.current
      const joined = p !== null && plans[p] ? joinNew(hand.okey, plans[p], placed) : null
      if (joined) {
        touchPlan(p!)
        return update({ ...left, plans: plans.map((plan, i) => (i === p ? joined : plan)) })
      }
      touchPlan(left.plans.length)
      return update({ ...left, plans: [...left.plans, [placed]] })
    }
    if (!canTable) return
    const last = lastPlaced.current === null ? null : find(lastPlaced.current)
    const s = last?.in === 'set' ? last.set : -1
    const set = sets[s]
    const joined = !set ? null : s < hand.table.length ? ([[...set, placed], [placed, ...set]].find(fits) ?? null) : joinNew(hand.okey, set, placed)
    touchTable(placed.id)
    if (joined) return update({ ...left, sets: left.sets.map((other, i) => (i === s ? joined : other)) })
    update(withNewSet(left, [placed], freeCell(footprint(1))))
  }

  const discard = (placed: Placed) => {
    const left = without(placed.id)
    const turn: Turn = { takeDiscard: taken, table: kept(left.sets).map(tilesOf), discard: placed.tile }
    const check = checkOkeyTurn(hand, turn)
    if (check.verdict === 'rejected') return setRefused({ kind: 'okey', error: check.error })
    update(left)
    send({ type: 'discard', ...turn })
  }

  const onDragStart = ({ active }: DragStartEvent) => {
    setRefused(null)
    const id = String(active.id)
    if (isSetItem(id)) return setDragging({ set: Number(id.slice('move-set-'.length)) })
    setDragging({ placed: placedOf(id) })
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
    let next: { cell: Cell; width: number } | null = null
    if (isSetItem(id)) {
      const s = Number(id.slice('move-set-'.length))
      next = landing({ row, col }, widths[s], s)
    } else if (over?.id === NEW_SET) {
      // The tile will be the set's first, after its grip.
      next = landing({ row, col: col - 1 }, footprint(1))
    }
    setTarget((t) =>
      t && next && t.width === next.width && t.cell.row === next.cell.row && t.cell.col === next.cell.col ? t : next,
    )
  }

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    const landed = target
    setDragging(null)
    setTarget(null)
    const id = String(active.id)
    if (isSetItem(id)) {
      const s = Number(id.slice('move-set-'.length))
      if (landed) setAt(pinned().map((cell, i) => (i === s ? landed.cell : cell)))
      return
    }
    if (!over) return
    const to = String(over.id)
    const from = find(id)
    const placed = placedOf(id)
    if (!from || !placed) return
    // A tile already on the table at the start of the turn stays there.
    if (from.in === 'set' && fixed.has(id)) return
    // Pointer left of a target's middle: its front.
    const translated = active.rect.current.translated
    const nearFront = translated !== null && translated.left + translated.width / 2 < over.rect.left + over.rect.width / 2

    // The discard taken goes back.
    if (to === OFFER) {
      if (id !== TAKEN) return
      update(without(TAKEN))
      setTaken(false)
      return
    }
    if (to === DISCARD) {
      if (from.in === 'offer' || !canTable) return
      return discard(placed)
    }
    // Set aside to discard next, any time: the player's own tiles, not the
    // discard taken; in order where let go, a fourth sending the last back.
    if (to === NEXT) {
      if (from.in === 'offer' || id === TAKEN || (from.in === 'set' && !canTable)) return
      const left = without(id)
      const x = translated ? translated.left + translated.width / 2 : over.rect.left + over.rect.width
      const { queue, out } = enqueue(left.parked, placed, x, over.rect)
      return update({ ...left, rack: out ? [...left.rack, out] : left.rack, parked: queue })
    }
    // The rack: put in order, or brought back from a plan or the table.
    const overRack = to === RACK || rack.some((p) => p.id === to)
    if (overRack) {
      if (from.in === 'rack') {
        const target = to === RACK ? rack.length - 1 : rack.findIndex((p) => p.id === to)
        setSort('custom')
        return update({ ...tiles, rack: arrayMove(rack, from.at, target) })
      }
      if (from.in === 'offer') setTaken(true)
      const left = from.in === 'offer' ? tiles : without(id)
      const at = to === RACK ? left.rack.length : left.rack.findIndex((p) => p.id === to)
      const next = [...left.rack]
      next.splice(at < 0 ? next.length : at, 0, placed)
      return update({ ...left, rack: next })
    }
    // The plans: any time, the player's own tiles (not the discard taken).
    if (to === NEW_PLAN || isPlanTarget(to)) {
      if (from.in === 'offer' || id === TAKEN || (from.in === 'set' && !canTable)) return
      const left = without(id)
      if (to === NEW_PLAN) {
        touchPlan(left.plans.length)
        return update({ ...left, plans: [...left.plans, [placed]] })
      }
      // The plan's place once the tile has left its old one, which goes if left empty.
      let p = Number(to.slice('plan-'.length))
      if (from.in === 'plan' && from.plan < p && plans[from.plan].length === 1) p--
      if (!left.plans[p]) return update({ ...left, plans: [...left.plans, [placed]] })
      touchPlan(p)
      return update({ ...left, plans: left.plans.map((plan, i) => (i === p ? insertAt(plan, placed, nearFront) : plan)) })
    }
    // Onto the table: only on the player's turn, and only tiles of this turn.
    if (!myTurn) return
    if (from.in === 'offer') {
      if (hand.phase !== 'draw') return
    } else if (!canTable) {
      return
    }
    const left = from.in === 'offer' ? tiles : without(id)
    if (from.in === 'offer') setTaken(true)
    touchTable(placed.id)
    if (to === NEW_SET) return update(withNewSet(left, [placed], landed?.cell))
    if (!isSetTarget(to)) return
    const s = Number(to.slice('set-'.length))
    const set = left.sets[s]
    if (!set) return
    // At the end it fits, the nearer one if both do; else in an okey's place,
    // taking the okey back to the rack; else at the nearer end, to fix.
    const atFront = [placed, ...set]
    const atBack = [...set, placed]
    const okeyAt = set.findIndex((p, i) => {
      if (!sameTile(p.tile, hand.okey) || !fixed.has(p.id)) return false
      return fits(set.map((other, j) => (j === i ? placed : other)))
    })
    let nextSet: Placed[]
    let nextRack = left.rack
    if (fits(nearFront ? atFront : atBack)) nextSet = nearFront ? atFront : atBack
    else if (fits(nearFront ? atBack : atFront)) nextSet = nearFront ? atBack : atFront
    else if (okeyAt >= 0 && s < hand.table.length && opened) {
      nextSet = set.map((p, i) => (i === okeyAt ? placed : p))
      nextRack = [...left.rack, { id: `okey-${set[okeyAt].id}`, tile: set[okeyAt].tile }]
    } else nextSet = nearFront ? atFront : atBack
    update({ ...left, rack: nextRack, sets: left.sets.map((other, i) => (i === s ? nextSet : other)) })
  }

  // This turn's tiles back to the rack (the discard taken to its pile); plans stay.
  const reset = () => {
    const laid = sets.flat().filter((p) => !fixed.has(p.id) && p.id !== TAKEN && !p.id.startsWith('okey-'))
    const okeys = rack.filter((p) => p.id.startsWith('okey-'))
    update({
      rack: [...rack.filter((p) => p.id !== TAKEN && !okeys.includes(p)), ...laid],
      plans,
      parked,
      sets: hand.table.map((set, s) => set.map((tile, i) => ({ id: `t${s}-${i}`, tile }))),
    })
    setAt(at.slice(0, hand.table.length))
    setTaken(false)
    setRefused(null)
    touchTable(null)
  }

  const sortRack = (by: 'number' | 'color') => {
    setSort(by)
    remember('rummy.rackSort', by)
    update({ ...tiles, rack: [...rack].sort((a, b) => compareTiles(a.tile, b.tile, by)) })
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
    return { tone: 'neutral', text: opened ? t('okey.yourTurn') : t('okey.yourTurnFirst', { points: hand.openingPoints }) }
  })()
  const noteColor = { neutral: 'text-slate-300', good: 'text-emerald-300', bad: 'text-amber-300' }[note.tone]

  const handNumber = Math.min(hand.rules.hands ?? view.played + 1, view.played + (hand.end ? 0 : 1))

  return (
    <FalseOkeyMark value={room.options.falseOkeyMarks ? hand.okey : undefined}>
      <DndContext
        sensors={sensors}
        collisionDetection={collisions}
        onDragStart={onDragStart}
        onDragMove={onDragMove}
        onDragEnd={onDragEnd}
        onDragCancel={() => {
          setDragging(null)
          setTarget(null)
        }}
      >
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
                  indicator={hand.indicator}
                  okey={hand.okey}
                  pool={hand.pool}
                  caption={t('okey.hand', { n: handNumber, total: hand.rules.hands ?? handNumber })}
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

              <div className="relative min-h-0 flex-1">
                <TableGrid
                  gridRef={tableGrid}
                  droppable={canTable || (canTake && !taken)}
                  height={metrics ? (Math.max(0, ...[...cells.values()].map((cell) => cell.row + 1)) + 1) * metrics.cellH : undefined}
                >
                  {kept(shown).length === 0 && (
                    <p dir="auto" className="pointer-events-none absolute inset-0 grid place-items-center text-center text-emerald-100/40">
                      {canTable || (canTake && !taken) ? t('board.dropToStart') : t('okey.emptyTable')}
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
                    shown.map(
                      (set, s) =>
                        set.length > 0 && (
                          <TableSet
                            key={s}
                            index={s}
                            tiles={set}
                            okey={hand.okey}
                            at={cells.get(s)!}
                            metrics={metrics}
                            droppable={canTable || (canTake && !taken)}
                            draggable={myTurn ? (id) => !fixed.has(id) : () => false}
                            onQuick={quickMove}
                            fresh={myTurn && (s >= hand.table.length || !same([tilesOf(set)], [hand.table[s]]))}
                            live={liveTable !== null && (s >= hand.table.length || !same([tilesOf(set)], [hand.table[s] ?? []]))}
                          />
                        ),
                    )}
                </TableGrid>
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
              <Plans
                plans={plans}
                okey={hand.okey}
                valid={okeySet}
                canPlay={canTable}
                note={plannedNote}
                drawnId={drawnId}
                onQuick={quickMove}
                onPlay={playPlan}
              />
              <div className="flex items-stretch gap-3">
                <Rack tiles={rack} sortable={!watching} okey={hand.okey} drawnId={drawnId} onQuick={quickMove} />
                <NextSpot tiles={parked} okey={hand.okey} drawnId={drawnId} onQuick={quickMove} />
                <DiscardSpot id={DISCARD} label={t('okey.discardHere')} active={myTurn && (hand.phase === 'play' || taken)} />
              </div>
            </section>
          )}
        </div>

        <DragOverlay dropAnimation={{ duration: 180, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)' }}>
          {dragging?.placed ? (
            <TileFace tile={dragging.placed.tile} size="rack" lifted />
          ) : dragging?.set !== undefined && shown[dragging.set] ? (
            <div dir="ltr" className="flex items-center gap-1 pl-4">
              {shown[dragging.set].map((p) => (
                <TileFace key={p.id} tile={p.tile} size="table" lifted />
              ))}
            </div>
          ) : null}
        </DragOverlay>
      </DndContext>
    </FalseOkeyMark>
  )
}

/** The indicator, the okey it picks, the pool to draw from, and a caption (the hand's number). */
export function Middle({
  indicator,
  okey,
  pool,
  caption,
  canDraw,
  onDraw,
}: {
  indicator: Tile
  okey: Tile
  pool: number
  caption: string
  canDraw: boolean
  onDraw: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-3 rounded-2xl bg-black/20 p-2 pe-3">
      <Labelled label={t('okey.indicator')}>
        <TileFace tile={indicator} size="table" />
      </Labelled>
      <Labelled label={t('okey.okey')}>
        <div className="rounded-md ring-2 ring-amber-300/80">
          <TileFace tile={okey} size="table" />
        </div>
      </Labelled>
      <Labelled label={t('board.draw')}>
        <button
          onClick={onDraw}
          disabled={!canDraw}
          data-pool
          title={t('board.pool', { count: pool })}
          className={`relative grid h-[3.25rem] w-9 place-items-center rounded-md bg-linear-to-b from-stone-100 to-stone-300 text-sm font-bold tabular-nums text-slate-700 shadow sm:h-16 sm:w-11 short:h-11 short:w-8 ${
            canDraw ? 'cursor-pointer ring-2 ring-teal-300 hover:brightness-105' : 'cursor-default opacity-80'
          }`}
        >
          {pool}
        </button>
      </Labelled>
      <p className="ms-1 max-w-28 text-xs leading-tight text-emerald-100/70">{caption}</p>
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
export function DiscardPile({ name, pile, offered, returnable }: { name: string; pile: Tile[]; offered: boolean; returnable: boolean }) {
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

/**
 * A set on the table, at its cell: tiles may join it at either end; this
 * turn's tiles move again; the whole set moves by its grip, any time (only
 * for this player, as the table's order never changes).
 */
function TableSet({
  index,
  tiles,
  okey,
  at,
  metrics,
  droppable,
  draggable,
  onQuick,
  fresh,
  live,
}: {
  index: number
  tiles: Placed[]
  okey: Tile
  at: Cell
  metrics: Metrics
  droppable: boolean
  draggable: (id: string) => boolean
  /** A double-clicked tile of this turn: back to the rack. */
  onQuick: (placed: Placed) => void
  fresh: boolean
  live: boolean
}) {
  const { t } = useTranslation()
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: setItem(index) })
  const { setNodeRef: setHolderRef, isOver } = useDroppable({ id: setTarget(index), disabled: !droppable })
  const valid = okeyMeldKind(okey, tilesOf(tiles)) !== undefined
  const ring = !valid ? 'ring-2 ring-amber-400' : fresh ? 'ring-2 ring-emerald-300/80' : live ? 'ring-2 ring-sky-300/70 animate-pulse' : ''
  return (
    <div
      ref={setNodeRef}
      style={{ left: at.col * metrics.cellW, top: at.row * metrics.cellH }}
      className={`absolute flex items-stretch rounded-lg transition-[left,top,background-color] duration-200 ${ring} ${
        isDragging ? 'opacity-30' : ''
      }`}
    >
      <button
        {...attributes}
        {...listeners}
        aria-label={t('board.moveSet')}
        aria-roledescription={t('board.setRole')}
        className="grid w-4 cursor-grab touch-manipulation place-items-center rounded-l-lg text-emerald-100/25 transition hover:bg-white/5 hover:text-emerald-100/70 active:cursor-grabbing"
      >
        <svg viewBox="0 0 6 16" className="h-4 w-1.5" fill="currentColor" aria-hidden>
          {[2, 8, 14].map((y) => [1, 5].map((x) => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.1" />))}
        </svg>
      </button>
      <div ref={setHolderRef} className={`flex items-center gap-1 rounded-r-lg py-1 pe-1 ${isOver ? 'bg-white/15' : ''}`}>
        {tiles.map((placed) => (
          <TableTile
            key={placed.id}
            placed={placed}
            draggable={draggable(placed.id)}
            okey={okey}
            onDoubleClick={draggable(placed.id) ? () => onQuick(placed) : undefined}
          />
        ))}
      </div>
    </div>
  )
}

function TableTile({
  placed,
  draggable,
  okey,
  size = 'table',
  drawn = false,
  onDoubleClick,
}: {
  placed: Placed
  draggable: boolean
  okey: Tile
  size?: TileSize
  drawn?: boolean
  onDoubleClick?: () => void
}) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: placed.id, disabled: !draggable })
  const isOkey = sameTile(placed.tile, okey)
  return (
    <div
      ref={setNodeRef}
      {...(draggable ? { ...attributes, ...listeners } : {})}
      onDoubleClick={onDoubleClick}
      className={`${draggable ? 'cursor-grab' : ''} ${isDragging ? 'opacity-30' : ''} ${isOkey ? 'rounded-md ring-2 ring-amber-300/70' : ''}`}
    >
      <TileFace tile={placed.tile} size={size} drawn={drawn} />
    </div>
  )
}

/**
 * Sets the player plans from their rack, above it: built any time by
 * dragging or double-clicking. In Okey 101, put on the table whole once the
 * player may lay tiles down (`canPlay`: its ↑ button, or a double-click),
 * with what the valid ones would count towards opening (`note`). Plain
 * Okey has no table: its plans only arrange the rack. `valid`: the rule
 * set's sets, for the outline.
 */
export function Plans({
  plans,
  okey,
  valid,
  canPlay,
  note,
  drawnId,
  onQuick,
  onPlay,
}: {
  plans: Placed[][]
  okey: Tile
  valid: (okey: Tile, tiles: Tile[]) => boolean
  canPlay: boolean
  note?: string
  /** The tile this player last drew, to mark. */
  drawnId?: string
  onQuick: (placed: Placed) => void
  onPlay?: (plan: number) => void
}) {
  const { t } = useTranslation()
  const { setNodeRef, isOver } = useDroppable({ id: NEW_PLAN })
  return (
    <div
      ref={setNodeRef}
      dir="ltr"
      className={`mb-2 flex min-h-11 flex-wrap items-center gap-2 rounded-2xl border border-dashed px-2 py-1.5 transition short:mb-1.5 ${
        isOver ? 'border-amber-100/50 bg-amber-100/10' : 'border-amber-100/15'
      }`}
    >
      {plans.length === 0 && (
        <p dir="auto" className="pointer-events-none px-1 text-xs text-amber-100/40">
          {t('board.plansHint')}
        </p>
      )}
      {plans.map((plan, p) => (
        <Plan
          key={plan[0].id}
          index={p}
          tiles={plan}
          okey={okey}
          valid={valid(okey, tilesOf(plan))}
          canPlay={canPlay && onPlay !== undefined}
          drawnId={drawnId}
          onQuick={onQuick}
          onPlay={() => onPlay?.(p)}
        />
      ))}
      {note && (
        <p dir="auto" className="ms-auto px-1 text-xs font-medium tabular-nums text-amber-100/70">
          {note}
        </p>
      )}
    </div>
  )
}

function Plan({
  index,
  tiles,
  okey,
  valid,
  canPlay,
  drawnId,
  onQuick,
  onPlay,
}: {
  index: number
  tiles: Placed[]
  okey: Tile
  valid: boolean
  canPlay: boolean
  drawnId?: string
  onQuick: (placed: Placed) => void
  onPlay: () => void
}) {
  const { t } = useTranslation()
  const { setNodeRef, isOver } = useDroppable({ id: planTarget(index) })
  return (
    <div
      ref={setNodeRef}
      title={valid ? t('board.planValid') : t('board.planInvalid')}
      className={`flex items-center gap-1 rounded-xl p-1 ring-1 ${valid ? 'bg-emerald-300/10 ring-emerald-300/50' : 'ring-amber-100/20'} ${
        isOver ? 'bg-white/10' : ''
      }`}
    >
      {tiles.map((placed) => (
        <TableTile
          key={placed.id}
          placed={placed}
          draggable
          okey={okey}
          drawn={placed.id === drawnId}
          onDoubleClick={() => onQuick(placed)}
        />
      ))}
      {canPlay && (
        <button
          className="ms-0.5 grid size-7 cursor-pointer place-items-center rounded-lg text-amber-100/70 hover:bg-white/10 hover:text-amber-50"
          title={t('board.playPlan')}
          aria-label={t('board.playPlan')}
          onClick={onPlay}
        >
          ↑
        </button>
      )}
    </div>
  )
}

/** The rack: one row that wraps, sorted by dragging. `onQuick`: a tile double-clicked. */
export function Rack({
  tiles,
  sortable,
  okey,
  drawnId,
  onQuick,
}: {
  tiles: Placed[]
  sortable: boolean
  okey: Tile
  /** The tile this player last drew, to mark. */
  drawnId?: string
  onQuick?: (placed: Placed) => void
}) {
  const { setNodeRef } = useDroppable({ id: RACK })
  return (
    <SortableContext items={tiles.map((p) => p.id)} strategy={rectSortingStrategy} disabled={!sortable}>
      <div ref={setNodeRef} dir="ltr" className="flex min-h-16 min-w-0 flex-1 flex-wrap content-start gap-1.5 sm:min-h-[4.75rem] short:min-h-12">
        {tiles.map((placed) => (
          <RackTile
            key={placed.id}
            placed={placed}
            okey={okey}
            drawn={placed.id === drawnId}
            onDoubleClick={onQuick && (() => onQuick(placed))}
          />
        ))}
      </div>
    </SortableContext>
  )
}

function RackTile({
  placed,
  okey,
  drawn,
  onDoubleClick,
}: {
  placed: Placed
  okey: Tile
  drawn: boolean
  onDoubleClick?: () => void
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: placed.id })
  const { t } = useTranslation()
  const isOkey = sameTile(placed.tile, okey)
  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      {...attributes}
      {...listeners}
      onDoubleClick={onDoubleClick}
      className={`cursor-grab touch-none rounded-lg ${isDragging ? 'opacity-30' : ''} ${
        placed.id === TAKEN ? 'ring-2 ring-teal-300' : isOkey ? 'ring-2 ring-amber-300/70' : ''
      }`}
      title={placed.id === TAKEN ? t('okey.takenTile') : undefined}
    >
      <TileFace tile={placed.tile} size="rack" drawn={drawn} />
    </div>
  )
}

/**
 * The tiles set aside to discard next, beside the discard spot, the first
 * nearest it: if the player's time runs out the first they hold is thrown
 * (in Okey 101 the first that costs no penalty). Any time, up to `QUEUE`;
 * dragged out, or double-clicked back to the rack.
 */
export function NextSpot({
  tiles,
  okey,
  drawnId,
  onQuick,
}: {
  tiles: Placed[]
  okey: Tile
  drawnId?: string
  onQuick: (placed: Placed) => void
}) {
  const { t } = useTranslation()
  const { setNodeRef, isOver } = useDroppable({ id: NEXT })
  return (
    <div
      ref={setNodeRef}
      dir="auto"
      title={t('okey.discardNextDetail')}
      className={`flex min-w-14 shrink-0 items-center justify-center gap-1 rounded-2xl border-2 border-dashed p-1 text-center text-[0.65rem] leading-tight transition sm:min-w-16 ${
        isOver ? 'border-sky-200 bg-sky-200/15 text-sky-100' : 'border-sky-200/30 text-sky-100/60'
      }`}
    >
      {tiles.length === 0 && t('okey.discardNext')}
      {/* Right to left: the first to go nearest the discard spot. */}
      {[...tiles].reverse().map((tile) => (
        <TableTile
          key={tile.id}
          placed={tile}
          draggable
          okey={okey}
          size="rack"
          drawn={tile.id === drawnId}
          onDoubleClick={() => onQuick(tile)}
        />
      ))}
    </div>
  )
}

/** Where a tile dropped is discarded, ending the turn (`id` DISCARD, or FINISH in plain Okey). */
export function DiscardSpot({
  id,
  label,
  active,
  className = 'w-20 sm:w-28',
}: {
  id: string
  label: string
  active: boolean
  className?: string
}) {
  const { setNodeRef, isOver } = useDroppable({ id, disabled: !active })
  return (
    <div
      ref={setNodeRef}
      dir="auto"
      className={`grid shrink-0 place-items-center rounded-2xl border-2 border-dashed p-2 text-center text-xs transition ${className} ${
        active
          ? isOver
            ? 'border-amber-200 bg-amber-200/15 text-amber-100'
            : 'border-amber-200/50 text-amber-100/80'
          : 'border-white/10 text-slate-500'
      }`}
    >
      {label}
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

export function Overlay({ children }: { children: ReactNode }) {
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

import { closestCenter, pointerWithin, type CollisionDetection } from '@dnd-kit/core'
import { createContext } from 'react'

import { arrangeRack, joinAt, keepPlans, without, type RackSort } from './arrange.ts'
import { remembered } from './storage.ts'
import { okeyMeldKind, type Tile } from './wasm/engine.js'

/** What the Okey boards (Okey 101 and plain Okey) share besides components. */

/** A tile where a board shows it, with an id for dragging. */
export interface Placed {
  id: string
  tile: Tile
}

export const RACK = 'rack'
export const NEW_SET = 'new-set'
export const DISCARD = 'discard'
/** Plain Okey: where the last tile goes to finish. */
export const FINISH = 'finish'
/** The previous player's discard, to take. */
export const OFFER = 'offer'
/** The discard once taken, wherever it is. */
export const TAKEN = 'taken'
export const setTarget = (index: number) => `set-${index}`
export const isSetTarget = (id: string) => id.startsWith('set-')
/** A set's grip, to move it around the table (this player's view only). */
export const setItem = (index: number) => `move-set-${index}`
export const isSetItem = (id: string) => id.startsWith('move-set-')
/** The tiles set aside to discard next, beside the discard spot. */
export const NEXT = 'next'
/** The most tiles set aside to discard next (the server keeps as many). */
export const QUEUE = 3

/**
 * The tiles set aside to discard next (first to go first), with `tile` put
 * in where it was let go over their slot: `x`, the dragged tile's middle,
 * across the slot's `rect`. The slot shows them right to left, the first
 * nearest the discard spot. Past `QUEUE`, the last goes back (`out`).
 */
export function enqueue(queue: Placed[], tile: Placed, x: number, rect: { left: number; width: number }): { queue: Placed[]; out?: Placed } {
  const n = queue.length
  const fromLeft = Math.min(n, Math.max(0, Math.round(((x - rect.left) / rect.width) * n)))
  const at = n - fromLeft
  const next = [...queue.slice(0, at), tile, ...queue.slice(at)]
  return next.length > QUEUE ? { queue: next.slice(0, QUEUE), out: next[QUEUE] } : { queue: next }
}
/** The plans strip, where a tile dropped starts a plan; and one plan. */
export const NEW_PLAN = 'new-plan'
export const planTarget = (index: number) => `plan-${index}`
export const isPlanTarget = (id: string) => id.startsWith('plan-')

export const collisions: CollisionDetection = (args) => {
  // A set lands wherever it's let go (see OkeyBoard's onDragMove), not on anything.
  if (isSetItem(String(args.active.id))) return []
  // Under the pointer, a set or plan before the empty table or strip around it.
  const within = pointerWithin(args)
  const wide = (id: string) => (id === NEW_SET || id === NEW_PLAN ? 1 : 0)
  if (within.length > 0) return [...within].sort((a, b) => wide(String(a.id)) - wide(String(b.id)))
  return closestCenter(args)
}

export const tilesOf = (placed: Placed[]) => placed.map((p) => p.tile)
export const same = (a: Tile[][], b: Tile[][]) => JSON.stringify(a) === JSON.stringify(b)
export const sameTile = (a: Tile, b: Tile) => JSON.stringify(a) === JSON.stringify(b)

/**
 * Okey: the tile false okeys stand for (the okey's), set by an Okey board
 * when its room marks them (`falseOkeyMarks`); each joker face then shows
 * it in a corner. Unset, a joker is plain.
 */
export const FalseOkeyMark = createContext<Tile | undefined>(undefined)

/**
 * The rack in the player's order, remembered for the tab under `key`: as
 * they last left it in room `code`, new tiles at the end.
 */
export function orderedRack(key: string, code: string, rack: Tile[], sort: RackSort): Placed[] {
  const saved = remembered<{ code: string; tiles: Tile[] }>(key, 'session')
  const previous = saved && saved.code === code ? saved.tiles : []
  const [first, second] = arrangeRack([previous, []], sort, rack)
  return [...first, ...second].map((tile, i) => ({ id: `r${i}`, tile }))
}

/** What an Okey board keeps for the tab: the rack's order, and for the hand `hand` its plans and the tiles set aside. */
export interface SavedRack {
  code: string
  tiles: Tile[]
  hand?: string
  plans?: Tile[][]
  next?: Tile[]
}

/**
 * The rack, the plans and the tiles set aside to discard next as the
 * player left them in room `code` (saved under `key`), for the tiles now on
 * `rack`. Plans and the tiles set aside belong to one hand (`hand`).
 */
export function rackAndPlans(
  key: string,
  code: string,
  hand: string,
  rack: Tile[],
  sort: RackSort,
): { rack: Placed[]; plans: Placed[][]; parked: Placed[] } {
  const saved = remembered<SavedRack>(key, 'session')
  const same = saved?.code === code && saved.hand === hand
  const planned = keepPlans(same ? (saved.plans ?? []) : [], rack)
  const left = without(rack, planned.flat())
  // Those still held, in order (an older save kept one tile, not a list).
  const wanted = same && saved.next ? (Array.isArray(saved.next) ? saved.next : [saved.next]) : []
  const next = keepPlans([wanted], left)[0] ?? []
  let n = 0
  return {
    rack: orderedRack(key, code, without(left, next), sort),
    plans: planned.map((plan) => plan.map((tile) => ({ id: `p${n++}`, tile }))),
    parked: next.map((tile, i) => ({ id: `next${i}`, tile })),
  }
}

/** A tile as the classic sets read it: the okey wild, a false okey the okey's own tile. */
const asClassic = (okey: Tile, tile: Tile): Tile => (sameTile(tile, okey) ? { kind: 'joker' } : tile.kind === 'joker' ? okey : tile)

/** `set` (being built, not on the table) with `tile` where it joins: as the classic board places it, or as a pair. */
export function joinNew(okey: Tile, set: Placed[], tile: Placed): Placed[] | null {
  const pair = set.length === 1 && sameTile(set[0].tile, tile.tile)
  const at = pair ? 1 : joinAt(set.map((p) => asClassic(okey, p.tile)), asClassic(okey, tile.tile))
  return at === null ? null : [...set.slice(0, at), tile, ...set.slice(at)]
}

/** Whether `tiles` are a run, group or pair of Okey 101, the okey wild. */
export const okeySet = (okey: Tile, tiles: Tile[]) => okeyMeldKind(okey, tiles) !== undefined

/**
 * Whether `tiles` are a set of plain Okey: as Okey 101's, or a run ending
 * 12 13 1 (1 high; not 13 1 2). Checked as the run one lower, ending on 13.
 */
export function plainSet(okey: Tile, tiles: Tile[]): boolean {
  if (okeySet(okey, tiles)) return true
  const last = tiles.at(-1)
  if (tiles.length < 3 || last?.kind !== 'number' || last.value !== 1 || sameTile(last, okey)) return false
  const lower = tiles.map((tile): Tile => {
    if (sameTile(tile, okey) || tile.kind !== 'number') return tile
    return { ...tile, value: tile.value === 1 ? 13 : tile.value - 1 }
  })
  return okeySet(okey, lower)
}

import { closestCenter, pointerWithin, type CollisionDetection } from '@dnd-kit/core'
import { createContext } from 'react'

import { arrangeRack, type RackSort } from './arrange.ts'
import { remembered } from './storage.ts'
import type { Tile } from './wasm/engine.js'

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

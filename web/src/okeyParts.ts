import { closestCenter, pointerWithin, type CollisionDetection } from '@dnd-kit/core'

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

export const collisions: CollisionDetection = (args) => {
  const within = pointerWithin(args)
  return within.length > 0 ? within : closestCenter(args)
}

export const tilesOf = (placed: Placed[]) => placed.map((p) => p.tile)
export const same = (a: Tile[][], b: Tile[][]) => JSON.stringify(a) === JSON.stringify(b)
export const sameTile = (a: Tile, b: Tile) => JSON.stringify(a) === JSON.stringify(b)

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

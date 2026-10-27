import type { Cell } from './grid.ts'
import type { Tile } from './wasm/engine.js'

/**
 * The player's own arrangement of their rack and of the table's sets. It
 * only changes what this player sees, and survives new states from the
 * server: tiles and sets keep their places as far as they still exist.
 */

/** How the rack was last put in order: a sort, or by hand. */
export type RackSort = 'custom' | 'number' | 'color'

/** A set on the table, and where this player put it (see grid.ts). */
export interface PlacedSet {
  tiles: Tile[]
  at?: Cell
}

export interface Arrangement {
  /** The rack's two rows (a flat rack shows them one after the other). */
  rows: [Tile[], Tile[]]
  sort: RackSort
  /** The table's sets and where this player put them. */
  table: PlacedSet[]
  /** Sets the player is planning from their rack, not yet played. */
  plans?: Tile[][]
}

export const emptyArrangement: Arrangement = { rows: [[], []], sort: 'custom', table: [], plans: [] }

/** How a tile is read out: "red 7", "Joker". */
export function tileName(tile: Tile): string {
  return tile.kind === 'joker' ? 'Joker' : `${tile.color} ${tile.value}`
}

const keyOf = (tile: Tile) => (tile.kind === 'joker' ? 'joker' : `${tile.color} ${tile.value}`)

const colorOrder = { black: 0, blue: 1, orange: 2, red: 3 }

/** Jokers last. */
function sortKey(tile: Tile, by: 'number' | 'color'): number {
  if (tile.kind === 'joker') return 1000
  return by === 'number' ? tile.value * 10 + colorOrder[tile.color] : colorOrder[tile.color] * 100 + tile.value
}

export function compareTiles(a: Tile, b: Tile, by: 'number' | 'color'): number {
  return sortKey(a, by) - sortKey(b, by)
}

export function sortTiles(tiles: Tile[], by: 'number' | 'color'): Tile[] {
  return [...tiles].sort((a, b) => compareTiles(a, b, by))
}

/** Items over two rows, the first row getting the odd one. */
export function splitRows<T>(items: T[]): [T[], T[]] {
  const half = Math.ceil(items.length / 2)
  return [items.slice(0, half), items.slice(half)]
}

/** How many of each tile. */
function counts(tiles: Tile[]): Map<string, number> {
  const counts = new Map<string, number>()
  for (const tile of tiles) counts.set(keyOf(tile), (counts.get(keyOf(tile)) ?? 0) + 1)
  return counts
}

/** Takes one of `tile` from `counts`, if there is one. */
function take(counts: Map<string, number>, tile: Tile): boolean {
  const n = counts.get(keyOf(tile)) ?? 0
  if (n === 0) return false
  counts.set(keyOf(tile), n - 1)
  return true
}

/** The tiles of `tiles` beyond those in `taken`, counting repeats. */
export function without(tiles: Tile[], taken: Tile[]): Tile[] {
  const left = counts(taken)
  return tiles.filter((tile) => !take(left, tile))
}

/**
 * The player's planned sets that are still on `rack` (tiles played or gone
 * leave them; a plan left empty goes).
 */
export function keepPlans(plans: Tile[][], rack: Tile[]): Tile[][] {
  const available = counts(rack)
  return plans.map((plan) => plan.filter((tile) => take(available, tile))).filter((plan) => plan.length > 0)
}

/**
 * Whether `tiles`, in this order, are a set or could become one with more
 * tiles at either end: a run of one colour going up by one (within 1-13), or
 * up to four of a number in different colours. Jokers fit anywhere.
 */
export function couldBeSet(tiles: Tile[]): boolean {
  const numbers = tiles.flatMap((tile, place) => (tile.kind === 'number' ? [{ ...tile, place }] : []))
  if (numbers.length === 0) return tiles.length <= 13
  const first = numbers[0]
  const start = first.value - first.place
  const run =
    start >= 1 &&
    start + tiles.length - 1 <= 13 &&
    numbers.every((n) => n.color === first.color && n.value === start + n.place)
  const group =
    tiles.length <= 4 &&
    numbers.every((n) => n.value === first.value) &&
    new Set(numbers.map((n) => n.color)).size === numbers.length
  return run || group
}

/**
 * Where `tile` can join `set`, if it can: at an end if the set could then
 * still become one, else (a set of numbers of one colour) at its place by
 * number, leaving gaps to fill later.
 */
export function joinAt(set: Tile[], tile: Tile): number | null {
  if (couldBeSet([...set, tile])) return set.length
  if (couldBeSet([tile, ...set])) return 0
  if (tile.kind !== 'number') return null
  const sameColour = set.every((t) => t.kind === 'number' && t.color === tile.color && t.value !== tile.value)
  if (set.length === 0 || !sameColour) return null
  const at = set.findIndex((t) => t.kind === 'number' && t.value > tile.value)
  return at < 0 ? set.length : at
}

/**
 * The rack `rack` laid out in two rows the way the player had it: sorted
 * again if they last sorted it; otherwise tiles keep their places and new
 * ones (a drawn tile) go at the end.
 */
export function arrangeRack(previous: [Tile[], Tile[]], sort: RackSort, rack: Tile[]): [Tile[], Tile[]] {
  if (sort !== 'custom') return splitRows(sortTiles(rack, sort))
  if (previous[0].length + previous[1].length === 0) return splitRows(rack)
  const available = counts(rack)
  const rows: [Tile[], Tile[]] = [
    previous[0].filter((t) => take(available, t)),
    previous[1].filter((t) => take(available, t)),
  ]
  const added = rack.filter((t) => take(available, t))
  rows[1].push(...added)
  return rows
}

/** Tiles two sets have in common. */
function overlap(a: Tile[], b: Tile[]): number {
  const available = counts(b)
  return a.filter((t) => take(available, t)).length
}

const same = (a: Tile[], b: Tile[]) => a.length === b.length && a.every((t, i) => keyOf(t) === keyOf(b[i]))

/**
 * The table's sets, placed where the player had them. A set keeps its place
 * if it's still there, or if a set there now shares tiles with it (another
 * player extended or split it); sets new to this player have no place yet
 * and come last.
 */
export function arrangeTable(previous: PlacedSet[], table: Tile[][]): PlacedSet[] {
  const unplaced = new Set(table.keys())
  const placed: (number | undefined)[] = previous.map(() => undefined)
  // Unchanged sets first, so a changed one can't take their place.
  previous.forEach(({ tiles: set }, i) => {
    const match = [...unplaced].find((j) => same(set, table[j]))
    if (match !== undefined) {
      placed[i] = match
      unplaced.delete(match)
    }
  })
  previous.forEach(({ tiles: set }, i) => {
    if (placed[i] !== undefined) return
    let best: number | undefined
    let bestOverlap = 0
    for (const j of unplaced) {
      const o = overlap(set, table[j])
      if (o > bestOverlap) [best, bestOverlap] = [j, o]
    }
    if (best !== undefined) {
      placed[i] = best
      unplaced.delete(best)
    }
  })
  const kept = placed.flatMap((j, i) => (j === undefined ? [] : [{ tiles: table[j], at: previous[i].at }]))
  return [...kept, ...[...unplaced].map((j) => ({ tiles: table[j] }))]
}

/**
 * The sets in `after` that weren't in `before` as they are now: new, or
 * changed. Only content counts, not order on the table.
 */
export function newOrChanged(before: Tile[][], after: Tile[][]): Tile[][] {
  const unused = [...before]
  return after.filter((set) => {
    const i = unused.findIndex((old) => same(old, set))
    if (i < 0) return true
    unused.splice(i, 1)
    return false
  })
}

/**
 * Where sets lie on the table. The table is a grid of cells one tile wide
 * and one row high; a set takes a cell for its grip and one per tile, along
 * a row. Each player places sets where they like; this works out where they
 * can actually be shown, on a table of a given width.
 */

export interface Cell {
  row: number
  col: number
}

/** Cells a set of `tiles` takes: its grip, then its tiles. */
export const footprint = (tiles: number) => tiles + 1

/** The taken cells of a table `columns` wide. */
export class Grid {
  private taken = new Set<string>()
  readonly columns: number

  constructor(columns: number) {
    this.columns = columns
  }

  /** The last column a set `width` wide may start at (one wider than the table still fits, at the start). */
  private lastCol(width: number) {
    return Math.max(this.columns, width) - width
  }

  fits(cell: Cell, width: number): boolean {
    if (cell.row < 0 || cell.col < 0 || cell.col > this.lastCol(width)) return false
    for (let col = cell.col; col < cell.col + width; col++) if (this.taken.has(`${cell.row}:${col}`)) return false
    return true
  }

  take(cell: Cell, width: number) {
    for (let col = cell.col; col < cell.col + width; col++) this.taken.add(`${cell.row}:${col}`)
  }

  /** The first place a set fits, reading rows from `fromRow` on. */
  firstFree(width: number, fromRow = 0): Cell {
    for (let row = Math.max(0, fromRow); ; row++) {
      for (let col = 0; col <= this.lastCol(width); col++) if (this.fits({ row, col }, width)) return { row, col }
    }
  }

  /** The free place in `cell`'s row nearest to it, if the row has room. */
  nearestInRow(cell: Cell, width: number): Cell | null {
    const row = Math.max(0, cell.row)
    const want = Math.min(Math.max(0, cell.col), this.lastCol(width))
    for (let distance = 0; distance <= this.lastCol(width); distance++) {
      for (const col of [want - distance, want + distance]) {
        if (this.fits({ row, col }, width)) return { row, col }
      }
    }
    return null
  }
}

export interface Placement {
  key: number
  width: number
  /** Where the player put it, if anywhere yet. */
  at?: Cell
}

/**
 * Where each set is shown on a table `columns` wide: where it was put, if it
 * fits there and the place is free (sets higher up and further left win);
 * otherwise the first free place from its row on. Sets not placed yet go
 * after the last row in use.
 */
export function resolve(sets: Placement[], columns: number): Map<number, Cell> {
  const grid = new Grid(columns)
  const shown = new Map<number, Cell>()
  const placed = sets
    .filter((s) => s.at)
    .sort((a, b) => a.at!.row - b.at!.row || a.at!.col - b.at!.col)
  const displaced: Placement[] = []
  for (const set of placed) {
    if (grid.fits(set.at!, set.width)) {
      grid.take(set.at!, set.width)
      shown.set(set.key, set.at!)
    } else {
      displaced.push(set)
    }
  }
  const lastRow = Math.max(0, ...placed.map((s) => s.at!.row))
  for (const set of [...displaced, ...sets.filter((s) => !s.at)]) {
    const cell = grid.firstFree(set.width, set.at?.row ?? lastRow)
    grid.take(cell, set.width)
    shown.set(set.key, cell)
  }
  return shown
}

/** A grid with every set in `shown` but `except`, to find room among them. */
export function gridWithout(shown: Map<number, Cell>, widths: Map<number, number>, columns: number, except?: number) {
  const grid = new Grid(columns)
  for (const [key, cell] of shown) if (key !== except) grid.take(cell, widths.get(key)!)
  return grid
}

import { describe, expect, it } from 'vitest'

import { footprint, Grid, gridWithout, resolve } from './grid.ts'

describe('footprint', () => {
  it('is the grip and the tiles', () => expect(footprint(3)).toBe(4))
})

describe('resolve', () => {
  it('shows sets where they were put', () => {
    const shown = resolve(
      [
        { key: 1, width: 4, at: { row: 2, col: 5 } },
        { key: 2, width: 5, at: { row: 0, col: 0 } },
      ],
      12,
    )
    expect(shown.get(1)).toEqual({ row: 2, col: 5 })
    expect(shown.get(2)).toEqual({ row: 0, col: 0 })
  })

  it('moves a set that a grown neighbour now overlaps, keeping the earlier one', () => {
    // Set 1 grew to 6 cells and reaches into set 2's place.
    const shown = resolve(
      [
        { key: 1, width: 6, at: { row: 0, col: 0 } },
        { key: 2, width: 4, at: { row: 0, col: 5 } },
      ],
      12,
    )
    expect(shown.get(1)).toEqual({ row: 0, col: 0 })
    expect(shown.get(2)).toEqual({ row: 0, col: 6 })
  })

  it('moves a set that no longer fits a narrower table, and puts it back when there is room', () => {
    const sets = [
      { key: 1, width: 4, at: { row: 0, col: 0 } },
      { key: 2, width: 4, at: { row: 0, col: 8 } },
    ]
    // Still room in its row.
    expect(resolve(sets, 8).get(2)).toEqual({ row: 0, col: 4 })
    // Its row is full: the next one.
    expect(resolve(sets, 6).get(2)).toEqual({ row: 1, col: 0 })
    // Back where it was put.
    expect(resolve(sets, 12).get(2)).toEqual({ row: 0, col: 8 })
  })

  it('puts a set wider than the table at the start of a row of its own', () => {
    const shown = resolve(
      [
        { key: 1, width: 4, at: { row: 0, col: 0 } },
        { key: 2, width: 14, at: { row: 0, col: 3 } },
      ],
      7,
    )
    expect(shown.get(2)).toEqual({ row: 1, col: 0 })
  })

  it('puts sets not placed yet after the last row in use', () => {
    const shown = resolve(
      [
        { key: 1, width: 4, at: { row: 2, col: 0 } },
        { key: 2, width: 4 },
        { key: 3, width: 4 },
      ],
      8,
    )
    expect(shown.get(2)).toEqual({ row: 2, col: 4 })
    expect(shown.get(3)).toEqual({ row: 3, col: 0 })
  })
})

describe('Grid.nearestInRow', () => {
  it('finds the free place in the row nearest to the one asked for', () => {
    const grid = new Grid(12)
    grid.take({ row: 0, col: 3 }, 4) // columns 3-6
    expect(grid.nearestInRow({ row: 0, col: 5 }, 3)).toEqual({ row: 0, col: 7 })
    expect(grid.nearestInRow({ row: 0, col: 2 }, 3)).toEqual({ row: 0, col: 0 })
    expect(grid.nearestInRow({ row: 0, col: 11 }, 3)).toEqual({ row: 0, col: 9 })
  })

  it('has nothing when the row is full', () => {
    const grid = new Grid(6)
    grid.take({ row: 1, col: 0 }, 6)
    expect(grid.nearestInRow({ row: 1, col: 2 }, 2)).toBeNull()
  })

  it('sees every set but the one being moved', () => {
    const shown = new Map([
      [1, { row: 0, col: 0 }],
      [2, { row: 0, col: 5 }],
    ])
    const widths = new Map([
      [1, 4],
      [2, 4],
    ])
    expect(gridWithout(shown, widths, 12, 2).fits({ row: 0, col: 5 }, 4)).toBe(true)
    expect(gridWithout(shown, widths, 12).fits({ row: 0, col: 5 }, 4)).toBe(false)
  })
})

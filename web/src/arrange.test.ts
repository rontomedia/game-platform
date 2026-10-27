import { describe, expect, it } from 'vitest'

import {
  arrangeRack,
  arrangeTable,
  couldBeSet,
  joinEnd,
  keepPlans,
  newOrChanged,
  sortTiles,
  splitRows,
  without,
} from './arrange.ts'
import type { Tile } from './wasm/engine.js'

const n = (color: 'black' | 'blue' | 'orange' | 'red', value: number): Tile => ({ kind: 'number', color, value })
const J: Tile = { kind: 'joker' }

describe('sortTiles', () => {
  it('sorts by number, then colour, jokers last', () => {
    expect(sortTiles([J, n('red', 3), n('black', 3), n('blue', 1)], 'number')).toEqual([
      n('blue', 1),
      n('black', 3),
      n('red', 3),
      J,
    ])
  })
  it('sorts by colour, then number', () => {
    expect(sortTiles([n('red', 1), n('black', 9), J, n('black', 2)], 'color')).toEqual([
      n('black', 2),
      n('black', 9),
      n('red', 1),
      J,
    ])
  })
})

describe('arrangeRack', () => {
  it('splits a new rack in two rows', () => {
    expect(splitRows([n('red', 1), n('red', 2), n('red', 3)])).toEqual([[n('red', 1), n('red', 2)], [n('red', 3)]])
    expect(arrangeRack([[], []], 'custom', [n('red', 1), n('red', 2)])).toEqual([[n('red', 1)], [n('red', 2)]])
  })

  it('keeps the player’s order, drops played tiles and adds a drawn one at the end', () => {
    const previous: [Tile[], Tile[]] = [
      [n('blue', 9), J, n('red', 2)],
      [n('black', 5), n('red', 2)],
    ]
    // One red 2 and the joker were played; an orange 7 was drawn.
    const rack = [n('red', 2), n('orange', 7), n('black', 5), n('blue', 9)]
    expect(arrangeRack(previous, 'custom', rack)).toEqual([
      [n('blue', 9), n('red', 2)],
      [n('black', 5), n('orange', 7)],
    ])
  })

  it('sorts again when the player last sorted', () => {
    const rack = [n('red', 9), n('blue', 1), n('red', 2)]
    expect(arrangeRack([[], []], 'number', rack)).toEqual([[n('blue', 1), n('red', 2)], [n('red', 9)]])
  })
})

describe('arrangeTable', () => {
  /** Sets with no place, in this order. */
  const unplaced = (...sets: Tile[][]) => sets.map((tiles) => ({ tiles }))
  const tilesOf = (sets: { tiles: Tile[] }[]) => sets.map((s) => s.tiles)
  const run = [n('blue', 9), n('blue', 10), n('blue', 11)]
  const group = [n('red', 7), n('blue', 7), n('black', 7)]
  const low = [n('orange', 1), n('orange', 2), n('orange', 3)]

  it('keeps the player’s order of unchanged sets', () => {
    expect(tilesOf(arrangeTable(unplaced(low, run, group), [run, group, low]))).toEqual([low, run, group])
  })

  it('keeps a changed set in its place and adds new sets at the end', () => {
    const longer = [...run, n('blue', 12)]
    const fresh = [n('red', 1), n('red', 2), n('red', 3)]
    expect(tilesOf(arrangeTable(unplaced(group, run), [fresh, longer, group]))).toEqual([group, longer, fresh])
  })

  it('does not let a changed set take an unchanged one’s place', () => {
    // `group` is still there; the other 7s set changed.
    const sevens = [n('orange', 7), n('blue', 7), n('black', 7)]
    const changed = [n('orange', 7), n('blue', 7), n('red', 7), n('black', 7)]
    expect(tilesOf(arrangeTable(unplaced(sevens, group), [group, changed]))).toEqual([changed, group])
  })

  it('starts with the server’s order', () => {
    expect(arrangeTable([], [run, group])).toEqual(unplaced(run, group))
  })

  it('keeps where each set was put, also when it changed; new sets have no place yet', () => {
    const longer = [...run, n('blue', 12)]
    const previous = [
      { tiles: run, at: { row: 3, col: 6 } },
      { tiles: group, at: { row: 0, col: 1 } },
    ]
    expect(arrangeTable(previous, [low, group, longer])).toEqual([
      { tiles: longer, at: { row: 3, col: 6 } },
      { tiles: group, at: { row: 0, col: 1 } },
      { tiles: low },
    ])
  })
})

describe('newOrChanged', () => {
  const run = [n('blue', 9), n('blue', 10), n('blue', 11)]
  const group = [n('red', 7), n('blue', 7), n('black', 7)]

  it('finds new and changed sets, whatever their order on the table', () => {
    const longer = [...run, n('blue', 12)]
    const fresh = [n('red', 1), n('red', 2), n('red', 3)]
    expect(newOrChanged([run, group], [group, longer, fresh])).toEqual([longer, fresh])
  })

  it('finds nothing when the table is the same', () => {
    expect(newOrChanged([run, group], [group, run])).toEqual([])
  })

  it('counts a set that appears twice only as often as it was there', () => {
    expect(newOrChanged([group], [group, group])).toEqual([group])
  })
})

describe('without', () => {
  it('takes away each tile once', () => {
    expect(without([n('red', 3), n('red', 3), J, n('blue', 1)], [n('red', 3), J, n('black', 9)])).toEqual([
      n('red', 3),
      n('blue', 1),
    ])
  })
})

describe('keepPlans', () => {
  it('keeps the planned tiles still on the rack, in order', () => {
    const plans = [[n('red', 3), n('red', 4), n('red', 5)], [n('blue', 9), n('black', 9)]]
    expect(keepPlans(plans, [n('red', 5), n('red', 3), n('blue', 9), n('orange', 1)])).toEqual([
      [n('red', 3), n('red', 5)],
      [n('blue', 9)],
    ])
    expect(keepPlans(plans, [n('red', 4)]), 'empty plans go').toEqual([[n('red', 4)]])
  })
  it('takes a tile once even if two plans want it', () => {
    expect(keepPlans([[n('red', 3)], [n('red', 3)]], [n('red', 3)])).toEqual([[n('red', 3)]])
  })
})

describe('couldBeSet', () => {
  it('accepts sets and the beginnings of sets', () => {
    expect(couldBeSet([n('red', 3)])).toBe(true)
    expect(couldBeSet([n('red', 3), n('red', 4)])).toBe(true)
    expect(couldBeSet([n('red', 3), J, n('red', 5)])).toBe(true)
    expect(couldBeSet([n('red', 9), n('blue', 9)])).toBe(true)
    expect(couldBeSet([J, n('blue', 1), n('blue', 2)]), 'nothing below 1').toBe(false)
  })
  it('refuses what can never be one', () => {
    expect(couldBeSet([n('red', 3), n('red', 5)])).toBe(false)
    expect(couldBeSet([n('red', 3), n('blue', 4)])).toBe(false)
    expect(couldBeSet([n('red', 9), n('red', 9)])).toBe(false)
    expect(couldBeSet([n('red', 9), n('blue', 9), n('black', 9), n('orange', 9), J])).toBe(false)
  })
})

describe('joinEnd', () => {
  it('puts a tile where it fits', () => {
    expect(joinEnd([n('red', 5), n('red', 6)], n('red', 7))).toBe('end')
    expect(joinEnd([n('red', 5), n('red', 6)], n('red', 4))).toBe('start')
    expect(joinEnd([n('red', 5), n('red', 6)], n('blue', 7))).toBe(null)
    expect(joinEnd([n('red', 5), n('blue', 5)], n('black', 5))).toBe('end')
  })
})

import { describe, expect, it } from 'vitest'

import { arrangeRack, arrangeTable, sortTiles, splitRows } from './arrange.ts'
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
  const run = [n('blue', 9), n('blue', 10), n('blue', 11)]
  const group = [n('red', 7), n('blue', 7), n('black', 7)]
  const low = [n('orange', 1), n('orange', 2), n('orange', 3)]

  it('keeps the player’s order of unchanged sets', () => {
    expect(arrangeTable([low, run, group], [run, group, low])).toEqual([low, run, group])
  })

  it('keeps a changed set in its place and adds new sets at the end', () => {
    const longer = [...run, n('blue', 12)]
    const fresh = [n('red', 1), n('red', 2), n('red', 3)]
    expect(arrangeTable([group, run], [fresh, longer, group])).toEqual([group, longer, fresh])
  })

  it('does not let a changed set take an unchanged one’s place', () => {
    // `group` is still there; the other 7s set changed.
    const sevens = [n('orange', 7), n('blue', 7), n('black', 7)]
    const changed = [n('orange', 7), n('blue', 7), n('red', 7), n('black', 7)]
    expect(arrangeTable([sevens, group], [group, changed])).toEqual([changed, group])
  })

  it('starts with the server’s order', () => {
    expect(arrangeTable([], [run, group])).toEqual([run, group])
  })
})

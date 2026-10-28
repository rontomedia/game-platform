import i18next from 'i18next'
import { describe, expect, it } from 'vitest'

import en from './locales/en.json'
import { problemText, turnError } from './messages.ts'
import type { TurnError } from './wasm/engine.js'

const i18n = i18next.createInstance()
await i18n.init({ lng: 'en', resources: { en: { translation: en } }, interpolation: { escapeValue: false } })
const t = i18n.t

describe('turnError', () => {
  it('words every error', () => {
    const errors: TurnError[] = [
      { kind: 'noSuchTile' },
      { kind: 'invalidSet', index: 0 },
      { kind: 'tileTakenFromTable' },
      { kind: 'tileNotOnRack' },
      { kind: 'nothingPlayed' },
      { kind: 'initialMeldChangedTable' },
      { kind: 'initialMeldTooLow', points: 1, needed: 30 },
      { kind: 'notYourTurn' },
      { kind: 'gameOver' },
    ]
    for (const error of errors) expect(turnError(t, error)).not.toMatch(/turnError|\{\{/)
  })

  it('fills in the numbers', () => {
    expect(turnError(t, { kind: 'invalidSet', index: 2 })).toBe('Set 3 is not a run or a group.')
    expect(turnError(t, { kind: 'initialMeldTooLow', points: 21, needed: 30 })).toBe(
      'Your first meld is worth 21 points; it needs 30.',
    )
  })
})

describe('problemText', () => {
  it('passes plain text on', () => {
    expect(problemText(t, { kind: 'text', message: 'not a message' })).toBe('not a message')
  })
})

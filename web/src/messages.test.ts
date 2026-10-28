import i18next from 'i18next'
import { describe, expect, it } from 'vitest'

import en from './locales/en.json'
import { noticeText, problemText, refusal, turnError } from './messages.ts'
import type { Refusal, TurnError } from './wasm/engine.js'

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

describe('refusal', () => {
  it('words every refusal', () => {
    const refusals: Refusal[] = [
      { kind: 'noSuchRoom', code: 'ABCD' },
      { kind: 'roomClosed' },
      { kind: 'roomFull', max: 4 },
      { kind: 'watchersFull', max: 8 },
      { kind: 'gameStarted' },
      { kind: 'noSuchSeat' },
      { kind: 'onlyHost' },
      { kind: 'notABot' },
      { kind: 'tooFewPlayers', min: 2 },
      { kind: 'noGame' },
      { kind: 'notBotsOnly' },
      { kind: 'watching' },
      { kind: 'notStarted' },
      { kind: 'notYourTiles' },
      { kind: 'alreadyInRoom' },
      { kind: 'notInRoom' },
    ]
    for (const r of refusals) expect(refusal(t, r)).not.toMatch(/refusal|\{\{/)
    expect(refusal(t, { kind: 'roomFull', max: 4 })).toBe('The room is full (4 players).')
  })
})

describe('noticeText', () => {
  it('says what happened to the tiles when time ran out', () => {
    expect(noticeText(t, { kind: 'timeRanOut', played: 0, back: 3 })).toMatch(/you drew a tile/)
    expect(noticeText(t, { kind: 'timeRanOut', played: 1, back: 0 })).toBe('Time ran out: your 1 tile was played.')
    expect(noticeText(t, { kind: 'timeRanOut', played: 3, back: 2 })).toBe(
      'Time ran out: 3 tiles played, the valid part of your turn; 2 tiles went back to your rack.',
    )
  })

  it('gives times as minutes and seconds', () => {
    expect(noticeText(t, { kind: 'unearnedWarning', soFarMs: 65_000, limitMs: 120_000 })).toContain('1:05 so far. At 2:00')
    expect(noticeText(t, { kind: 'noMoreBonus', takenBackMs: 125_400 })).toContain('2:05')
  })
})

describe('problemText', () => {
  it('passes plain text on', () => {
    expect(problemText(t, { kind: 'text', message: 'not a message' })).toBe('not a message')
  })
})

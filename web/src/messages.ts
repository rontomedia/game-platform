import type { TFunction } from 'i18next'

import type { Notice, OkeyError, Refusal, TurnError } from './games/tile-rummy/wasm/engine.js'

/**
 * Something that went wrong, kept as what happened and worded when shown
 * (the server sends no sentences, only what happened). `text` is the
 * server's English for a message it couldn't read: a bug, not for players.
 */
export type Problem =
  | { kind: 'turn'; error: TurnError }
  | { kind: 'okey'; error: OkeyError }
  | { kind: 'refused'; refusal: Refusal }
  | { kind: 'text'; message: string }

/** Why a turn isn't allowed, for the player. */
export function turnError(t: TFunction, error: TurnError): string {
  switch (error.kind) {
    case 'invalidSet':
      return t('turnError.invalidSet', { n: error.index + 1 })
    case 'initialMeldTooLow':
      return t('turnError.initialMeldTooLow', { count: error.points, needed: error.needed })
    default:
      return t(`turnError.${error.kind}`)
  }
}

/** Why an Okey 101 move isn't allowed, for the player. */
export function okeyError(t: TFunction, error: OkeyError): string {
  switch (error.kind) {
    case 'invalidSet':
    case 'setChanged':
    case 'okeyNotFree':
      return t(`okeyError.${error.kind}`, { n: error.index + 1 })
    case 'openingTooLow':
      return t('okeyError.openingTooLow', { points: error.points, needed: error.needed })
    case 'tooFewPairs':
      return t('okeyError.tooFewPairs', { pairs: error.pairs, needed: error.needed })
    default:
      return t(`okeyError.${error.kind}`)
  }
}

/** Why the server won't do what the player asked. */
export function refusal(t: TFunction, refusal: Refusal): string {
  switch (refusal.kind) {
    case 'noSuchRoom':
      return t('refusal.noSuchRoom', { code: refusal.code })
    case 'roomFull':
      return t('refusal.roomFull', { count: refusal.max })
    case 'watchersFull':
      return t('refusal.watchersFull', { count: refusal.max })
    case 'tooFewPlayers':
      return t('refusal.tooFewPlayers', { count: refusal.min })
    default:
      return t(`refusal.${refusal.kind}`)
  }
}

export function problemText(t: TFunction, problem: Problem): string {
  switch (problem.kind) {
    case 'turn':
      return turnError(t, problem.error)
    case 'okey':
      return okeyError(t, problem.error)
    case 'refused':
      return refusal(t, problem.refusal)
    case 'text':
      return problem.message
  }
}

/** Milliseconds as minutes and seconds: "1:05". */
export function clockTime(ms: number): string {
  const seconds = Math.round(ms / 1000)
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/** Something the server tells the player, worded. */
export function noticeText(t: TFunction, notice: Notice): string {
  switch (notice.kind) {
    case 'timeRanOut':
      if (notice.played === 0) return t('notice.timeRanOutDrew')
      if (notice.back === 0) return t('notice.timeRanOutAll', { count: notice.played })
      return t('notice.timeRanOutSome', {
        played: t('notice.tilesPlayed', { count: notice.played }),
        back: t('notice.tilesBack', { count: notice.back }),
      })
    case 'unearnedWarning':
      return t('notice.unearnedWarning', { soFar: clockTime(notice.soFarMs), limit: clockTime(notice.limitMs) })
    case 'noMoreBonus':
      return t('notice.noMoreBonus', { time: clockTime(notice.takenBackMs) })
  }
}

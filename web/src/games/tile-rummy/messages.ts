import type { TFunction } from 'i18next'

import { roomNoticeText, roomProblemText } from '../../platform/messages.ts'
import type { RoomProblem } from '../../platform/useRoom.ts'
import type { OkeyError, TurnError } from './wasm/engine.js'
import type { AnyNotice } from './protocol.ts'

/** Tile rummy's own problems: a turn or an Okey move refused. */
export type RummyProblem = { kind: 'turn'; error: TurnError } | { kind: 'okey'; error: OkeyError }

/**
 * Something that went wrong, kept as what happened and worded when shown
 * (the server sends no sentences, only what happened).
 */
export type Problem = RoomProblem | RummyProblem

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

export function problemText(t: TFunction, problem: Problem): string {
  switch (problem.kind) {
    case 'turn':
      return turnError(t, problem.error)
    case 'okey':
      return okeyError(t, problem.error)
    default:
      return roomProblemText(t, problem)
  }
}

/** Something the server tells the player, worded. */
export function noticeText(t: TFunction, notice: AnyNotice): string {
  switch (notice.kind) {
    case 'timeRanOut':
      if (notice.played === 0) return t('notice.timeRanOutDrew')
      if (notice.back === 0) return t('notice.timeRanOutAll', { count: notice.played })
      return t('notice.timeRanOutSome', {
        played: t('notice.tilesPlayed', { count: notice.played }),
        back: t('notice.tilesBack', { count: notice.back }),
      })
    default:
      return roomNoticeText(t, notice)
  }
}

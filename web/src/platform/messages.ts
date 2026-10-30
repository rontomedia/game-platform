import type { TFunction } from 'i18next'

import type { Notice, Refusal } from './protocol.ts'
import type { RoomProblem } from './useRoom.ts'

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

/** The room's problem, for the player (`text` is a bug's English, shown as is). */
export function roomProblemText(t: TFunction, problem: RoomProblem): string {
  return problem.kind === 'refused' ? refusal(t, problem.refusal) : problem.message
}

/** Milliseconds as minutes and seconds: "1:05". */
export function clockTime(ms: number): string {
  const seconds = Math.round(ms / 1000)
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/** Something the room tells the player, worded. */
export function roomNoticeText(t: TFunction, notice: Notice): string {
  switch (notice.kind) {
    case 'unearnedWarning':
      return t('notice.unearnedWarning', { soFar: clockTime(notice.soFarMs), limit: clockTime(notice.limitMs) })
    case 'noMoreBonus':
      return t('notice.noMoreBonus', { time: clockTime(notice.takenBackMs) })
  }
}

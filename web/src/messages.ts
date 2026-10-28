import type { TFunction } from 'i18next'

import type { TurnError } from './wasm/engine.js'

/** Something that went wrong, kept as what happened and worded when shown. */
export type Problem = { kind: 'turn'; error: TurnError } | { kind: 'text'; message: string }

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

export function problemText(t: TFunction, problem: Problem): string {
  return problem.kind === 'turn' ? turnError(t, problem.error) : problem.message
}

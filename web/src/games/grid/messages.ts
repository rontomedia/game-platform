import type { TFunction } from 'i18next'

import { roomProblemText } from '../../platform/messages.ts'
import type { RoomProblem } from '../../platform/useRoom.ts'
import type { GridError } from './wasm/engine.js'

/** A mark refused. */
export type GridProblem = { kind: 'grid'; error: GridError }

export type Problem = RoomProblem | GridProblem

export function problemText(t: TFunction, problem: Problem): string {
  return problem.kind === 'grid' ? t(`gridError.${problem.error.kind}`) : roomProblemText(t, problem)
}

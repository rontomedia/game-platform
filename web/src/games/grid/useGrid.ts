import { useRef, useState } from 'react'

import { useRoom } from '../../platform/useRoom.ts'
import type { GridProblem } from './messages.ts'
import {
  gameProtocolVersion,
  protocolVersion,
  type BotLevel,
  type GridMessage,
  type GridRequest,
  type GridRules,
  type GridView,
} from './wasm/engine.js'

/** The latest state of a grid game. */
export interface GridGame {
  view: GridView
  /** `Date.now()` when the current turn runs out, if it can. */
  turnEnds?: number
  /** Milliseconds left in each player's time bank, as of `turnEnds` for the current player. */
  banks?: number[]
  /** Bumped on every state. */
  version: number
}

/** A connection to the match server for grid games: the room and game this tab is in. */
export function useGrid() {
  const [game, setGame] = useState<GridGame | null>(null)
  const version = useRef(0)
  const connection = useRoom<GridRules, BotLevel, GridMessage, GridRequest, GridProblem>({
    game: 'grid',
    protocol: protocolVersion(),
    gameProtocol: gameProtocolVersion(),
    onGame,
    onLeave: () => setGame(null),
  })
  const { setProblem } = connection

  /** The grid's own messages (`game`). */
  function onGame(message: GridMessage) {
    switch (message.type) {
      case 'state':
        setGame({
          view: message.view,
          turnEnds: message.turnMs === undefined ? undefined : Date.now() + message.turnMs,
          banks: message.banks,
          version: ++version.current,
        })
        setProblem(null)
        break
      case 'rejected':
        setProblem({ kind: 'grid', error: message.error })
        break
    }
  }

  return { ...connection, game }
}

import type { TFunction } from 'i18next'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Advanced, Choice, Lobby, Option, RoomHome, RoomPage, StandIn, TurnTime } from '../../platform/RoomScreens.tsx'
import { useSettings } from '../../platform/settings.ts'
import { remember, remembered } from '../../platform/storage.ts'
import { turnLength } from '../../platform/turns.ts'
import { GridBoard } from './GridBoard.tsx'
import { GridHowToPlay } from './GridRules.tsx'
import { problemText } from './messages.ts'
import { named, type GridOptions } from './protocol.ts'
import { useGrid } from './useGrid.ts'
import initEngine, { gameProtocolVersion, protocolVersion, type BotLevel, type RuleSet } from './wasm/engine.js'

// The rules engine (Rust, games/grid-wasm) checks marks in the browser: loaded with this page.
await initEngine()

/**
 * Grid games' page (Tic-Tac-Toe, Gomoku): home, lobby and board. `kind`:
 * the rule set the home screen starts on (its page's).
 */
export default function GridApp({ kind }: { kind?: RuleSet }) {
  const { status, room, game, problem, mismatch, send, reconnect, leave } = useGrid()
  const [settings, changeSettings] = useSettings()
  const { t } = useTranslation()

  if (room?.started && game) {
    return (
      <GridBoard
        game={game}
        room={room}
        status={status}
        problem={problem && problemText(t, problem)}
        settings={settings}
        onSettings={changeSettings}
        send={send}
        leave={leave}
      />
    )
  }

  const ruleSet = room?.options.game.ruleSet ?? kind ?? 'ticTacToe'
  return (
    <RoomPage
      title={t(`grid.${ruleSet}`)}
      status={status}
      reconnect={reconnect}
      mismatch={mismatch}
      page={`${protocolVersion()}.${gameProtocolVersion()}`}
    >
      {!room ? (
        <Home send={send} problem={problem && problemText(t, problem)} ready={status === 'open'} kind={kind} />
      ) : (
        <Lobby
          room={room}
          rules={describe(t, room.options)}
          levels={BOT_LEVELS}
          problem={problem && problemText(t, problem)}
          send={send}
          leave={leave}
          after={<GridHowToPlay ruleSet={ruleSet} />}
        />
      )}
    </RoomPage>
  )
}

/** The bots a host may seat. */
const BOT_LEVELS = ['easy', 'medium', 'hard', 'extraHard'] as const
/** The bots that may take over a seat: those that answer quickly. */
const STAND_INS = ['easy', 'medium', 'hard'] as const

/** The options the home screen lets the player choose (and remembers). */
interface Choices {
  ruleSet: RuleSet
  turnSeconds: number
  timeBank: boolean
  standIn: BotLevel
}

const defaultChoices: Choices = { ruleSet: 'ticTacToe', turnSeconds: 60, timeBank: false, standIn: 'medium' }

/** Remembered across visits: the options the player last created a grid room with. */
const OPTIONS_KEY = 'grid.roomOptions'

/** The choices as the server takes them: a named rule set, no house rules yet. */
function roomOptions({ ruleSet, turnSeconds, timeBank, standIn }: Choices): GridOptions {
  return { turnSeconds, liveTurns: true, timeBank, standIn, game: named(ruleSet) }
}

function Home({
  send,
  problem,
  ready,
  kind,
}: {
  send: ReturnType<typeof useGrid>['send']
  problem: string | null
  ready: boolean
  kind?: RuleSet
}) {
  const { t } = useTranslation()
  const [choices, setChoices] = useState<Choices>(() => {
    const saved = { ...defaultChoices, ...remembered<Partial<Choices>>(OPTIONS_KEY) }
    return kind ? { ...saved, ruleSet: kind } : saved
  })
  const change = (change: Partial<Choices>) => {
    const next = { ...choices, ...change }
    remember(OPTIONS_KEY, next)
    setChoices(next)
  }
  const changed = Number(choices.timeBank !== defaultChoices.timeBank) + Number(choices.standIn !== defaultChoices.standIn)
  return (
    <RoomHome
      send={send}
      ready={ready}
      problem={problem}
      options={() => roomOptions(choices)}
      after={<GridHowToPlay ruleSet={choices.ruleSet} />}
    >
      <div className="mt-5">
        <span className="block text-sm font-medium text-slate-300">{t('home.game')}</span>
        <Choice
          label={t('home.game')}
          choices={(['ticTacToe', 'gomoku'] as const).map((ruleSet) => [ruleSet, t(`grid.${ruleSet}`)] as const)}
          value={choices.ruleSet}
          onChange={(ruleSet) => change({ ruleSet })}
        />
        <p className="mt-2 text-xs text-slate-500">
          {t(choices.ruleSet === 'gomoku' ? 'grid.goalStone' : 'grid.goal', { line: named(choices.ruleSet).line })}
        </p>
      </div>
      <TurnTime value={choices.turnSeconds} onChange={(turnSeconds) => change({ turnSeconds })} />
      <Advanced changed={changed}>
        <Option
          title={t('home.timeBank')}
          detail={t('home.timeBankDetail')}
          checked={choices.timeBank}
          onChange={(timeBank) => change({ timeBank })}
        />
        <StandIn levels={STAND_INS} value={choices.standIn} onChange={(standIn) => change({ standIn })} />
      </Advanced>
    </RoomHome>
  )
}

/** The room's rules in a sentence, for the lobby. */
function describe(t: TFunction, options: GridOptions): string {
  const { ruleSet, width, height, line } = options.game
  const parts = [`${t(`grid.${ruleSet}`)}: ${t('grid.rules', { line, width, height })}`]
  parts.push(t('room.turn', { time: turnLength(t, options.turnSeconds ?? 60) }))
  if (options.timeBank) parts.push(t('room.timeBank'))
  parts.push(t(`room.standIn.${options.standIn}`))
  return t('room.summary', { parts: parts.join(t('room.separator')) })
}

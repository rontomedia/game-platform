import type { TFunction } from 'i18next'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Advanced, Choice, Lobby, Option, RoomHome, RoomPage, StandIn, TurnTime } from '../../platform/RoomScreens.tsx'
import { turnLength } from '../../platform/turns.ts'
import { useSettings } from '../../platform/settings.ts'
import { remember, remembered } from '../../platform/storage.ts'
import { emptyArrangement, type Arrangement, type RackSort } from './arrange.ts'
import { Board } from './Board.tsx'
import {
  asWatcher,
  demoDrafts,
  demoGame,
  demoOkey,
  demoOkeyRoom,
  demoPlainOkey,
  demoPlainOkeyRoom,
  demoRoom,
  demoWatcherRoom,
  withOpenRacks,
} from './demo.ts'
import { problemText } from './messages.ts'
import { OkeyBoard } from './OkeyBoard.tsx'
import { PlainOkeyBoard } from './PlainOkeyBoard.tsx'
import type { RummyOptions, RummyRoom } from './protocol.ts'
import { HowToPlay } from './Rules.tsx'
import { useRummy, type Draft } from './useRummy.ts'
import { gameProtocolVersion, protocolVersion, type BotLevel, type Okey101Rules, type RuleSet } from './wasm/engine.js'

/** Tile rummy's rule sets, as a page each picks one. */
export type RummyKind = RuleSet['kind']

/**
 * Tile rummy's page: home, lobby and the board of the room's rule set.
 * `kind`: the rule set the home screen starts on (its page's). `?demo`:
 * the board with a made-up game and no server, for working on its look.
 */
export default function RummyApp({ kind }: { kind?: RummyKind }) {
  const search = new URLSearchParams(location.search)
  if (!search.has('demo')) return <Online kind={kind} />
  const mode = search.get('demo')
  if (mode?.startsWith('plain')) return <PlainOkeyDemo />
  return mode?.startsWith('okey') ? <OkeyDemo /> : <Demo />
}

/**
 * `?demo=plain`: a plain Okey hand; `?demo=plain-scores`: its end;
 * `?demo=plain-marks`: the hand with false okeys marked.
 */
function PlainOkeyDemo() {
  const mode = new URLSearchParams(location.search).get('demo')
  const scores = mode === 'plain-scores'
  const [game] = useState(() => demoPlainOkey(scores))
  const [settings, changeSettings] = useSettings()
  return (
    <PlainOkeyBoard
      game={game}
      room={mode === 'plain-marks' ? withMarks(demoPlainOkeyRoom) : demoPlainOkeyRoom}
      status="open"
      problem={null}
      notice={null}
      settings={settings}
      onSettings={changeSettings}
      send={(message) => console.info('demo, not sent:', message)}
      leave={() => location.reload()}
    />
  )
}

/**
 * `?demo=okey`: an Okey 101 hand, the player about to draw.
 * `?demo=okey-scores`: the scores after a hand. `?demo=okey-marks`: the
 * hand with false okeys marked.
 */
function OkeyDemo() {
  const mode = new URLSearchParams(location.search).get('demo')
  const scores = mode === 'okey-scores'
  const [okey] = useState(() => demoOkey(scores))
  const [settings, changeSettings] = useSettings()
  return (
    <OkeyBoard
      okey={okey}
      room={mode === 'okey-marks' ? withMarks(demoOkeyRoom) : demoOkeyRoom}
      status="open"
      problem={null}
      notice={null}
      settings={settings}
      onSettings={changeSettings}
      draft={null}
      send={(message) => console.info('demo, not sent:', message)}
      leave={() => location.reload()}
    />
  )
}

/** A demo room with the false okeys marked (`falseOkeyMarks`). */
const withMarks = (room: RummyRoom): RummyRoom => ({
  ...room,
  options: { ...room.options, game: { ...room.options.game, falseOkeyMarks: true } },
})

/**
 * The demo game. `?demo=watch`: it's Maya's turn instead, and she shares it
 * live, a step every few seconds. `?demo=racks`: every rack is shown, as
 * with a server started with `--open-racks`. `?demo=watcher`: bots play
 * and the player watches, as the host who can pause them.
 */
function Demo() {
  const mode = new URLSearchParams(location.search).get('demo')
  const watcher = mode === 'watcher'
  const watch = mode === 'watch' || watcher
  const [game] = useState(() =>
    mode === 'racks' ? withOpenRacks(demoGame()) : watcher ? asWatcher(demoGame(true)) : demoGame(watch),
  )
  const [paused, setPaused] = useState(false)
  const [settings, changeSettings] = useSettings()
  const [arrangement, setArrangement] = useArrangement(demoRoom.code)
  const [step, setStep] = useState(0)
  useEffect(() => {
    if (!watch || paused) return
    const timer = setInterval(() => setStep((step) => step + 1), 2500)
    return () => clearInterval(timer)
  }, [watch, paused])
  // Made once: a new table each render would look like a new draft each time.
  const [drafts] = useState(demoDrafts)
  const table = watch ? drafts[step % drafts.length] : null
  const draft: Draft | null = table && { by: game.view.current, table, version: game.version }
  return (
    <Board
      game={game}
      room={watcher ? { ...demoWatcherRoom, paused } : demoRoom}
      status="open"
      problem={null}
      notice={null}
      settings={settings}
      onSettings={changeSettings}
      arrangement={arrangement}
      onArrange={setArrangement}
      draft={settings.watchLive ? draft : null}
      send={(message) => {
        if (message.type === 'pause') setPaused(message.paused)
        else console.info('demo, not sent:', message)
      }}
      leave={() => location.reload()}
    />
  )
}

/** Remembered across visits: how the player last ordered their rack. */
const SORT_KEY = 'rummy.rackSort'
/** Remembered for the tab: the player's arrangement in their current room. */
const ARRANGEMENT_KEY = 'rummy.arrangement.v2'

/**
 * The player's arrangement of rack and table in room `code`, kept across
 * turns and reloads (see arrange.ts).
 */
function useArrangement(code: string | undefined): [Arrangement, (arrangement: Arrangement) => void] {
  const [saved, setSaved] = useState(() => remembered<{ code: string; arrangement: Arrangement }>(ARRANGEMENT_KEY, 'session'))
  const arrangement =
    saved && saved.code === code ? saved.arrangement : { ...emptyArrangement, sort: remembered<RackSort>(SORT_KEY) ?? 'custom' }
  // Stable, as the board saves it in an effect.
  const save = useCallback(
    (arrangement: Arrangement) => {
      if (code === undefined) return
      const next = { code, arrangement }
      remember(ARRANGEMENT_KEY, next, 'session')
      remember(SORT_KEY, arrangement.sort)
      setSaved(next)
    },
    [code],
  )
  return [arrangement, save]
}

function Online({ kind }: { kind?: RummyKind }) {
  const { status, room, game, okey, plainOkey, draft, problem, notice, mismatch, send, reconnect, leave } = useRummy()
  const [settings, changeSettings] = useSettings()
  const [arrangement, setArrangement] = useArrangement(room?.code)
  const { t } = useTranslation()
  const page = `${protocolVersion()}.${gameProtocolVersion()}`

  if (room?.started && plainOkey && room.options.game.rules?.kind === 'okey') {
    return (
      <PlainOkeyBoard
        key={plainOkey.version}
        game={plainOkey}
        room={room}
        status={status}
        problem={problem}
        notice={notice}
        settings={settings}
        onSettings={changeSettings}
        send={send}
        leave={leave}
      />
    )
  }

  if (room?.started && okey && room.options.game.rules?.kind === 'okey101') {
    return (
      <OkeyBoard
        key={okey.version}
        okey={okey}
        room={room}
        status={status}
        problem={problem}
        notice={notice}
        settings={settings}
        onSettings={changeSettings}
        draft={settings.watchLive ? draft : null}
        send={send}
        leave={leave}
      />
    )
  }

  if (room?.started && game) {
    return (
      <Board
        key={game.version}
        game={game}
        room={room}
        status={status}
        problem={problem}
        notice={notice}
        settings={settings}
        onSettings={changeSettings}
        arrangement={arrangement}
        onArrange={setArrangement}
        draft={settings.watchLive ? draft : null}
        send={send}
        leave={leave}
      />
    )
  }

  return (
    <RoomPage title={t('app.title')} status={status} reconnect={reconnect} mismatch={mismatch} page={page}>
      {!room ? (
        <Home send={send} problem={problem && problemText(t, problem)} ready={status === 'open'} kind={kind} />
      ) : (
        <Lobby
          room={room}
          rules={describe(t, choices(room.options))}
          levels={BOT_LEVELS}
          problem={problem && problemText(t, problem)}
          send={send}
          leave={leave}
          after={<HowToPlay kind={room.options.game.rules?.kind ?? 'classic'} />}
        />
      )}
    </RoomPage>
  )
}

/** The bots a host may seat. */
const BOT_LEVELS = ['easy', 'medium', 'hard', 'extraHard'] as const

/** The home screen with tile rummy's options: the rule set its page has, unless the player chose another. */
function Home({
  send,
  problem,
  ready,
  kind,
}: {
  send: ReturnType<typeof useRummy>['send']
  problem: string | null
  ready: boolean
  kind?: RummyKind
}) {
  const { t } = useTranslation()
  const [options, setOptions] = useState<Choices>(() => {
    const saved = { ...defaultOptions, ...remembered<Partial<Choices>>(OPTIONS_KEY) }
    return kind && saved.rules.kind !== kind ? { ...saved, rules: rulesOf(kind) } : saved
  })
  const change = (change: Partial<Choices>) => {
    const next = { ...options, ...change }
    remember(OPTIONS_KEY, next)
    setOptions(next)
  }

  // Settings left out take the defaults, as on the server.
  const okeyRules = options.rules.kind === 'okey101' ? { ...OKEY_DEFAULTS, ...options.rules } : null
  const changeOkey = (rules: Partial<Okey101Rules>) => okeyRules && change({ rules: { ...okeyRules, ...rules } })

  return (
    <RoomHome send={send} ready={ready} problem={problem} options={() => roomOptions(options)} after={<HowToPlay kind={options.rules.kind} />}>
      <div className="mt-5">
        <span className="block text-sm font-medium text-slate-300">{t('home.game')}</span>
        <Choice
          label={t('home.game')}
          choices={[['classic', t('home.classic')], ['okey101', t('home.okey101')], ['okey', t('home.okey')]] as const}
          value={options.rules.kind}
          onChange={(kind) =>
            change({
              rules: kind === 'okey101' ? { kind, ...OKEY_DEFAULTS } : kind === 'okey' ? { kind, points: 20 } : { kind },
            })
          }
        />
        {options.rules.kind === 'okey' && (
          <>
            <span className="mt-3 block text-sm font-medium text-slate-300">{t('home.points')}</span>
            <Choice
              label={t('home.points')}
              choices={OKEY_POINTS.map((points) => [points, String(points)] as const)}
              value={options.rules.points ?? 20}
              onChange={(points) => change({ rules: { kind: 'okey', points } })}
            />
          </>
        )}
        {okeyRules && (
          <>
            <span className="mt-3 block text-sm font-medium text-slate-300">{t('home.hands')}</span>
            <Choice
              label={t('home.hands')}
              choices={OKEY_HANDS.map((hands) => [hands, String(hands)] as const)}
              value={okeyRules.hands}
              onChange={(hands) => changeOkey({ hands })}
            />
          </>
        )}
      </div>
      <TurnTime value={options.turnSeconds} onChange={(turnSeconds) => change({ turnSeconds })} />
      <Advanced changed={changedOptions(options)}>
        <Option
          title={t('home.timeBank')}
          detail={t('home.timeBankDetail')}
          checked={options.timeBank}
          onChange={(timeBank) => change({ timeBank })}
        />
        {options.rules.kind === 'okey' ? null : okeyRules ? (
          <>
            <Option
              title={t('home.progressive')}
              detail={t('home.progressiveDetail')}
              checked={okeyRules.progressive}
              onChange={(progressive) => changeOkey({ progressive })}
            />
            <Option
              title={t('home.penalties')}
              detail={t('home.penaltiesDetail')}
              checked={okeyRules.penalties}
              onChange={(penalties) => changeOkey({ penalties })}
            />
          </>
        ) : (
          <Option
            title={t('home.tileBonus')}
            detail={t('home.tileBonusDetail')}
            checked={options.tileBonus}
            onChange={(tileBonus) => change({ tileBonus })}
          />
        )}
        {options.rules.kind !== 'classic' && (
          <Option
            title={t('home.falseOkeyMarks')}
            detail={t('home.falseOkeyMarksDetail')}
            checked={options.falseOkeyMarks}
            onChange={(falseOkeyMarks) => change({ falseOkeyMarks })}
          />
        )}
        <Option
          title={t('home.liveTurns')}
          detail={t('home.liveTurnsDetail')}
          checked={options.liveTurns}
          onChange={(liveTurns) => change({ liveTurns })}
        />
        <StandIn levels={STAND_INS} value={options.standIn} onChange={(standIn) => change({ standIn })} />
      </Advanced>
    </RoomHome>
  )
}

/** A rule set with its settings at their defaults. */
function rulesOf(kind: RummyKind): RuleSet {
  return kind === 'okey101' ? { kind, ...OKEY_DEFAULTS } : kind === 'okey' ? { kind, points: 20 } : { kind }
}

/**
 * The room's options as the home screen lets the player choose them (and
 * remembers them): side by side, though tile rummy's are sent inside
 * `game` (`roomOptions`).
 */
interface Choices {
  turnSeconds: number
  liveTurns: boolean
  timeBank: boolean
  tileBonus: boolean
  standIn: BotLevel
  falseOkeyMarks: boolean
  rules: RuleSet
}

/** The choices as the server takes them. */
function roomOptions({ turnSeconds, liveTurns, timeBank, standIn, rules, tileBonus, falseOkeyMarks }: Choices): RummyOptions {
  return { turnSeconds, liveTurns, timeBank, standIn, game: { rules, tileBonus, falseOkeyMarks } }
}

/** A room's options as choices, the defaults for any left out. */
function choices(options: RummyOptions): Choices {
  const { game, ...room } = options
  return { ...defaultOptions, ...room, ...game }
}

/** The room options to start from, as the server has them. */
const defaultOptions: Choices = {
  turnSeconds: 60,
  liveTurns: true,
  timeBank: false,
  tileBonus: false,
  standIn: 'medium',
  falseOkeyMarks: false,
  rules: { kind: 'classic' },
}
/** An Okey 101 room's settings to start from (the engine's defaults). */
const OKEY_DEFAULTS: Required<Okey101Rules> = { hands: 7, progressive: false, penalties: true }
/** The match lengths an Okey 101 room may have (the server's `OKEY_HANDS`). */
const OKEY_HANDS = [1, 3, 5, 7, 9, 11]
/** The points a plain Okey room may start each player with (the server's `OKEY_POINTS`). */
const OKEY_POINTS = [4, 10, 20, 30]
/** The bots that may take over a seat: not Extra Hard (the server's `checked`). */
const STAND_INS = ['easy', 'medium', 'hard'] as const

/** How many of the options under "Advanced" differ from the defaults. */
function changedOptions(options: Choices): number {
  const shared = (['timeBank', 'liveTurns', 'standIn'] as const).filter((key) => options[key] !== defaultOptions[key]).length
  const marks = Number(options.falseOkeyMarks !== defaultOptions.falseOkeyMarks)
  if (options.rules.kind === 'okey101') {
    const okey = { ...OKEY_DEFAULTS, ...options.rules }
    return shared + marks + Number(okey.progressive !== OKEY_DEFAULTS.progressive) + Number(okey.penalties !== OKEY_DEFAULTS.penalties)
  }
  if (options.rules.kind === 'okey') return shared + marks
  return shared + Number(options.tileBonus !== defaultOptions.tileBonus)
}
/** Remembered across visits: the options the player last created a room with. */
const OPTIONS_KEY = 'rummy.roomOptions'

/** The room's rules in a sentence, for the lobby. */
function describe(t: TFunction, options: Choices): string {
  const rules = options.rules
  const parts = [
    rules.kind === 'okey101'
      ? t('room.okey101', { hands: rules.hands ?? OKEY_DEFAULTS.hands })
      : rules.kind === 'okey'
        ? t('room.okeyPlain', { points: rules.points ?? 20 })
        : t('room.classic'),
  ]
  parts.push(t('room.turn', { time: turnLength(t, options.turnSeconds) }))
  if (rules.kind === 'okey101' && rules.progressive) parts.push(t('room.progressive'))
  if (rules.kind === 'okey101' && rules.penalties === false) parts.push(t('room.noPenalties'))
  if (options.falseOkeyMarks && rules.kind !== 'classic') parts.push(t('room.falseOkeyMarks'))
  if (options.timeBank) parts.push(t('room.timeBank'))
  if (options.tileBonus && rules.kind === 'classic') parts.push(t('room.tileBonus'))
  parts.push(options.liveTurns ? t('room.liveTurns') : t('room.finishedTurns'))
  parts.push(t(`room.standIn.${options.standIn}`))
  return t('room.summary', { parts: parts.join(t('room.separator')) })
}


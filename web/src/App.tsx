import { AnimatePresence, motion } from 'motion/react'
import type { TFunction } from 'i18next'
import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { emptyArrangement, type Arrangement, type RackSort } from './arrange.ts'
import { Board } from './Board.tsx'
import { LanguagePicker } from './LanguagePicker.tsx'

import { asWatcher, demoDrafts, demoGame, demoRoom, demoWatcherRoom, withOpenRacks } from './demo.ts'
import { useSettings } from './settings.ts'
import { remember, remembered } from './storage.ts'
import { problemText, type Problem } from './messages.ts'
import { Button, inputClass, Note, Panel, Switch } from './ui.tsx'
import { SERVER_URL, useGame, type Draft, type Status } from './useGame.ts'
import { protocolVersion, type ClientMessage, type RoomInfo, type RoomOptions } from './wasm/engine.js'

const statusDot: Record<Status, string> = {
  connecting: 'bg-amber-400 animate-pulse',
  open: 'bg-emerald-400',
  closed: 'bg-rose-500',
}

/** `?demo`: the board with a made-up game and no server, for working on its look. */
const DEMO = new URLSearchParams(location.search).has('demo')

export default function App() {
  return DEMO ? <Demo /> : <Online />
}

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

function Online() {
  const { status, room, game, draft, problem, notice, mismatch, send, reconnect, leave } = useGame()
  const [settings, changeSettings] = useSettings()
  const [arrangement, setArrangement] = useArrangement(room?.code)
  const { t } = useTranslation()

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
    <div className="mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6 sm:px-6 lg:py-10">
      <header className="flex items-center gap-3 sm:gap-4">
        <h1 className="me-auto shrink-0 text-3xl font-bold tracking-tight">{t('app.title')}</h1>
        <span className="flex shrink-0 items-center gap-2 whitespace-nowrap text-sm text-slate-300" title={SERVER_URL}>
          <span className={`size-2.5 shrink-0 rounded-full ${statusDot[status]}`} />
          <span className="max-sm:sr-only">{t(`app.status.${status}`)}</span>
          {status === 'closed' && (
            <Button look="quiet" className="px-2 py-1 text-sm" onClick={reconnect}>
              {t('app.reconnect')}
            </Button>
          )}
        </span>
        <LanguagePicker
          value={settings.language}
          onChange={(language) => changeSettings({ language })}
          className="min-w-0 max-w-52 sm:max-w-60"
        />
      </header>
      {mismatch !== null && (
        <p className="rounded-xl bg-rose-500/15 px-3 py-2 text-sm text-rose-200">
          {t('app.mismatch', { server: mismatch, page: protocolVersion() })}
        </p>
      )}
      {!room ? (
        <Home send={send} problem={problem} ready={status === 'open'} />
      ) : (
        <Lobby room={room} problem={problem} send={send} leave={leave} />
      )}
    </div>
  )
}

/** The player's name, remembered on this device. */
const NAME_KEY = 'rummy.name'

function Home({ send, problem, ready }: { send: (m: ClientMessage) => void; problem: Problem | null; ready: boolean }) {
  const { t } = useTranslation()
  const [name, setName] = useState(() => remembered<string>(NAME_KEY) ?? '')
  const [code, setCode] = useState('')

  const [options, setOptions] = useState<Required<RoomOptions>>(() => ({
    ...defaultOptions,
    ...remembered<RoomOptions>(OPTIONS_KEY),
  }))
  const change = (change: RoomOptions) => {
    const next = { ...options, ...change }
    remember(OPTIONS_KEY, next)
    setOptions(next)
  }

  const create = () => {
    remember(NAME_KEY, name.trim())
    send({ type: 'createRoom', name, options })
  }
  const join = (watch: boolean) => {
    remember(NAME_KEY, name.trim())
    send({ type: 'joinRoom', code, name, watch })
  }

  return (
    <div className="mx-auto w-full max-w-lg">
      <Panel>
        <label className="flex flex-col gap-2">
          <span className="text-sm font-medium text-slate-300">{t('home.yourName')}</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={20}
            placeholder={t('home.name')}
            className={inputClass}
          />
        </label>
        <div className="mt-5">
          <span className="block text-sm font-medium text-slate-300">{t('home.turnTime')}</span>
          <Choice
            label={t('home.turnTime')}
            choices={TURN_SECONDS.map((seconds) => [seconds, turnLength(t, seconds)] as const)}
            value={options.turnSeconds}
            onChange={(turnSeconds) => change({ turnSeconds })}
          />
        </div>
        <Advanced changed={changedOptions(options)}>
          <Option
            title={t('home.timeBank')}
            detail={t('home.timeBankDetail')}
            checked={options.timeBank}
            onChange={(timeBank) => change({ timeBank })}
          />
          <Option
            title={t('home.tileBonus')}
            detail={t('home.tileBonusDetail')}
            checked={options.tileBonus}
            onChange={(tileBonus) => change({ tileBonus })}
          />
          <Option
            title={t('home.liveTurns')}
            detail={t('home.liveTurnsDetail')}
            checked={options.liveTurns}
            onChange={(liveTurns) => change({ liveTurns })}
          />
          <div className="mt-5">
            <span className="block text-sm font-medium text-slate-300">{t('home.standIn')}</span>
            <span className="mt-0.5 block text-xs text-slate-500">{t('home.standInDetail')}</span>
            <Choice
              label={t('home.standInLabel')}
              choices={STAND_INS.map((level) => [level, t(`bots.level.${level}`)] as const)}
              value={options.standIn}
              onChange={(standIn) => change({ standIn })}
            />
          </div>
        </Advanced>
        <Button look="primary" className="mt-4 w-full" disabled={!ready} onClick={create}>
          {t('home.create')}
        </Button>
        <div className="my-5 flex items-center gap-3 text-sm text-slate-500">
          <span className="h-px flex-1 bg-white/10" />
          {t('home.orJoin')}
          <span className="h-px flex-1 bg-white/10" />
        </div>
        <form
          onSubmit={(e: FormEvent) => {
            e.preventDefault()
            join(false)
          }}
          // Long labels in some languages: the buttons go below the code rather than squeeze it.
          className="flex flex-wrap gap-2"
        >
          <input
            value={code}
            onChange={(e) => setCode(e.target.value.toUpperCase())}
            maxLength={4}
            placeholder={t('home.codePlaceholder')}
            aria-label={t('home.code')}
            autoCapitalize="characters"
            autoComplete="off"
            className={`${inputClass} min-w-32 flex-[1_1_8rem] text-center font-mono text-lg tracking-[0.3em]`}
          />
          <Button className="grow" disabled={!ready || code.trim().length !== 4}>
            {t('home.join')}
          </Button>
          <Button
            type="button"
            look="quiet"
            className="grow"
            disabled={!ready || code.trim().length !== 4}
            onClick={() => join(true)}
          >
            {t('home.watch')}
          </Button>
        </form>
        {problem && (
          <div className="mt-4">
            <Note tone="bad">{problemText(t, problem)}</Note>
          </div>
        )}
      </Panel>
    </div>
  )
}

/** The room options to start from, as the server has them. */
const defaultOptions: Required<RoomOptions> = {
  turnSeconds: 60,
  liveTurns: true,
  timeBank: false,
  tileBonus: false,
  standIn: 'medium',
}
/** The bots that may take over a seat: not Extra Hard (the server's `checked`). */
const STAND_INS = ['easy', 'medium', 'hard'] as const

/** How many of the options under "Advanced" differ from the defaults. */
function changedOptions(options: Required<RoomOptions>): number {
  const advanced = ['timeBank', 'tileBonus', 'liveTurns', 'standIn'] as const
  return advanced.filter((key) => options[key] !== defaultOptions[key]).length
}
/** Remembered across visits: the options the player last created a room with. */
const OPTIONS_KEY = 'rummy.roomOptions'
/** The turn lengths a room may have (the server's `TURN_SECONDS`). */
const TURN_SECONDS = [30, 60, 90, 120, 180]

/** A turn's length, short: in minutes when it's whole ones. */
function turnLength(t: TFunction, seconds: number): string {
  return seconds % 60 === 0 ? t('units.minutes', { n: seconds / 60 }) : t('units.seconds', { n: seconds })
}

/** The room's rules in a sentence, for the lobby. */
function describe(t: TFunction, options: RoomOptions): string {
  const parts = [t('room.turn', { time: turnLength(t, options.turnSeconds ?? 60) })]
  if (options.timeBank) parts.push(t('room.timeBank'))
  if (options.tileBonus) parts.push(t('room.tileBonus'))
  parts.push(options.liveTurns ? t('room.liveTurns') : t('room.finishedTurns'))
  parts.push(t(`room.standIn.${options.standIn ?? 'medium'}`))
  return t('room.summary', { parts: parts.join(t('room.separator')) })
}

/**
 * Options most rooms leave as they are, folded away. When adding a room
 * option, put it here unless most players would want to choose it.
 */
function Advanced({ changed, children }: { changed: number; children: ReactNode }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <div className="mt-5 border-t border-white/10 pt-3">
      <button
        className="flex w-full cursor-pointer items-center gap-2 py-1 text-sm font-medium text-slate-300 hover:text-slate-100"
        aria-expanded={open}
        onClick={() => setOpen((open) => !open)}
      >
        <svg
          viewBox="0 0 16 16"
          className={`size-3.5 transition-transform rtl:-scale-x-100 ${open ? 'rotate-90' : ''}`}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden
        >
          <path d="m6 3 5 5-5 5" />
        </svg>
        <span className="flex-1 text-start">{t('home.advanced')}</span>
        {changed > 0 && <span className="text-xs font-normal text-teal-300">{t('home.changed', { count: changed })}</span>}
      </button>
      <AnimatePresence initial={false}>
        {open && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            {children}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

/** One of a few values, side by side. */
function Choice<T extends string | number>({
  label,
  choices,
  value,
  onChange,
}: {
  label: string
  choices: readonly (readonly [T, string])[]
  value: T
  onChange: (value: T) => void
}) {
  return (
    <div className="mt-2 flex rounded-xl bg-black/30 p-1 text-sm font-semibold" role="group" aria-label={label}>
      {choices.map(([choice, text]) => (
        <button
          key={choice}
          onClick={() => onChange(choice)}
          aria-pressed={value === choice}
          className={`flex-1 cursor-pointer rounded-lg py-1.5 transition ${
            value === choice ? 'bg-teal-500 text-slate-950' : 'text-slate-300 hover:bg-white/10'
          }`}
        >
          {text}
        </button>
      ))}
    </div>
  )
}

function Option({
  title,
  detail,
  checked,
  onChange,
}: {
  title: string
  detail: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label className="mt-5 flex cursor-pointer items-start gap-3">
      <span className="flex-1">
        <span className="block text-sm font-medium text-slate-300">{title}</span>
        <span className="mt-0.5 block text-xs text-slate-500">{detail}</span>
      </span>
      <Switch checked={checked} onChange={onChange} />
    </label>
  )
}

/** Seats in a room (the server's limit too). */
const MAX_PLAYERS = 4

function Lobby({
  room,
  problem,
  send,
  leave,
}: {
  room: RoomInfo
  problem: Problem | null
  send: (m: ClientMessage) => void
  leave: () => void
}) {
  const { t } = useTranslation()
  const present = room.seats.filter((s) => s.connected).length
  const watching = room.watching !== undefined
  const isHost = (watching ? room.watchers[room.watching!] : room.seats[room.you ?? -1])?.host ?? false
  const full = room.seats.length >= MAX_PLAYERS
  return (
    <div className="mx-auto w-full max-w-lg">
      <Panel>
        <p className="text-sm text-slate-400">{t('lobby.code')}</p>
        <p className="mt-1 font-mono text-5xl font-bold tracking-[0.2em] text-teal-300">{room.code}</p>
        <p className="mt-3 text-sm text-slate-400">{describe(t, room.options)}</p>
        <ul className="mt-6 space-y-2">
          {room.seats.map((seat, i) => (
            <li key={i} className="flex items-center gap-3 rounded-xl bg-white/5 px-3 py-2">
              <span className={`size-2 rounded-full ${seat.connected ? 'bg-emerald-400' : 'bg-slate-500'}`} />
              <span className="flex-1">
                {seat.name}
                {i === room.you && <span className="text-slate-400"> {t('lobby.you')}</span>}
              </span>
              {seat.bot && <span className="text-xs uppercase tracking-wide text-violet-300">{t(`bots.bot.${seat.bot}`)}</span>}
              {seat.bot && isHost && (
                <button
                  className="-my-1 -me-1 grid size-7 cursor-pointer place-items-center rounded-lg text-slate-400 hover:bg-white/10 hover:text-slate-100"
                  aria-label={t('lobby.removeBot', { name: seat.name })}
                  title={t('lobby.remove')}
                  onClick={() => send({ type: 'removeBot', seat: i })}
                >
                  ×
                </button>
              )}
              {seat.host && <span className="text-xs uppercase tracking-wide text-teal-300">{t('lobby.host')}</span>}
            </li>
          ))}
        </ul>
        {room.watchers.length > 0 && (
          <>
            <p className="mt-5 text-sm text-slate-400">{t('lobby.watching')}</p>
            <ul className="mt-2 space-y-2">
              {room.watchers.map((watcher, i) => (
                <li key={i} className="flex items-center gap-3 rounded-xl bg-white/[0.03] px-3 py-2 text-slate-300">
                  <span className={`size-2 rounded-full ${watcher.connected ? 'bg-emerald-400' : 'bg-slate-500'}`} />
                  <span className="flex-1">
                    {watcher.name}
                    {i === room.watching && <span className="text-slate-400"> {t('lobby.you')}</span>}
                  </span>
                  {watcher.host && <span className="text-xs uppercase tracking-wide text-teal-300">{t('lobby.host')}</span>}
                </li>
              ))}
            </ul>
          </>
        )}
        <button
          className="mt-3 cursor-pointer text-sm text-teal-300 hover:text-teal-200 disabled:cursor-default disabled:text-slate-500"
          disabled={watching && full}
          onClick={() => send({ type: 'watch', watching: !watching })}
        >
          {watching ? t('lobby.takeSeat') : t('lobby.watchInstead')}
        </button>
        {isHost && !full && (
          <div className="mt-3 text-sm text-slate-400">
            <span>{t('lobby.addBot')}</span>
            {/* Whole buttons wrap, never a label: some levels' names are long. */}
            <div className="mt-1.5 flex flex-wrap gap-1">
              {(['easy', 'medium', 'hard', 'extraHard'] as const).map((level) => (
                <Button
                  key={level}
                  look="quiet"
                  className="grow whitespace-nowrap px-2 py-1.5 text-sm"
                  onClick={() => send({ type: 'addBot', level })}
                >
                  + {t(`bots.level.${level}`)}
                </Button>
              ))}
            </div>
          </div>
        )}
        <div className="mt-6 flex gap-2">
          <Button onClick={leave}>{t('lobby.leave')}</Button>
          {isHost ? (
            <Button look="primary" className="flex-1" disabled={present < 2} onClick={() => send({ type: 'start' })}>
              {present < 2 ? t('lobby.waitingForPlayers') : t('lobby.start', { count: present })}
            </Button>
          ) : (
            <p className="flex-1 self-center text-center text-sm text-slate-400">{t('lobby.waitingForHost')}</p>

          )}
        </div>
        {problem && (
          <div className="mt-4">
            <Note tone="bad">{problemText(t, problem)}</Note>
          </div>
        )}
      </Panel>
    </div>
  )
}

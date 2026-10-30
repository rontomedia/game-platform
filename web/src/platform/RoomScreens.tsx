import { AnimatePresence, motion } from 'motion/react'
import { useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { LanguagePicker } from './LanguagePicker.tsx'
import type { RoomInfo } from './protocol.ts'
import { useSettings } from './settings.ts'
import { remember, remembered } from './storage.ts'
import { TURN_SECONDS, turnLength } from './turns.ts'
import { Button, inputClass, Note, Panel, Switch } from './ui.tsx'
import { SERVER_URL, type Outgoing, type Status } from './useRoom.ts'

/**
 * The screens around a game: its page's header, the home screen (a name,
 * the game's options, create or join) and the lobby. Each game fills in its
 * options, how its rules read, and how to play.
 */

const statusDot: Record<Status, string> = {
  connecting: 'bg-amber-400 animate-pulse',
  open: 'bg-emerald-400',
  closed: 'bg-rose-500',
}

/** A game's page before its board: its title, the connection, the language, then `children`. */
export function RoomPage({
  title,
  status,
  reconnect,
  mismatch,
  page,
  children,
}: {
  title: string
  status: Status
  reconnect: () => void
  /** The server's protocol versions, when they aren't this page's (`page`). */
  mismatch: string | null
  page: string
  children: ReactNode
}) {
  const { t } = useTranslation()
  const [settings, changeSettings] = useSettings()
  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6 sm:px-6 lg:py-10">
      <header className="flex items-center gap-3 sm:gap-4">
        <h1 className="me-auto shrink-0 text-3xl font-bold tracking-tight">{title}</h1>
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
          {t('app.mismatch', { server: mismatch, page })}
        </p>
      )}
      {children}
    </div>
  )
}

/** The player's name, remembered on this device (for every game). */
const NAME_KEY = 'player.name'
/** Where it was kept when tile rummy was the only game. */
const OLD_NAME_KEY = 'rummy.name'

/**
 * The home screen: the player's name, the game's options (`children`),
 * create a room with `options()`, or join one by its code.
 */
export function RoomHome<R, L, Request>({
  send,
  ready,
  problem,
  options,
  children,
  after,
}: {
  send: (message: Outgoing<R, L, Request>) => void
  ready: boolean
  /** What went wrong, worded. */
  problem: string | null
  options: () => import('./protocol.ts').RoomOptions<R, L>
  children: ReactNode
  /** Below the box, as wide as the page (how to play). */
  after?: ReactNode
}) {
  const { t } = useTranslation()
  const [name, setName] = useState(() => remembered<string>(NAME_KEY) ?? remembered<string>(OLD_NAME_KEY) ?? '')
  const [code, setCode] = useState('')
  const create = () => {
    remember(NAME_KEY, name.trim())
    send({ type: 'createRoom', name, options: options() })
  }
  const join = (watch: boolean) => {
    remember(NAME_KEY, name.trim())
    send({ type: 'joinRoom', code, name, watch })
  }
  return (
    <div className="w-full">
      <Panel className="mx-auto max-w-lg">
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
        {children}
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
            <Note tone="bad">{problem}</Note>
          </div>
        )}
      </Panel>
      {after}
    </div>
  )
}

/** The turn length to choose, as every game has it. */
export function TurnTime({ value, onChange }: { value: number; onChange: (seconds: number) => void }) {
  const { t } = useTranslation()
  return (
    <div className="mt-5">
      <span className="block text-sm font-medium text-slate-300">{t('home.turnTime')}</span>
      <Choice
        label={t('home.turnTime')}
        choices={TURN_SECONDS.map((seconds) => [seconds, turnLength(t, seconds)] as const)}
        value={value}
        onChange={onChange}
      />
    </div>
  )
}

/** The bot to take over a seat, of `levels`. */
export function StandIn<L extends string>({
  levels,
  value,
  onChange,
}: {
  levels: readonly L[]
  value: L
  onChange: (level: L) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="mt-5">
      <span className="block text-sm font-medium text-slate-300">{t('home.standIn')}</span>
      <span className="mt-0.5 block text-xs text-slate-500">{t('home.standInDetail')}</span>
      <Choice
        label={t('home.standInLabel')}
        choices={levels.map((level) => [level, t(`bots.level.${level}` as 'bots.level.easy')] as const)}
        value={value}
        onChange={onChange}
      />
    </div>
  )
}

/**
 * Options most rooms leave as they are, folded away. When adding a room
 * option, put it here unless most players would want to choose it.
 */
export function Advanced({ changed, children }: { changed: number; children: ReactNode }) {
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
export function Choice<T extends string | number>({
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

/** An option to switch on or off. */
export function Option({
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

/**
 * The lobby: the room's code, its rules (`rules`, worded), who's in it,
 * bots of `levels` to add (the host), and the start.
 */
export function Lobby<R, L extends string, Request>({
  room,
  rules,
  levels,
  problem,
  send,
  leave,
  after,
}: {
  room: RoomInfo<R, L>
  rules: string
  levels: readonly L[]
  /** What went wrong, worded. */
  problem: string | null
  send: (message: Outgoing<R, L, Request>) => void
  leave: () => void
  /** Below the box (how to play). */
  after?: ReactNode
}) {
  const { t } = useTranslation()
  const present = room.seats.filter((s) => s.connected).length
  const watching = room.watching !== undefined
  const isHost = (watching ? room.watchers[room.watching!] : room.seats[room.you ?? -1])?.host ?? false
  const full = room.seats.length >= room.maxSeats
  const bot = (level: L) => t(`bots.bot.${level}` as 'bots.bot.easy')
  return (
    <div className="w-full">
      <Panel className="mx-auto max-w-lg">
        <p className="text-sm text-slate-400">{t('lobby.code')}</p>
        <p className="mt-1 font-mono text-5xl font-bold tracking-[0.2em] text-teal-300">{room.code}</p>
        <p className="mt-3 text-sm text-slate-400">{rules}</p>
        <ul className="mt-6 space-y-2">
          {room.seats.map((seat, i) => (
            <li key={i} className="flex items-center gap-3 rounded-xl bg-white/5 px-3 py-2">
              <span className={`size-2 rounded-full ${seat.connected ? 'bg-emerald-400' : 'bg-slate-500'}`} />
              <span className="flex-1">
                {seat.name}
                {i === room.you && <span className="text-slate-400"> {t('lobby.you')}</span>}
              </span>
              {seat.bot && <span className="text-xs uppercase tracking-wide text-violet-300">{bot(seat.bot)}</span>}
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
              {levels.map((level) => (
                <Button
                  key={level}
                  look="quiet"
                  className="grow whitespace-nowrap px-2 py-1.5 text-sm"
                  onClick={() => send({ type: 'addBot', level })}
                >
                  + {t(`bots.level.${level}` as 'bots.level.easy')}
                </Button>
              ))}
            </div>
          </div>
        )}
        <div className="mt-6 flex gap-2">
          <Button onClick={leave}>{t('lobby.leave')}</Button>
          {isHost ? (
            <Button look="primary" className="flex-1" disabled={present < room.minSeats} onClick={() => send({ type: 'start' })}>
              {present < room.minSeats ? t('lobby.waitingForPlayers') : t('lobby.start', { count: present })}
            </Button>
          ) : (
            <p className="flex-1 self-center text-center text-sm text-slate-400">{t('lobby.waitingForHost')}</p>
          )}
        </div>
        {problem && (
          <div className="mt-4">
            <Note tone="bad">{problem}</Note>
          </div>
        )}
      </Panel>
      {after}
    </div>
  )
}

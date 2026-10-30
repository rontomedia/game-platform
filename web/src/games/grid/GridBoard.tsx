import { motion } from 'motion/react'
import { useCallback, useState, type CSSProperties } from 'react'
import { useTranslation } from 'react-i18next'

import { LeaveMenu } from '../../platform/LeaveMenu.tsx'
import type { Settings } from '../../platform/settings.ts'
import { SettingsMenu } from '../../platform/SettingsMenu.tsx'
import { Button, Note } from '../../platform/ui.tsx'
import { useNow } from '../../platform/useNow.ts'
import type { Status } from '../../platform/useRoom.ts'
import { WatchBar } from '../../platform/WatchBar.tsx'
import type { GridOutgoing, GridRoom } from './protocol.ts'
import { StoneBoard, StoneFace, type PaperLines } from './StoneBoard.tsx'
import type { GridGame } from './useGrid.ts'
import { checkPlace, type Cell, type Square } from './wasm/engine.js'

/** Seconds left at which the clock turns amber. */
const LOW_TIME = 10

/**
 * The grid game's board, drawn as on paper (`.paper` in index.css; a light
 * or dark sheet, `Settings.paper`): the players and their clocks, the grid
 * (a mark goes where the player clicks, checked first in the browser),
 * what's going on, and for watchers the watch bar.
 */
export function GridBoard({
  game,
  room,
  status,
  problem,
  settings,
  onSettings,
  send,
  leave,
}: {
  game: GridGame
  room: GridRoom
  status: Status
  /** What went wrong, worded. */
  problem: string | null
  settings: Settings
  onSettings: (change: Partial<Settings>) => void
  send: (message: GridOutgoing) => void
  leave: () => void
}) {
  const { t } = useTranslation()
  const now = useNow()
  const { view } = game
  const { rules, outcome } = view
  const yourTurn = view.you === view.current && !outcome
  const name = (seat: number) => room.seats[seat]?.name ?? t('board.player', { n: seat + 1 })
  // A mark refused here before it's sent, so the reason shows at once, and
  // one sent: each for the state it was made in (the next clears it).
  const [refused, setRefused] = useState<{ version: number; text: string } | null>(null)
  const [sentIn, setSentIn] = useState<number | null>(null)
  const local = refused?.version === game.version ? refused.text : null
  // No second click while a mark is on its way.
  const sent = sentIn === game.version

  const place = (cell: Cell) => {
    if (sent) return
    const verdict = checkPlace(view, cell)
    if (verdict.verdict === 'rejected') return setRefused({ version: game.version, text: t(`gridError.${verdict.error.kind}`) })
    setSentIn(game.version)
    send({ type: 'place', cell })
  }

  /** Seconds left in `seat`'s turn, and in their bank. */
  const clock = (seat: number) => {
    const current = seat === view.current && !outcome
    const turn = current && game.turnEnds !== undefined ? Math.max(0, Math.ceil((game.turnEnds - now) / 1000)) : undefined
    const overdue = current && game.turnEnds !== undefined ? Math.max(0, now - game.turnEnds) : 0
    const bankMs = game.banks?.[seat]
    const bank = bankMs === undefined ? undefined : Math.max(0, Math.ceil((bankMs - overdue) / 1000))
    return { turn, bank }
  }

  const winning = new Set(outcome?.line.map((cell) => cell.row * rules.width + cell.col))
  const last = view.moves.at(-1)
  const lastIndex = last && last.row * rules.width + last.col
  // Tic-Tac-Toe's grid is drawn as by hand, inner lines only, marks in its
  // cells; a big one (Gomoku) is a board of lines with stones on their
  // intersections, drawn over the paper's own lines (`paper`).
  const stones = rules.width * rules.height > 25
  const ink = '4px solid var(--grid-ink)'
  const [paper, setPaper] = useState<PaperLines | null>(null)
  const onPaper = useCallback(
    (lines: PaperLines) =>
      setPaper((old) => (old && old.size === lines.size && old.x === lines.x && old.y === lines.y ? old : lines)),
    [],
  )
  const sheet: CSSProperties | undefined =
    stones && paper
      ? { backgroundSize: `${paper.size}px ${paper.size}px`, backgroundPosition: `${paper.x}px ${paper.y}px` }
      : undefined

  const note = outcome
    ? outcome.winner === undefined
      ? t('grid.draw')
      : outcome.winner === view.you
        ? t('results.youWon')
        : t('results.won', { names: name(outcome.winner), count: 1 })
    : room.paused
      ? t('board.paused', { name: name(view.current) })
      : yourTurn
        ? t(stones ? 'grid.yourTurnStone' : 'grid.yourTurn')
        : t('board.playing', { name: name(view.current) })
  const shown = local ?? problem

  return (
    <div className={`paper paper-${settings.paper}`} style={sheet}>
      <div className="mx-auto flex h-dvh max-w-5xl flex-col gap-3 p-3 sm:gap-4 sm:p-4">
        <header className="flex items-center gap-3">
          <div className="hidden shrink-0 items-baseline gap-2 md:flex">
            <span className="text-lg font-bold tracking-tight">{t(`grid.${rules.ruleSet}`)}</span>
            <span className="ink-soft font-mono text-xs tracking-widest">{room.code}</span>
          </div>
          <div className="flex min-w-0 flex-1 gap-2 overflow-x-auto p-0.5 [scrollbar-width:none] md:justify-center">
            {[0, 1].map((seat) => (
              <PlayerChip
                key={seat}
                name={name(seat)}
                mark={seat === 0 ? 'x' : 'o'}
                stones={stones}
                you={seat === view.you}
                bot={room.seats[seat]?.bot !== undefined}
                connected={room.seats[seat]?.connected ?? false}
                current={seat === view.current && !outcome}
                winner={outcome?.winner === seat}
                {...clock(seat)}
              />
            ))}
          </div>
          <span
            className={`size-2.5 shrink-0 rounded-full ${status === 'open' ? 'bg-emerald-500' : status === 'connecting' ? 'animate-pulse bg-amber-400' : 'bg-rose-500'}`}
            title={t(`app.status.${status}`)}
          />
          <div className="paper-tools flex items-center">
            <SettingsMenu settings={settings} onChange={onSettings} tiles={false} paper />
            <LeaveMenu standIn={room.options.standIn} ask={view.you !== undefined && !outcome} leave={leave} />
          </div>
        </header>

        <main className="grid min-h-0 flex-1 place-items-center p-2 sm:p-6">
          {stones ? (
            <StoneBoard
              view={view}
              preview={yourTurn && !sent ? (view.you === 0 ? 'black' : 'white') : undefined}
              winning={outcome?.line ?? []}
              last={last}
              onPaper={onPaper}
              onPlace={place}
            />
          ) : (
          <div
            className="grid w-[min(100%,calc(100dvh-15rem))]"
            style={{ gridTemplateColumns: `repeat(${rules.width}, minmax(0, 1fr))`, aspectRatio: `${rules.width} / ${rules.height}` }}
            dir="ltr"
            role="grid"
            aria-label={t(`grid.${rules.ruleSet}`)}
          >
            {view.board.map((square, index) => {
              const cell = { row: Math.floor(index / rules.width), col: index % rules.width }
              // By hand: no lines along the edge.
              const lines: CSSProperties = {
                borderRight: cell.col < rules.width - 1 ? ink : undefined,
                borderBottom: cell.row < rules.height - 1 ? ink : undefined,
              }
              return (
                <GridCell
                  key={index}
                  square={square}
                  preview={yourTurn && square === 'empty' && !sent ? (view.you === 0 ? 'x' : 'o') : undefined}
                  winning={winning.has(index)}
                  last={index === lastIndex}
                  pen={2.6}
                  style={lines}
                  onClick={() => place(cell)}
                />
              )
            })}
          </div>
          )}
        </main>

        {view.you === undefined ? (
          <WatchBar room={room} over={!!outcome} note={note} noteColor="" send={send} />
        ) : (
          <section className="paper-card flex flex-wrap items-center gap-x-4 gap-y-2 rounded-2xl p-3 sm:p-4">
            <p className="min-w-0 flex-1 text-sm font-medium" style={yourTurn ? { color: 'var(--pen-x)' } : undefined}>
              {note}
            </p>
            {shown && <Note tone="bad">{shown}</Note>}
            {outcome && (
              <Button look="primary" onClick={leave}>
                {t('results.leave')}
              </Button>
            )}
          </section>
        )}
      </div>
    </div>
  )
}

/** A cell: empty (with the player's mark faint under the pointer on their turn), or a mark. */
function GridCell({
  square,
  preview,
  winning,
  last,
  pen,
  style,
  onClick,
}: {
  square: Square
  preview?: 'x' | 'o'
  winning: boolean
  last: boolean
  /** The pen's width, in the mark's 24-unit box. */
  pen: number
  style: CSSProperties
  onClick: () => void
}) {
  return (
    <button
      role="gridcell"
      onClick={onClick}
      disabled={square !== 'empty'}
      aria-label={square === 'empty' ? undefined : square.toUpperCase()}
      title={square === 'empty' ? undefined : square.toUpperCase()}
      style={{ ...style, background: winning ? 'var(--highlight)' : last ? 'var(--touch)' : undefined }}
      className={`group relative grid place-items-center transition-colors ${preview ? 'cursor-pointer hover:bg-[var(--touch)]' : 'cursor-default'}`}
    >
      {square !== 'empty' ? (
        <span className="size-[70%]">
          <Mark mark={square} pen={pen} draw />
        </span>
      ) : (
        preview && (
          <span className="size-[70%] opacity-0 transition-opacity group-hover:opacity-25" aria-hidden>
            <Mark mark={preview} pen={pen} />
          </span>
        )
      )}
    </button>
  )
}

/** X (seat 0) and O (seat 1) as in pen, slightly by hand; `draw`: stroke by stroke as it appears. */
function Mark({ mark, pen = 2.6, draw = false }: { mark: 'x' | 'o'; pen?: number; draw?: boolean }) {
  const strokes =
    mark === 'x'
      ? ['M5.6 5.3C9.4 9.6 13.8 14 18.7 18.8', 'M18.5 5.5C14.4 9.7 9.8 14.4 5.4 18.6']
      : ['M12.4 4.7C16.8 4.6 19.5 7.9 19.3 12.3C19.1 16.6 15.8 19.4 11.8 19.3C7.6 19.2 4.7 16 4.8 11.9C4.9 8.2 7.5 5.1 11.6 4.6']
  return (
    <svg viewBox="0 0 24 24" className="size-full" fill="none" aria-hidden>
      {strokes.map((d, i) => (
        <motion.path
          key={i}
          d={d}
          stroke={mark === 'x' ? 'var(--pen-x)' : 'var(--pen-o)'}
          strokeWidth={pen}
          strokeLinecap="round"
          initial={draw ? { pathLength: 0 } : false}
          animate={{ pathLength: 1 }}
          transition={{ duration: mark === 'x' ? 0.16 : 0.32, delay: i * 0.14, ease: 'easeOut' }}
        />
      ))}
    </svg>
  )
}

/** A player: their mark, name, and clock while it's their turn. */
function PlayerChip({
  name,
  mark,
  stones,
  you,
  bot,
  connected,
  current,
  winner,
  turn,
  bank,
}: {
  name: string
  mark: 'x' | 'o'
  /** Shown as a stone (Gomoku), black for X. */
  stones: boolean
  you: boolean
  bot: boolean
  connected: boolean
  current: boolean
  winner: boolean
  turn?: number
  bank?: number
}) {
  const { t } = useTranslation()
  const banking = current && turn === 0 && bank !== undefined && bank > 0
  const seconds = banking ? bank : turn
  const low = seconds !== undefined && seconds <= LOW_TIME
  return (
    <div
      className={`paper-card flex shrink-0 items-center gap-2.5 rounded-2xl p-1.5 pe-3 transition ${connected ? '' : 'opacity-50'}`}
      style={
        winner
          ? { background: 'var(--highlight)' }
          : current
            ? { boxShadow: `0 0 0 2px ${stones ? 'var(--ink)' : mark === 'x' ? 'var(--pen-x)' : 'var(--pen-o)'}` }
            : undefined
      }
    >
      <span className="grid size-9 place-items-center p-1.5">
        {stones ? <StoneFace stone={mark === 'x' ? 'black' : 'white'} /> : <Mark mark={mark} />}
      </span>
      <div className="leading-tight">
        <div className="max-w-32 truncate text-sm font-semibold">
          {name}
          {you && <span className="ink-soft font-normal"> · {t('board.you')}</span>}
          {bot && <span className="ink-soft font-normal"> · {t('board.bot')}</span>}
        </div>
        <div className="ink-soft text-xs tabular-nums">
          {current && seconds !== undefined ? (
            <span className={low ? 'font-semibold text-amber-600' : 'font-medium'}>
              {banking ? t('board.bank', { time: minutes(seconds) }) : t('board.seconds', { n: seconds })}
            </span>
          ) : connected ? (
            stones ? t(mark === 'x' ? 'grid.black' : 'grid.white') : mark.toUpperCase()
          ) : (
            t('board.away')
          )}
        </div>
      </div>
    </div>
  )
}

const minutes = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`

import { motion } from 'motion/react'
import { useCallback, useEffect, useState, type CSSProperties } from 'react'
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
import { Mark, type Shape } from './marks.tsx'
import { checkPlace, type Cell, type GridView, type Letter, type Line, type Square } from './wasm/engine.js'

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
  // S.O.S: the letter the player writes, S or O (also the keys S and O).
  const sos = rules.ruleSet === 'sos'
  const [letter, setLetter] = useState<Letter>('s')
  useEffect(() => {
    if (!sos) return
    const key = (event: KeyboardEvent) => {
      const typed = event.key.toLowerCase()
      if ((typed === 's' || typed === 'o') && !event.ctrlKey && !event.metaKey && !event.altKey) setLetter(typed)
    }
    window.addEventListener('keydown', key)
    return () => window.removeEventListener('keydown', key)
  }, [sos])

  const place = (cell: Cell) => {
    if (sent) return
    const placing = sos ? { cell, letter } : { cell }
    const verdict = checkPlace(view, placing)
    if (verdict.verdict === 'rejected') return setRefused({ version: game.version, text: t(`gridError.${verdict.error.kind}`) })
    setSentIn(game.version)
    send({ type: 'place', ...placing })
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

  const columns = rules.width * (rules.ruleSet === 'ultimate' ? 3 : rules.boards)
  const winning = new Set(outcome?.line.map((cell) => cell.row * columns + cell.col))
  const last = view.moves.at(-1)
  const lastIndex = last && last.row * columns + last.col
  // Tic-Tac-Toe's grid is drawn as by hand, inner lines only, marks in its
  // cells; Notakto's boards likewise, side by side; a big one (Gomoku) is
  // a board of lines with stones on their intersections, drawn over the
  // paper's own lines (`paper`).
  const notakto = rules.ruleSet === 'notakto'
  const ultimate = rules.ruleSet === 'ultimate'
  const stones = (rules.ruleSet === 'ticTacToe' || rules.ruleSet === 'gomoku') && rules.width * rules.height > 25
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
  const preview = yourTurn && !sent ? (sos ? letter : notakto || view.you === 0 ? 'x' : 'o') : undefined
  /** The squares of board `b` (of Notakto's, side by side, or Ultimate's, 3 × 3), row by row, with their cells. */
  const boardCells = (b: number) =>
    Array.from({ length: rules.width * rules.height }, (_, i) => {
      const [across, down] = ultimate ? [b % 3, Math.floor(b / 3)] : [b, 0]
      const cell = { row: down * rules.height + Math.floor(i / rules.width), col: across * rules.width + (i % rules.width) }
      const index = cell.row * columns + cell.col
      return { cell, index, square: view.board[index] }
    })

  // In S.O.S, a player who just scored writes again.
  const again = sos && yourTurn && view.lines.at(-1)?.seat === view.you && view.lines.at(-1)?.cells.some((c) => c.row === last?.row && c.col === last?.col)
  const note = outcome
    ? outcome.winner === undefined
      ? t(sos ? 'grid.drawSos' : 'grid.draw')
      : outcome.winner === view.you
        ? t('results.youWon')
        : t('results.won', { names: name(outcome.winner), count: 1 })
    : room.paused
      ? t('board.paused', { name: name(view.current) })
      : yourTurn
        ? t(
            again
              ? 'grid.again'
              : sos
                ? 'grid.yourTurnSos'
                : ultimate
                  ? view.nextBoard === undefined
                    ? 'grid.yourTurnAnyBoard'
                    : 'grid.yourTurnBoard'
                  : stones
                    ? 'grid.yourTurnStone'
                    : 'grid.yourTurn',
          )
        : t('board.playing', { name: name(view.current) })
  const shown = local ?? problem

  return (
    <div className={`paper paper-${settings.paper} min-h-dvh`} style={sheet}>
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
                mark={seat === 0 || notakto ? 'x' : 'o'}
                stones={stones}
                score={view.scores?.[seat]}
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
          ) : ultimate ? (
            <UltimateBoard
              view={view}
              cells={boardCells}
              preview={preview}
              lastIndex={lastIndex}
              onPlace={place}
            />
          ) : notakto ? (
            <NotaktoBoards
              boards={rules.boards}
              finished={view.lines}
              cells={boardCells}
              preview={preview}
              winning={winning}
              lastIndex={lastIndex}
              onPlace={place}
            />
          ) : (
            <div className="relative w-[min(100%,calc(100dvh-15rem))]">
              <MarkGrid
                className="w-full"
                width={rules.width}
                height={rules.height}
                cells={boardCells(0)}
                preview={preview}
                winning={winning}
                lastIndex={lastIndex}
                label={t(`grid.${rules.ruleSet}`)}
                letters={sos}
                pen={sos ? 3 : 2.6}
                ink={sos ? '2px solid var(--grid-ink)' : undefined}
                onPlace={place}
              />
              {sos && <Strokes width={rules.width} height={rules.height} lines={view.lines} />}
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
            {sos && !outcome && <LetterPicker letter={letter} onChange={setLetter} />}
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

type BoardCell = { cell: Cell; index: number; square: Square }

/**
 * A grid drawn by hand, inner lines only, marks in its cells: Tic-Tac-Toe's,
 * or one of Notakto's boards. Its rows are as fixed as its columns: a mark
 * being drawn mustn't size its row.
 */
function MarkGrid({
  className,
  width,
  height,
  cells,
  preview,
  winning,
  lastIndex,
  label,
  pen = 2.6,
  ink = '4px solid var(--grid-ink)',
  closed = false,
  letters = false,
  onPlace,
}: {
  className: string
  width: number
  height: number
  cells: BoardCell[]
  preview?: Shape
  winning: Set<number>
  lastIndex?: number
  label: string
  pen?: number
  ink?: string
  /** Finished: no cell may be marked. */
  closed?: boolean
  /** S.O.S: the cells hold letters, in ink. */
  letters?: boolean
  onPlace: (cell: Cell) => void
}) {
  return (
    <div
      className={`grid ${className}`}
      style={{
        gridTemplateColumns: `repeat(${width}, minmax(0, 1fr))`,
        gridTemplateRows: `repeat(${height}, minmax(0, 1fr))`,
        aspectRatio: `${width} / ${height}`,
      }}
      dir="ltr"
      role="grid"
      aria-label={label}
    >
      {cells.map(({ cell, index, square }, i) => (
        <GridCell
          key={index}
          square={square}
          closed={closed}
          letters={letters}
          preview={square === 'empty' && !closed ? preview : undefined}
          winning={winning.has(index)}
          last={index === lastIndex}
          pen={pen}
          // By hand: no lines along the edge.
          style={{
            borderRight: i % width < width - 1 ? ink : undefined,
            borderBottom: Math.floor(i / width) < height - 1 ? ink : undefined,
          }}
          onClick={() => onPlace(cell)}
        />
      ))}
    </div>
  )
}

/**
 * Notakto's boards side by side (four in two rows), all marks X; a finished
 * board has its line struck through and fades, as nobody may play there.
 */
function NotaktoBoards({
  boards,
  finished,
  cells,
  preview,
  winning,
  lastIndex,
  onPlace,
}: {
  boards: number
  finished: Line[]
  cells: (board: number) => BoardCell[]
  preview?: Shape
  winning: Set<number>
  lastIndex?: number
  onPlace: (cell: Cell) => void
}) {
  const { t } = useTranslation()
  const { across, size } = useLayout(boards)
  return (
    <div className="flex flex-wrap justify-center gap-10" style={{ width: across * size + (across - 1) * 40 }}>
      {Array.from({ length: boards }, (_, b) => {
        const line = finished.find((line) => line.cells[0].col >= b * 3 && line.cells[0].col < b * 3 + 3)
        return (
          <div key={b} className="relative transition-opacity" style={{ width: size, opacity: line && !line.cells.some((cell) => winning.has(cell.row * boards * 3 + cell.col)) ? 0.55 : 1 }}>
            <MarkGrid
              className="w-full"
              width={3}
              height={3}
              cells={cells(b)}
              preview={preview}
              closed={!!line}
              winning={winning}
              lastIndex={lastIndex}
              label={t('grid.board', { n: b + 1 })}
              pen={3}
              ink="3px solid var(--grid-ink)"
              onPlace={onPlace}
            />
            {line && <StrikeThrough cells={line.cells} offset={b * 3} />}
          </div>
        )
      })}
    </div>
  )
}

/**
 * Ultimate Tic-Tac-Toe's nine boards in a 3 × 3, between thick lines: the
 * one the player to move is sent to highlighted, a won board under its
 * winner's big mark, a full one faded; three won in a row struck through.
 */
function UltimateBoard({
  view,
  cells,
  preview,
  lastIndex,
  onPlace,
}: {
  view: GridView
  cells: (board: number) => BoardCell[]
  preview?: Shape
  lastIndex?: number
  onPlace: (cell: Cell) => void
}) {
  const { t } = useTranslation()
  const ink = '5px solid var(--grid-ink)'
  const none = new Set<number>()
  const { outcome } = view
  return (
    <div className="relative grid aspect-square w-[min(100%,calc(100dvh-15rem))] grid-cols-3 grid-rows-3" dir="ltr">
      {Array.from({ length: 9 }, (_, b) => {
        const squares = cells(b)
        const won = view.lines.find((line) => boardOf(line.cells[0]) === b)
        const full = squares.every(({ square }) => square !== 'empty')
        const closed = !!won || full
        const target = !outcome && !closed && (view.nextBoard === undefined || view.nextBoard === b)
        return (
          <div
            key={b}
            className="relative p-[7%] transition-colors"
            style={{
              borderRight: b % 3 < 2 ? ink : undefined,
              borderBottom: b < 6 ? ink : undefined,
              background: target && view.nextBoard !== undefined ? 'var(--highlight)' : undefined,
            }}
          >
            <div style={{ opacity: won ? 0.3 : full ? 0.5 : 1 }}>
              <MarkGrid
                className="w-full"
                width={3}
                height={3}
                cells={squares}
                preview={target ? preview : undefined}
                winning={none}
                lastIndex={lastIndex}
                label={t('grid.board', { n: b + 1 })}
                pen={3.2}
                ink="2px solid var(--grid-ink)"
                closed={!target}
                onPlace={onPlace}
              />
            </div>
            {won && (
              <span className="pointer-events-none absolute inset-[10%]">
                <Mark mark={won.seat === 0 ? 'x' : 'o'} pen={2.2} draw />
              </span>
            )}
          </div>
        )
      })}
      {outcome && outcome.line.length > 0 && <BigLine cells={outcome.line} />}
    </div>
  )
}

/** Ultimate's board (0-8, row by row) a cell of the 9 × 9 grid is on. */
const boardOf = (cell: Cell) => Math.floor(cell.row / 3) * 3 + Math.floor(cell.col / 3)

/** A pen stroke through three won boards' middles, over the 9 × 9 grid. */
function BigLine({ cells }: { cells: Cell[] }) {
  const [first, end] = [cells[0], cells[cells.length - 1]]
  const [x1, y1, x2, y2] = [first.col * 10 + 5, first.row * 10 + 5, end.col * 10 + 5, end.row * 10 + 5]
  const [dx, dy] = [(x2 - x1) * 0.2, (y2 - y1) * 0.2]
  return (
    <svg viewBox="0 0 90 90" className="pointer-events-none absolute inset-0 size-full" aria-hidden>
      <motion.path
        d={`M${x1 - dx} ${y1 - dy}L${x2 + dx} ${y2 + dy}`}
        stroke="var(--ink)"
        strokeWidth={2.2}
        strokeLinecap="round"
        initial={{ pathLength: 0 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 0.45, delay: 0.5, ease: 'easeOut' }}
      />
    </svg>
  )
}

/**
 * How many of `boards` boards go across, and how wide each is (px): of the
 * ways to lay them out in rows (4 only as 4, 2 × 2 or 1 a row), the one
 * that makes them biggest in the window, below the header and status line.
 */
function useLayout(boards: number) {
  const [window_, setWindow] = useState(() => [window.innerWidth, window.innerHeight])
  useEffect(() => {
    const resize = () => setWindow([window.innerWidth, window.innerHeight])
    window.addEventListener('resize', resize)
    return () => window.removeEventListener('resize', resize)
  }, [])
  const [width, height] = window_
  const [gap, rem] = [40, 16]
  const layouts = Array.from({ length: boards }, (_, i) => i + 1).filter((across) => boards !== 4 || across !== 3)
  return layouts
    .map((across) => {
      const down = Math.ceil(boards / across)
      const fit = Math.min((width - 3 * rem - (across - 1) * gap) / across, (height - 15 * rem - (down - 1) * gap) / down, 18 * rem)
      return { across, size: Math.max(64, Math.floor(fit)) }
    })
    .reduce((best, layout) => (layout.size > best.size ? layout : best))
}

/** A pen stroke through a finished board's line, end to end, drawn as it appears. */
function StrikeThrough({ cells, offset }: { cells: Cell[]; offset: number }) {
  const [first, end] = [cells[0], cells[cells.length - 1]]
  // In thirds of the board, from cell centres, overshooting a little at both ends.
  const at = (cell: Cell) => [(cell.col - offset) * 10 + 5, cell.row * 10 + 5]
  const [x1, y1] = at(first)
  const [x2, y2] = at(end)
  const [dx, dy] = [(x2 - x1) * 0.12, (y2 - y1) * 0.12]
  return (
    <svg viewBox="0 0 30 30" className="pointer-events-none absolute inset-0 size-full" aria-hidden>
      <motion.path
        d={`M${x1 - dx} ${y1 - dy}L${x2 + dx} ${y2 + dy}`}
        stroke="var(--ink)"
        strokeWidth={1.1}
        strokeLinecap="round"
        initial={{ pathLength: 0 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 0.3, delay: 0.25, ease: 'easeOut' }}
      />
    </svg>
  )
}

/** A cell: empty (with the player's mark faint under the pointer on their turn), or a mark. */
function GridCell({
  square,
  closed = false,
  letters = false,
  preview,
  winning,
  last,
  pen,
  style,
  onClick,
}: {
  square: Square
  closed?: boolean
  letters?: boolean
  preview?: Shape
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
      disabled={square !== 'empty' || closed}
      aria-label={square === 'empty' ? undefined : square.toUpperCase()}
      title={square === 'empty' ? undefined : square.toUpperCase()}
      style={{ ...style, background: winning ? 'var(--highlight)' : last ? 'var(--touch)' : undefined }}
      className={`group relative grid place-items-center transition-colors ${preview ? 'cursor-pointer hover:bg-[var(--touch)]' : 'cursor-default'}`}
    >
      {square !== 'empty' ? (
        <span className="absolute inset-[15%]">
          <Mark mark={square} pen={pen} ink={letters} draw />
        </span>
      ) : (
        preview && (
          <span className="absolute inset-[15%] opacity-0 transition-opacity group-hover:opacity-25" aria-hidden>
            <Mark mark={preview} pen={pen} ink={letters} />
          </span>
        )
      )}
    </button>
  )
}

/**
 * S.O.S's S-O-Ss struck through in the pen of who made them, over a grid
 * of `width` × `height` cells.
 */
function Strokes({ width, height, lines }: { width: number; height: number; lines: Line[] }) {
  const at = (cell: Cell) => [cell.col * 10 + 5, cell.row * 10 + 5]
  return (
    <svg viewBox={`0 0 ${width * 10} ${height * 10}`} className="pointer-events-none absolute inset-0 size-full" aria-hidden>
      {lines.map(({ seat, cells }) => {
        const [x1, y1] = at(cells[0])
        const [x2, y2] = at(cells[cells.length - 1])
        const [dx, dy] = [(x2 - x1) * 0.14, (y2 - y1) * 0.14]
        return (
          <motion.path
            key={cells.map((c) => `${c.row}.${c.col}`).join('-')}
            d={`M${x1 - dx} ${y1 - dy}L${x2 + dx} ${y2 + dy}`}
            stroke={seat === 0 ? 'var(--pen-x)' : 'var(--pen-o)'}
            strokeOpacity={0.6}
            strokeWidth={0.75}
            strokeLinecap="round"
            initial={{ pathLength: 0 }}
            animate={{ pathLength: 1 }}
            transition={{ duration: 0.35, delay: 0.3, ease: 'easeOut' }}
          />
        )
      })}
    </svg>
  )
}

/** S.O.S: the letter the player writes, S or O. */
function LetterPicker({ letter, onChange }: { letter: Letter; onChange: (letter: Letter) => void }) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-1.5" role="group" aria-label={t('grid.letter')}>
      {(['s', 'o'] as const).map((choice) => (
        <button
          key={choice}
          onClick={() => onChange(choice)}
          aria-pressed={letter === choice}
          title={t('grid.letterKey', { letter: choice.toUpperCase() })}
          className={`grid size-11 cursor-pointer place-items-center rounded-xl p-2 transition ${letter === choice ? 'bg-[var(--highlight)] ring-2 ring-[var(--ink)]' : 'hover:bg-[var(--touch)]'}`}
        >
          <Mark mark={choice} pen={3} ink />
        </button>
      ))}
    </div>
  )
}

/** A player: their mark (or S.O.S points), name, and clock while it's their turn. */
function PlayerChip({
  name,
  mark,
  stones,
  score,
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
  /** S.O.S's points, shown in place of the mark. */
  score?: number
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
        {score !== undefined ? (
          <span className="text-xl font-bold tabular-nums" style={{ color: mark === 'x' ? 'var(--pen-x)' : 'var(--pen-o)' }}>
            {score}
          </span>
        ) : stones ? (
          <StoneFace stone={mark === 'x' ? 'black' : 'white'} />
        ) : (
          <Mark mark={mark} />
        )}
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
            score !== undefined ? t('grid.points') : stones ? t(mark === 'x' ? 'grid.black' : 'grid.white') : mark.toUpperCase()
          ) : (
            t('board.away')
          )}
        </div>
      </div>
    </div>
  )
}

const minutes = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`

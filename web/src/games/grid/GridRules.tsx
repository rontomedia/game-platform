import { motion } from 'motion/react'
import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode, type Ref, type RefObject } from 'react'
import { useTranslation } from 'react-i18next'

import { HowToPlay, RuleList, RuleSection, RuleText } from '../../platform/HowToPlay.tsx'
import { useSettings } from '../../platform/settings.ts'
import { useScene } from '../../platform/useScene.ts'
import { Mark } from './marks.tsx'
import { StoneFace } from './StoneBoard.tsx'
import type { RuleSet } from './wasm/engine.js'

/** "How to play" for a grid rule set, under its home and lobby box. */
export function GridHowToPlay({ ruleSet }: { ruleSet: RuleSet }) {
  const { t } = useTranslation()
  return <HowToPlay game={t(`grid.${ruleSet}`)}>{ruleSet === 'gomoku' ? <GomokuRules /> : <TicTacToeRules />}</HowToPlay>
}

function TicTacToeRules() {
  const { t } = useTranslation()
  return (
    <>
      <RuleSection title={t('rules.classic.aimTitle')}>
        <RuleText>{t('rules.ticTacToe.aim')}</RuleText>
        {/* X takes the top row while O looks elsewhere. */}
        <Marks moves={[0, 4, 1, 8, 2]} line={[0, 1, 2]} caption={t('rules.ticTacToe.aimCaption')} />
      </RuleSection>
      <RuleSection title={t('rules.ticTacToe.turnTitle')}>
        <RuleText>{t('rules.ticTacToe.turn')}</RuleText>
      </RuleSection>
      <RuleSection title={t('rules.ticTacToe.tipsTitle')}>
        <RuleText>{t('rules.ticTacToe.tips')}</RuleText>
        {/* X's third mark (bottom left) threatens the left column and a diagonal: O can block only one. */}
        <Marks moves={[0, 1, 4, 8, 6, 3, 2]} line={[2, 4, 6]} caption={t('rules.ticTacToe.tipsCaption')} />
      </RuleSection>
      <RuleSection title={t('rules.ticTacToe.drawTitle')}>
        <RuleText>{t('rules.ticTacToe.draw')}</RuleText>
        <Marks moves={[4, 0, 2, 6, 3, 5, 7, 1, 8]} from={5} caption={t('rules.ticTacToe.drawCaption')} />
      </RuleSection>
      <RuleSection title={t('rules.classic.controlsTitle')}>
        <RuleList items={[t('rules.grid.controlsMark'), t('rules.grid.controlsTime'), t('rules.grid.controlsPaper')]} />
      </RuleSection>
    </>
  )
}

function GomokuRules() {
  const { t } = useTranslation()
  return (
    <>
      <RuleSection title={t('rules.classic.aimTitle')}>
        <RuleText>{t('rules.gomoku.aim')}</RuleText>
        {/* Black's open three becomes an open four; White blocks one end, Black finishes at the other. */}
        <Stones
          moves={[[4, 4], [3, 3], [4, 5], [5, 3], [4, 6], [3, 6], [4, 3], [4, 2], [4, 7]]}
          from={4}
          line={[[4, 3], [4, 7]]}
          caption={t('rules.gomoku.aimCaption')}
        />
      </RuleSection>
      <RuleSection title={t('rules.ticTacToe.turnTitle')}>
        <RuleText>{t('rules.gomoku.turn')}</RuleText>
      </RuleSection>
      <RuleSection title={t('rules.gomoku.threatsTitle')}>
        <RuleText>{t('rules.gomoku.threats')}</RuleText>
        {/* White's three on the diagonal, open at both ends: Black takes one end. */}
        <Stones
          extra={[[3, 3, 'w'], [4, 4, 'w'], [5, 5, 'w'], [4, 5, 'b'], [3, 5, 'b']]}
          moves={[[2, 2]]}
          caption={t('rules.gomoku.threatsCaption')}
        />
      </RuleSection>
      <RuleSection title={t('rules.ticTacToe.drawTitle')}>
        <RuleText>{t('rules.gomoku.draw')}</RuleText>
      </RuleSection>
      <RuleSection title={t('rules.classic.controlsTitle')}>
        <RuleList items={[t('rules.grid.controlsStone'), t('rules.grid.controlsTimeStone'), t('rules.grid.controlsPaper')]} />
      </RuleSection>
    </>
  )
}

/** A little sheet of paper, as the player has set it. */
function Sheet({ children, style, sheet }: { children: ReactNode; style?: CSSProperties; sheet?: Ref<HTMLDivElement> }) {
  const [settings] = useSettings()
  return (
    <div ref={sheet} dir="ltr" className={`paper paper-${settings.paper} grid place-items-center rounded-xl p-4`} style={style} aria-hidden>
      {children}
    </div>
  )
}

/**
 * The paper's lines through a board's (`board` in `sheet`), `pitch` apart,
 * as on the real board: wherever the board sits in its sheet.
 */
function usePaperLines(sheet: RefObject<HTMLDivElement | null>, board: RefObject<HTMLDivElement | null>, pitch: number) {
  const [offset, setOffset] = useState<[number, number] | null>(null)
  useLayoutEffect(() => {
    const outer = sheet.current
    const inner = board.current
    if (!outer || !inner) return
    const place = () => {
      const [a, b] = [outer.getBoundingClientRect(), inner.getBoundingClientRect()]
      const at = (from: number) => (((Math.round(from) + pitch / 2) % pitch) + pitch) % pitch
      setOffset([at(b.left - a.left), at(b.top - a.top)])
    }
    place()
    const observer = new ResizeObserver(place)
    observer.observe(outer)
    return () => observer.disconnect()
  }, [sheet, board, pitch])
  return offset && { backgroundSize: `${pitch}px ${pitch}px`, backgroundPosition: `${offset[0]}px ${offset[1]}px` }
}

/**
 * Tic-Tac-Toe played a mark at a time (`moves`: cells 0-8, X first), from
 * `from` marks in, over and over; `line` highlighted at the end.
 */
function Marks({ moves, from = 0, line, caption }: { moves: number[]; from?: number; line?: number[]; caption: string }) {
  const ref = useRef<HTMLElement>(null)
  const { step, round } = useScene(ref, moves.length - from + 1)
  const placed = moves.slice(0, from + step)
  const done = from + step === moves.length
  const ink = '3px solid var(--grid-ink)'
  return (
    <figure ref={ref} className="mt-3">
      <Sheet>
        <div className="grid size-32 grid-cols-3 grid-rows-3">
          {Array.from({ length: 9 }, (_, cell) => {
            const turn = placed.indexOf(cell)
            return (
              <div
                key={cell}
                className="relative"
                style={{
                  borderRight: cell % 3 < 2 ? ink : undefined,
                  borderBottom: cell < 6 ? ink : undefined,
                  background: done && line?.includes(cell) ? 'var(--highlight)' : undefined,
                }}
              >
                {turn >= 0 && (
                  <span key={round} className="absolute inset-[15%]">
                    <Mark mark={turn % 2 === 0 ? 'x' : 'o'} draw />
                  </span>
                )}
              </div>
            )
          })}
        </div>
      </Sheet>
      <figcaption className="mt-1.5 text-xs text-slate-400">{caption}</figcaption>
    </figure>
  )
}

/** Intersections of the little Gomoku board: a corner of the real one. */
const SIZE = 9
const PITCH = 22

/**
 * Gomoku played a stone at a time (`moves`: [row, col], Black first), from
 * `from` stones in; `extra` stones there from the start; the line from
 * `line[0]` to `line[1]` highlighted at the end.
 */
function Stones({
  moves,
  from = 0,
  line,
  caption,
  extra = [],
}: {
  moves: [number, number][]
  from?: number
  line?: [[number, number], [number, number]]
  caption: string
  extra?: [number, number, 'b' | 'w'][]
}) {
  const ref = useRef<HTMLElement>(null)
  const sheet = useRef<HTMLDivElement>(null)
  const board = useRef<HTMLDivElement>(null)
  const lines = usePaperLines(sheet, board, PITCH)
  const { step, round } = useScene(ref, moves.length - from + 1)
  const placed = moves.slice(0, from + step)
  const done = from + step === moves.length
  const at = (i: number) => i * PITCH + PITCH / 2
  const stones: [number, number, 'b' | 'w', boolean][] = [
    ...extra.map(([r, c, k]) => [r, c, k, false] as [number, number, 'b' | 'w', boolean]),
    ...placed.map(([r, c], i) => [r, c, i % 2 === 0 ? 'b' : 'w', i >= from] as [number, number, 'b' | 'w', boolean]),
  ]
  return (
    <figure ref={ref} className="mt-3">
      <Sheet sheet={sheet} style={lines ?? undefined}>
        <div ref={board} className="relative" style={{ width: SIZE * PITCH, height: SIZE * PITCH }}>
          <svg className="absolute inset-0 size-full">
            {done && line && (
              <line
                x1={at(line[0][1])}
                y1={at(line[0][0])}
                x2={at(line[1][1])}
                y2={at(line[1][0])}
                stroke="var(--highlight)"
                strokeWidth={PITCH * 0.8}
                strokeLinecap="round"
              />
            )}
            {Array.from({ length: SIZE }, (_, i) => (
              <g key={i} stroke="var(--grid-ink)" strokeWidth={1.5}>
                <line x1={at(i)} y1={at(0)} x2={at(i)} y2={at(SIZE - 1)} />
                <line x1={at(0)} y1={at(i)} x2={at(SIZE - 1)} y2={at(i)} />
              </g>
            ))}
          </svg>
          {stones.map(([r, c, k, fresh]) => (
            <motion.span
              key={`${round}-${r}-${c}`}
              className="absolute"
              style={{ left: at(c) - PITCH * 0.44, top: at(r) - PITCH * 0.44, width: PITCH * 0.88, height: PITCH * 0.88 }}
              initial={fresh ? { scale: 1.25, opacity: 0 } : false}
              animate={{ scale: 1, opacity: 1 }}
              transition={{ type: 'spring', stiffness: 520, damping: 28 }}
            >
              <StoneFace stone={k === 'b' ? 'black' : 'white'} />
            </motion.span>
          ))}
        </div>
      </Sheet>
      <figcaption className="mt-1.5 text-xs text-slate-400">{caption}</figcaption>
    </figure>
  )
}

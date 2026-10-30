import { motion } from 'motion/react'
import { useLayoutEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { Cell, GridView } from './wasm/engine.js'

/** Where the paper's lines run, so the board's are the same lines drawn over: their spacing and offset, in px. */
export interface PaperLines {
  size: number
  x: number
  y: number
}

/**
 * Gomoku's board: stones on the intersections of lines drawn over the
 * paper's own (`onPaper` tells the page where they must run). Black (seat
 * 0) moves first. A cell of the view is an intersection here.
 */
export function StoneBoard({
  view,
  preview,
  winning,
  last,
  onPaper,
  onPlace,
}: {
  view: GridView
  /** The player's stone to show faint under the pointer, on their turn. */
  preview?: Stone
  winning: Cell[]
  last?: Cell
  onPaper: (lines: PaperLines) => void
  onPlace: (cell: Cell) => void
}) {
  const { t } = useTranslation()
  const { width: cols, height: rows } = view.rules
  const area = useRef<HTMLDivElement>(null)
  const board = useRef<HTMLDivElement>(null)
  // A whole, even number of pixels apart, so lines fall on pixels like the paper's.
  const [pitch, setPitch] = useState(0)

  useLayoutEffect(() => {
    const element = area.current
    if (!element) return
    const measure = () => {
      const { width, height } = element.getBoundingClientRect()
      setPitch(Math.max(8, Math.floor(Math.min(width / cols, height / rows) / 2) * 2))
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [cols, rows])

  // Once laid out: the paper's lines through the board's.
  useLayoutEffect(() => {
    const element = board.current
    const paper = element?.closest('.paper')
    if (!element || !paper || pitch === 0) return
    const place = () => {
      const at = element.getBoundingClientRect()
      const sheet = paper.getBoundingClientRect()
      const offset = (from: number) => (((Math.round(from) + pitch / 2) % pitch) + pitch) % pitch
      onPaper({ size: pitch, x: offset(at.left - sheet.left), y: offset(at.top - sheet.top) })
    }
    place()
    // The board moves when its area does (the line below it grows at the end), not only with the window.
    const observer = new ResizeObserver(place)
    if (area.current) observer.observe(area.current)
    window.addEventListener('resize', place)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', place)
    }
  }, [pitch, onPaper])

  const centre = (i: number) => i * pitch + pitch / 2
  // Star points, as on a go board: 3 in from each corner, and the middle.
  const stars = cols >= 13 && rows >= 13 ? [3, cols - 4].flatMap((x) => [3, rows - 4].map((y) => [x, y])) : []
  if (cols % 2 && rows % 2 && cols >= 9) stars.push([(cols - 1) / 2, (rows - 1) / 2])
  const first = winning[0]
  const end = winning.at(-1)

  return (
    <div ref={area} className="grid size-full place-items-center">
      <div
        ref={board}
        className="relative"
        style={{ width: cols * pitch, height: rows * pitch, visibility: pitch ? 'visible' : 'hidden' }}
        dir="ltr"
        role="grid"
        aria-label={t('grid.gomoku')}
      >
        <svg className="pointer-events-none absolute inset-0 size-full" aria-hidden>
          {first && end && (
            <line
              x1={centre(first.col)}
              y1={centre(first.row)}
              x2={centre(end.col)}
              y2={centre(end.row)}
              stroke="var(--highlight)"
              strokeWidth={pitch * 0.8}
              strokeLinecap="round"
            />
          )}
          {Array.from({ length: cols }, (_, i) => (
            <line key={`c${i}`} x1={centre(i)} y1={centre(0)} x2={centre(i)} y2={centre(rows - 1)} stroke="var(--grid-ink)" strokeWidth={i === 0 || i === cols - 1 ? 2.5 : 1.75} />
          ))}
          {Array.from({ length: rows }, (_, i) => (
            <line key={`r${i}`} x1={centre(0)} y1={centre(i)} x2={centre(cols - 1)} y2={centre(i)} stroke="var(--grid-ink)" strokeWidth={i === 0 || i === rows - 1 ? 2.5 : 1.75} />
          ))}
          {stars.map(([x, y]) => (
            <circle key={`${x}-${y}`} cx={centre(x)} cy={centre(y)} r={Math.max(2.5, pitch * 0.09)} fill="var(--grid-ink)" />
          ))}
        </svg>
        <div className="absolute inset-0 grid" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)`, gridTemplateRows: `repeat(${rows}, 1fr)` }}>
          {view.board.map((square, index) => {
            const cell = { row: Math.floor(index / cols), col: index % cols }
            const stone = square === 'x' ? 'black' : square === 'o' ? 'white' : undefined
            const isLast = last?.row === cell.row && last.col === cell.col
            return (
              <button
                key={index}
                role="gridcell"
                onClick={() => onPlace(cell)}
                disabled={stone !== undefined}
                aria-label={stone && t(`grid.${stone}`)}
                className={`group grid place-items-center ${preview && !stone ? 'cursor-pointer' : 'cursor-default'}`}
              >
                {stone ? (
                  <motion.span
                    className="relative grid size-[88%] place-items-center"
                    initial={{ scale: 1.25, opacity: 0 }}
                    animate={{ scale: 1, opacity: 1 }}
                    transition={{ type: 'spring', stiffness: 520, damping: 28 }}
                  >
                    <StoneFace stone={stone} />
                    {isLast && <span className="absolute size-[26%] rounded-full bg-rose-500" aria-hidden />}
                  </motion.span>
                ) : (
                  preview && (
                    <span className="size-[88%] opacity-0 transition-opacity group-hover:opacity-40" aria-hidden>
                      <StoneFace stone={preview} />
                    </span>
                  )
                )}
              </button>
            )
          })}
        </div>
      </div>
    </div>
  )
}

export type Stone = 'black' | 'white'

/** A stone, shaded as round, with a rim that shows it on either paper (`--rim-*` in index.css). */
export function StoneFace({ stone }: { stone: Stone }) {
  return (
    <span
      className="block size-full rounded-full"
      style={{
        background:
          stone === 'black'
            ? 'radial-gradient(circle at 35% 30%, #6b6b6b, #1a1a1a 55%, #050505)'
            : 'radial-gradient(circle at 35% 30%, #ffffff, #eeece6 55%, #c8c4ba)',
        boxShadow: `0 0 0 1px var(--rim-${stone}), 0 2px 3px rgb(0 0 0 / 0.35)`,
      }}
    />
  )
}

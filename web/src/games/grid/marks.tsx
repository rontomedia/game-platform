import { motion } from 'motion/react'

/** What a cell may show: a player's mark, or S.O.S's letters. */
export type Shape = 'x' | 'o' | 's'

const STROKES: Record<Shape, string[]> = {
  x: ['M5.6 5.3C9.4 9.6 13.8 14 18.7 18.8', 'M18.5 5.5C14.4 9.7 9.8 14.4 5.4 18.6'],
  o: ['M12.4 4.7C16.8 4.6 19.5 7.9 19.3 12.3C19.1 16.6 15.8 19.4 11.8 19.3C7.6 19.2 4.7 16 4.8 11.9C4.9 8.2 7.5 5.1 11.6 4.6'],
  s: ['M16.9 6.6C15.6 5.2 13.9 4.6 12.1 4.7C9.4 4.8 7.4 6.3 7.5 8.5C7.6 12.9 16.9 11.1 16.8 15.6C16.7 18 14.6 19.4 11.9 19.3C9.8 19.3 8.1 18.5 6.9 17.1'],
}

/**
 * X (seat 0) and O (seat 1) as in pen, slightly by hand, or S.O.S's
 * letters in ink (`ink`, as they are nobody's); `draw`: stroke by stroke
 * as it appears.
 */
export function Mark({ mark, pen = 2.6, draw = false, ink = false }: { mark: Shape; pen?: number; draw?: boolean; ink?: boolean }) {
  const color = ink ? 'var(--ink)' : mark === 'x' ? 'var(--pen-x)' : 'var(--pen-o)'
  return (
    <svg viewBox="0 0 24 24" className="size-full" fill="none" aria-hidden>
      {STROKES[mark].map((d, i) => (
        <motion.path
          key={i}
          d={d}
          stroke={color}
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

import { motion } from 'motion/react'

/** X (seat 0) and O (seat 1) as in pen, slightly by hand; `draw`: stroke by stroke as it appears. */
export function Mark({ mark, pen = 2.6, draw = false }: { mark: 'x' | 'o'; pen?: number; draw?: boolean }) {
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

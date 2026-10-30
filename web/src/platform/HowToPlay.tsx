import { AnimatePresence, motion, useReducedMotion } from 'motion/react'
import { useId, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

/**
 * "How to play" for any game: a button under the home and lobby box,
 * unfolding the rules (`children`: `RuleSection`s, two to a row on wide
 * screens, numbered) as wide as the page.
 */

const unfold = { closed: { height: 0, opacity: 0 }, open: { height: 'auto', opacity: 1 } }

/** "How to play", unfolding the rules of the game named `game` below it. */
export function HowToPlay({ game, children }: { game: string; children: ReactNode }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const reduced = useReducedMotion()
  const section = useRef<HTMLElement>(null)
  const heading = useId()
  return (
    <>
      <button
        onClick={() => setOpen((open) => !open)}
        aria-expanded={open}
        className="mx-auto mt-4 flex cursor-pointer items-center gap-2 rounded-xl px-3 py-2 text-sm font-medium text-teal-300 transition hover:bg-white/5 hover:text-teal-200"
      >
        <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
          <circle cx="12" cy="12" r="9" />
          <path d="M9.6 9.4a2.5 2.5 0 0 1 4.8.9c0 1.7-2.4 2.1-2.4 3.7" strokeLinecap="round" />
          <circle cx="12" cy="17" r="0.6" fill="currentColor" />
        </svg>
        {t('rules.open')}
        <svg
          viewBox="0 0 16 16"
          className={`size-3.5 transition-transform ${open ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden
        >
          <path d="m3 6 5 5 5-5" />
        </svg>
      </button>
      <AnimatePresence initial={false}>
        {open && (
          <motion.section
            ref={section}
            aria-labelledby={heading}
            variants={unfold}
            initial="closed"
            animate="open"
            exit="closed"
            transition={{ duration: 0.25 }}
            // Once open (the page is only tall enough then), bring the rules into view.
            onAnimationComplete={(done) =>
              done === 'open' &&
              section.current?.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' })
            }
            className="scroll-mt-16 overflow-hidden"
          >
            <div className="mt-2 rounded-2xl border border-white/10 bg-white/5 px-5 pb-5 pt-4 shadow-2xl backdrop-blur-xl sm:px-8 sm:pb-6 sm:pt-6">
              <h2 id={heading} className="text-lg font-semibold">
                {t('rules.title', { game })}
              </h2>
              {/* Wide screens show two sections to a row, numbered so the order is plain. */}
              <div className="grid [counter-reset:rule] lg:grid-cols-2 lg:gap-x-12">{children}</div>
              <button
                onClick={() => setOpen(false)}
                className="mx-auto mt-6 block cursor-pointer rounded-xl px-3 py-2 text-sm font-medium text-teal-300 transition hover:bg-white/5 hover:text-teal-200"
              >
                {t('rules.close')}
              </button>
            </div>
          </motion.section>
        )}
      </AnimatePresence>
    </>
  )
}

/** A numbered section of the rules. */
export function RuleSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="pt-5">
      <h3 className="flex items-center gap-2 font-semibold text-teal-200 [counter-increment:rule] before:grid before:size-6 before:shrink-0 before:place-items-center before:rounded-full before:bg-teal-400/15 before:text-xs before:font-bold before:tabular-nums before:text-teal-200 before:content-[counter(rule)]">
        {title}
      </h3>
      {children}
    </section>
  )
}

export function RuleText({ children }: { children: ReactNode }) {
  return <p className="mt-1.5 text-sm leading-relaxed text-slate-300">{children}</p>
}

/** A list of points, such as the controls. */
export function RuleList({ items }: { items: string[] }) {
  return (
    <ul className="mt-1.5 list-disc space-y-1 ps-5 text-sm leading-relaxed text-slate-300 marker:text-teal-300/60">
      {items.map((item) => (
        <li key={item}>{item}</li>
      ))}
    </ul>
  )
}

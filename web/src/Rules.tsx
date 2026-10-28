import { AnimatePresence, LayoutGroup, motion, useInView, useReducedMotion } from 'motion/react'
import { useEffect, useId, useRef, useState, type ComponentType, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { TileFace } from './TileView.tsx'
import type { Color, RuleSet, Tile } from './wasm/engine.js'

/**
 * How to play each rule set: a few short sections, most with a little scene
 * of real tiles. A rule set without an entry gets no "How to play" button.
 */
const RULES: Partial<Record<RuleSet['kind'], { name: `home.${RuleSet['kind']}`; Content: ComponentType }>> = {
  classic: { name: 'home.classic', Content: ClassicRules },
}

/** "How to play", opening the rules of `kind` in a panel. */
export function HowToPlay({ kind }: { kind: RuleSet['kind'] }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const rules = RULES[kind]
  if (!rules) return null
  return (
    <>
      <button
        onClick={() => setOpen(true)}
        className="mx-auto mt-4 flex cursor-pointer items-center gap-2 rounded-xl px-3 py-2 text-sm font-medium text-teal-300 transition hover:bg-white/5 hover:text-teal-200"
      >
        <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
          <circle cx="12" cy="12" r="9" />
          <path d="M9.6 9.4a2.5 2.5 0 0 1 4.8.9c0 1.7-2.4 2.1-2.4 3.7" strokeLinecap="round" />
          <circle cx="12" cy="17" r="0.6" fill="currentColor" />
        </svg>
        {t('rules.open')}
      </button>
      <AnimatePresence>
        {open && (
          <Dialog title={t('rules.title', { game: t(rules.name) })} onClose={() => setOpen(false)}>
            <rules.Content />
          </Dialog>
        )}
      </AnimatePresence>
    </>
  )
}

function Dialog({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const { t } = useTranslation()
  const heading = useId()
  useEffect(() => {
    const close = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    document.addEventListener('keydown', close)
    return () => document.removeEventListener('keydown', close)
  }, [onClose])
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-4 backdrop-blur-sm"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        role="dialog"
        aria-modal
        aria-labelledby={heading}
        initial={{ y: 12, scale: 0.98 }}
        animate={{ y: 0, scale: 1 }}
        exit={{ y: 12, scale: 0.98 }}
        transition={{ duration: 0.15 }}
        className="flex max-h-[min(90dvh,48rem)] w-full max-w-xl flex-col overflow-hidden rounded-2xl bg-slate-900/95 shadow-2xl ring-1 ring-white/10"
      >
        <div className="flex items-center gap-4 border-b border-white/10 px-5 py-3 sm:px-6">
          <h2 id={heading} className="flex-1 text-lg font-semibold">
            {title}
          </h2>
          <button
            autoFocus
            onClick={onClose}
            aria-label={t('rules.close')}
            className="-me-2 grid size-9 cursor-pointer place-items-center rounded-xl text-xl text-slate-300 transition hover:bg-white/10 hover:text-white"
          >
            ×
          </button>
        </div>
        <div className="overflow-y-auto px-5 pb-6 sm:px-6">{children}</div>
      </motion.div>
    </motion.div>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mt-5">
      <h3 className="font-semibold text-teal-200">{title}</h3>
      {children}
    </section>
  )
}

function Text({ children }: { children: ReactNode }) {
  return <p className="mt-1.5 text-sm leading-relaxed text-slate-300">{children}</p>
}

function ClassicRules() {
  const { t } = useTranslation()
  const buttons = { draw: t('board.draw'), endTurn: t('board.endTurn'), reset: t('board.reset') }
  return (
    <>
      <Section title={t('rules.classic.aimTitle')}>
        <Text>{t('rules.classic.aim')}</Text>
        <figure dir="ltr" className="felt mt-3 flex justify-center gap-0.5 rounded-xl p-3">
          {['k1', 'b5', 'o9', 'r13', 'j'].map((id) => (
            <TileFace key={id} tile={tileOf(id)} size="guide" />
          ))}
        </figure>
      </Section>
      <Section title={t('rules.classic.setsTitle')}>
        <Text>{t('rules.classic.sets')}</Text>
        <figure dir="ltr" className="felt mt-3 flex flex-wrap justify-around gap-3 rounded-xl p-3">
          {(
            [
              [t('rules.classic.run'), ['b3', 'b4', 'b5', 'b6']],
              [t('rules.classic.group'), ['k7', 'r7', 'b7', 'o7']],
            ] as const
          ).map(([name, set]) => (
            <div key={name} className="flex flex-col items-center gap-1.5">
              <div className="flex gap-0.5">
                {set.map((id) => (
                  <TileFace key={id} tile={tileOf(id)} size="guide" />
                ))}
              </div>
              <figcaption dir="auto" className="text-xs font-medium text-emerald-100/70">
                {name}
              </figcaption>
            </div>
          ))}
        </figure>
      </Section>
      <Section title={t('rules.classic.turnTitle')}>
        <Text>{t('rules.classic.turn', buttons)}</Text>
        <Scene
          caption={t('rules.classic.turnCaption')}
          frames={[
            { table: [['b4', 'b5', 'b6'], ['k8', 'o8', 'b8']], rack: ['b3', 'k12', 'r8', 'o2'] },
            { table: [['b3', 'b4', 'b5', 'b6'], ['k8', 'o8', 'b8']], rack: ['k12', 'r8', 'o2'] },
            { table: [['b3', 'b4', 'b5', 'b6'], ['k8', 'o8', 'b8', 'r8']], rack: ['k12', 'o2'] },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.firstTitle')}>
        <Text>{t('rules.classic.first')}</Text>
        <Scene
          caption={t('rules.classic.firstCaption')}
          frames={[
            { table: [], rack: ['k2', 'r10', 'o7', 'r11', 'r12'] },
            { table: [['r10', 'r11', 'r12']], rack: ['k2', 'o7'], note: '10 + 11 + 12 = 33' },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.tableTitle')}>
        <Text>{t('rules.classic.table')}</Text>
        <Scene
          caption={t('rules.classic.splitCaption')}
          frames={[
            { table: [['r4', 'r5', 'r6', 'r7', 'r8']], rack: ['r6.2', 'k1'] },
            { table: [['r4', 'r5', 'r6'], ['r7', 'r8']], rack: ['r6.2', 'k1'] },
            { table: [['r4', 'r5', 'r6'], ['r6.2', 'r7', 'r8']], rack: ['k1'] },
          ]}
        />
        <Scene
          caption={t('rules.classic.borrowCaption')}
          frames={[
            { table: [['b4', 'r4', 'k4', 'o4']], rack: ['b3', 'b5', 'b6'] },
            { table: [['r4', 'k4', 'o4'], ['b4']], rack: ['b3', 'b5', 'b6'] },
            { table: [['r4', 'k4', 'o4'], ['b3', 'b4', 'b5', 'b6']], rack: [] },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.jokerTitle')}>
        <Text>{t('rules.classic.joker')}</Text>
        <Scene
          caption={t('rules.classic.jokerCaption')}
          frames={[
            { table: [['r3', 'b3', 'j']], rack: ['k3', 'o5', 'o6'] },
            { table: [['r3', 'b3', 'k3'], ['j']], rack: ['o5', 'o6'] },
            { table: [['r3', 'b3', 'k3'], ['o5', 'o6', 'j']], rack: [] },
          ]}
        />
      </Section>
      <Section title={t('rules.classic.endTitle')}>
        <Text>{t('rules.classic.end')}</Text>
      </Section>
      <Section title={t('rules.classic.controlsTitle')}>
        <ul className="mt-1.5 list-disc space-y-1 ps-5 text-sm leading-relaxed text-slate-300 marker:text-teal-300/60">
          <li>{t('rules.classic.controlsDrag')}</li>
          <li>{t('rules.classic.controlsSets')}</li>
          <li>{t('rules.classic.controlsPlans')}</li>
          <li>{t('rules.classic.controlsReset', buttons)}</li>
          <li>{t('rules.classic.controlsTime')}</li>
        </ul>
      </Section>
    </>
  )
}

const COLORS: Record<string, Color> = { k: 'black', b: 'blue', o: 'orange', r: 'red' }

/** A scene's tile: `b4` is a blue 4, `j` a joker; `.2` tells twins apart. */
function tileOf(id: string): Tile {
  const face = id.split('.')[0]
  return face === 'j' ? { kind: 'joker' } : { kind: 'number', color: COLORS[face[0]], value: Number(face.slice(1)) }
}

/** The table's sets and the rack at one moment of a scene. */
interface Frame {
  table: string[][]
  rack: string[]
  /** A line shown under the table, such as a sum. */
  note?: string
}

/**
 * A little table and rack whose tiles glide from one frame to the next,
 * over and over while it's in view. Outlines as on the board: amber for a set
 * still too short, green for one that wasn't there at the start. With
 * reduced motion, the last frame.
 */
function Scene({ frames, caption }: { frames: Frame[]; caption?: string }) {
  const reduced = useReducedMotion()
  const ref = useRef<HTMLElement>(null)
  const inView = useInView(ref, { amount: 0.6 })
  const [step, setStep] = useState(0)
  const [round, setRound] = useState(0)
  const group = useId()

  useEffect(() => {
    if (reduced || !inView || frames.length < 2) return
    const last = step === frames.length - 1
    const timer = setTimeout(
      () => {
        if (!last) return setStep(step + 1)
        setStep(0)
        setRound((round) => round + 1)
      },
      step === 0 ? 1400 : last ? 2800 : 1300,
    )
    return () => clearTimeout(timer)
  }, [step, inView, reduced, frames.length])

  const frame = frames[reduced ? frames.length - 1 : step]
  const before = new Set(frames[0].table.map((set) => set.join()))
  const hasRack = frames.some((f) => f.rack.length > 0)
  const hasNote = frames.some((f) => f.note)
  return (
    <figure ref={ref} className="mt-3">
      <div dir="ltr" className="felt overflow-hidden rounded-xl p-3" aria-hidden>
        <LayoutGroup id={group}>
          {/* A new round fades in afresh rather than every tile flying back. */}
          <motion.div key={round} initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.4 }}>
            <div className="flex min-h-[var(--scene-row)] flex-wrap content-start items-start gap-2">
              {frame.table.map((set) => (
                <div
                  key={set.join()}
                  className={`flex gap-0.5 rounded-lg p-1 transition-shadow duration-300 ${
                    set.length < 3
                      ? 'bg-amber-400/15 shadow-[0_0_0_2px_rgb(251_191_36/0.7)]'
                      : before.has(set.join())
                        ? ''
                        : 'bg-emerald-400/15 shadow-[0_0_0_2px_rgb(52_211_153/0.75)]'
                  }`}
                >
                  {set.map((id) => (
                    <Piece key={id} id={id} />
                  ))}
                </div>
              ))}
            </div>
            {hasNote && <p className="h-5 text-center text-sm font-semibold tabular-nums text-emerald-100/80">{frame.note}</p>}
            {hasRack && (
              <div className="tray mt-3 flex min-h-[var(--scene-row)] gap-0.5 rounded-lg p-1">
                {frame.rack.map((id) => (
                  <Piece key={id} id={id} />
                ))}
              </div>
            )}
          </motion.div>
        </LayoutGroup>
      </div>
      {caption && <figcaption className="mt-1.5 text-xs text-slate-400">{caption}</figcaption>}
    </figure>
  )
}

function Piece({ id }: { id: string }) {
  return (
    <motion.div layoutId={id} transition={{ type: 'spring', stiffness: 260, damping: 28 }}>
      <TileFace tile={tileOf(id)} size="guide" />
    </motion.div>
  )
}

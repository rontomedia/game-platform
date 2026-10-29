import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from './ui.tsx'
import type { BotLevel } from '../games/tile-rummy/wasm/engine.js'

/**
 * A door to leave the room by. A player in a game going on is asked first,
 * as it's for good: `standIn` takes over their seat. Watchers, and anyone
 * once the game is over, just go.
 */
export function LeaveMenu({ standIn, ask, leave }: { standIn: BotLevel; ask: boolean; leave: () => void }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const menu = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const close = (e: PointerEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === 'Escape' : !menu.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', close)
    document.addEventListener('keydown', close)
    return () => {
      document.removeEventListener('pointerdown', close)
      document.removeEventListener('keydown', close)
    }
  }, [open])

  return (
    <div ref={menu} className="relative shrink-0">
      <button
        onClick={() => (ask ? setOpen((open) => !open) : leave())}
        aria-label={t('leave.open')}
        title={t('leave.open')}
        aria-expanded={ask ? open : undefined}
        className="grid size-9 cursor-pointer place-items-center rounded-xl text-slate-300 transition hover:bg-white/10 hover:text-white"
      >
        <svg viewBox="0 0 24 24" className="size-5 rtl:-scale-x-100" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
          <path d="M14 4h4a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-4" />
          <path d="M10 16l-4-4 4-4M6 12h10" />
        </svg>
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -6, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -6, scale: 0.97 }}
            transition={{ duration: 0.15 }}
            className="absolute end-0 top-full z-50 mt-2 w-72 rounded-2xl bg-slate-900/95 p-4 shadow-2xl ring-1 ring-white/10 backdrop-blur-xl"
          >
            <h2 className="text-sm font-semibold">{t('leave.title')}</h2>
            <p className="mt-1.5 text-xs text-slate-400">{t(`leave.detail.${standIn}`)}</p>
            <div className="mt-4 flex gap-2">
              <Button look="quiet" className="flex-1 py-1.5 text-sm" onClick={() => setOpen(false)}>
                {t('leave.stay')}
              </Button>
              <Button className="flex-1 py-1.5 text-sm" onClick={leave}>
                {t('leave.leave')}

              </Button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

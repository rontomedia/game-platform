import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useRef, useState } from 'react'

import { Button } from './ui.tsx'
import type { BotLevel } from './wasm/engine.js'

const standInLabel: Record<BotLevel, string> = { easy: 'An Easy', medium: 'A Medium', hard: 'A Hard', extraHard: 'An Extra Hard' }

/**
 * A door to leave the room by. A player in a game going on is asked first,
 * as it's for good: `standIn` takes over their seat. Watchers, and anyone
 * once the game is over, just go.
 */
export function LeaveMenu({ standIn, ask, leave }: { standIn: BotLevel; ask: boolean; leave: () => void }) {
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
        aria-label="Leave the room"
        title="Leave the room"
        aria-expanded={ask ? open : undefined}
        className="grid size-9 cursor-pointer place-items-center rounded-xl text-slate-300 transition hover:bg-white/10 hover:text-white"
      >
        <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
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
            className="absolute right-0 top-full z-50 mt-2 w-72 rounded-2xl bg-slate-900/95 p-4 shadow-2xl ring-1 ring-white/10 backdrop-blur-xl"
          >
            <h2 className="text-sm font-semibold">Leave the game?</h2>
            <p className="mt-1.5 text-xs text-slate-400">
              {standInLabel[standIn]} bot plays your seat for the rest of the game. You can’t come back to it.
            </p>
            <div className="mt-4 flex gap-2">
              <Button look="quiet" className="flex-1 py-1.5 text-sm" onClick={() => setOpen(false)}>
                Stay
              </Button>
              <Button className="flex-1 py-1.5 text-sm" onClick={leave}>
                Leave
              </Button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

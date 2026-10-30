import { AnimatePresence, motion } from 'motion/react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { LanguagePicker } from './LanguagePicker.tsx'
import type { Settings } from './settings.ts'
import { Switch } from './ui.tsx'

/**
 * A gear that opens the settings. `tiles`: with the tile games' own (rack
 * style, watching live turns); `paper`: the paper games' (a light or dark
 * sheet). The language always.
 */
export function SettingsMenu({
  settings,
  onChange,
  tiles = true,
  paper = false,
}: {
  settings: Settings
  onChange: (change: Partial<Settings>) => void
  tiles?: boolean
  paper?: boolean
}) {
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
        onClick={() => setOpen((open) => !open)}
        aria-label={t('settings.open')}
        aria-expanded={open}
        className="grid size-9 cursor-pointer place-items-center rounded-xl text-slate-300 transition hover:bg-white/10 hover:text-white"
      >
        <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
          <path d="M10.3 3.6a1.8 1.8 0 0 1 3.4 0l.3.9a1.8 1.8 0 0 0 2.4 1l.9-.4a1.8 1.8 0 0 1 2.4 2.4l-.4.9a1.8 1.8 0 0 0 1 2.4l.9.3a1.8 1.8 0 0 1 0 3.4l-.9.3a1.8 1.8 0 0 0-1 2.4l.4.9a1.8 1.8 0 0 1-2.4 2.4l-.9-.4a1.8 1.8 0 0 0-2.4 1l-.3.9a1.8 1.8 0 0 1-3.4 0l-.3-.9a1.8 1.8 0 0 0-2.4-1l-.9.4a1.8 1.8 0 0 1-2.4-2.4l.4-.9a1.8 1.8 0 0 0-1-2.4l-.9-.3a1.8 1.8 0 0 1 0-3.4l.9-.3a1.8 1.8 0 0 0 1-2.4l-.4-.9a1.8 1.8 0 0 1 2.4-2.4l.9.4a1.8 1.8 0 0 0 2.4-1z" />
          <circle cx="12" cy="12" r="3" />
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
            <h2 className="text-sm font-semibold">{t('settings.title')}</h2>
            {tiles && (
              <>
                <fieldset className="mt-3">
                  <legend className="text-xs font-medium uppercase tracking-wider text-slate-400">{t('settings.rack')}</legend>
                  <div className="mt-2 grid grid-cols-2 gap-2">
                    {(
                      [
                        ['flat', t('settings.flat'), <FlatIcon key="flat" />],
                        ['classic', t('settings.classic'), <ClassicIcon key="classic" />],
                      ] as const
                    ).map(([style, label, icon]) => (
                      <button
                        key={style}
                        onClick={() => onChange({ rackStyle: style })}
                        aria-pressed={settings.rackStyle === style}
                        className={`flex cursor-pointer flex-col items-center gap-2 rounded-xl p-3 text-xs font-medium ring-1 transition ${
                          settings.rackStyle === style
                            ? 'bg-teal-400/15 text-teal-100 ring-teal-300/60'
                            : 'text-slate-300 ring-white/10 hover:bg-white/5'
                        }`}
                      >
                        {icon}
                        {label}
                      </button>
                    ))}
                  </div>
                </fieldset>
                <label className="mt-4 flex cursor-pointer items-start gap-3">
                  <span className="flex-1">
                    <span className="block text-sm font-medium">{t('settings.watchLive')}</span>
                    <span className="mt-0.5 block text-xs text-slate-400">{t('settings.watchLiveHint')}</span>
                  </span>
                  <Switch checked={settings.watchLive} onChange={(watchLive) => onChange({ watchLive })} />
                </label>
              </>
            )}
            {paper && (
              <fieldset className="mt-3">
                <legend className="text-xs font-medium uppercase tracking-wider text-slate-400">{t('settings.paper')}</legend>
                <div className="mt-2 grid grid-cols-2 gap-2">
                  {(
                    [
                      ['light', t('settings.paperLight'), 'bg-[#fbf8ef]'],
                      ['dark', t('settings.paperDark'), 'bg-[#1b1e25]'],
                    ] as const
                  ).map(([sheet, label, swatch]) => (
                    <button
                      key={sheet}
                      onClick={() => onChange({ paper: sheet })}
                      aria-pressed={settings.paper === sheet}
                      className={`flex cursor-pointer flex-col items-center gap-2 rounded-xl p-3 text-xs font-medium ring-1 transition ${
                        settings.paper === sheet
                          ? 'bg-teal-400/15 text-teal-100 ring-teal-300/60'
                          : 'text-slate-300 ring-white/10 hover:bg-white/5'
                      }`}
                    >
                      <span className={`h-6 w-10 rounded ring-1 ring-white/20 ${swatch}`} aria-hidden />
                      {label}
                    </button>
                  ))}
                </div>
              </fieldset>
            )}
            <div className="mt-4">
              <span className="block text-xs font-medium uppercase tracking-wider text-slate-400">{t('settings.language')}</span>
              <LanguagePicker value={settings.language} onChange={(language) => onChange({ language })} className="mt-2" />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

function FlatIcon() {
  return (
    <svg viewBox="0 0 48 24" className="h-6 w-12" aria-hidden>
      <rect x="1" y="4" width="46" height="16" rx="4" fill="#3a2516" />
      {[0, 1, 2, 3, 4, 5].map((i) => (
        <rect key={i} x={5 + i * 6.6} y="7" width="5" height="10" rx="1" fill="#f3eee3" />
      ))}
    </svg>
  )
}

function ClassicIcon() {
  return (
    <svg viewBox="0 0 48 24" className="h-6 w-12" aria-hidden>
      <rect x="1" y="1" width="46" height="22" rx="3" fill="#6b4226" />
      <rect x="1" y="10" width="46" height="2" fill="#4a2b16" />
      <rect x="1" y="21" width="46" height="2" fill="#4a2b16" />
      {[0, 1, 2, 3, 4].map((i) => (
        <rect key={`a${i}`} x={5 + i * 8} y="2.5" width="5" height="7.5" rx="1" fill="#f3eee3" />
      ))}
      {[0, 1, 2, 3].map((i) => (
        <rect key={`b${i}`} x={5 + i * 8} y="13.5" width="5" height="7.5" rx="1" fill="#f3eee3" />
      ))}
    </svg>
  )
}

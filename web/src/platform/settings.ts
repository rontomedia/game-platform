import { useCallback, useEffect, useState } from 'react'

import { applyLanguage } from './i18n.ts'
import type { LanguageChoice } from './languages.ts'
import { remember, remembered } from './storage.ts'

/** The player's preferences, kept on this device. */
export interface Settings {
  /** A flat tray that wraps, or a two-row rack like a physical one. */
  rackStyle: 'flat' | 'classic'
  /** See the current player's table as they rearrange it (in rooms that share turns live). */
  watchLive: boolean
  /** The game's language, or `auto` for the browser's. */
  language: LanguageChoice
  /** Paper games (grid): a light sheet of paper, or a dark one. */
  paper: 'light' | 'dark'
}

const defaults: Settings = { rackStyle: 'flat', watchLive: true, language: 'auto', paper: 'light' }
const KEY = 'rummy.settings'

/** The settings as saved, before any screen is shown. */
export function savedSettings(): Settings {
  return { ...defaults, ...remembered<Partial<Settings>>(KEY) }
}

export function useSettings(): [Settings, (change: Partial<Settings>) => void] {
  const [settings, setSettings] = useState(savedSettings)
  const change = useCallback(
    (change: Partial<Settings>) =>
      setSettings((settings) => {
        const next = { ...settings, ...change }
        remember(KEY, next)
        return next
      }),
    [],
  )
  useEffect(() => applyLanguage(settings.language), [settings.language])
  return [settings, change]
}

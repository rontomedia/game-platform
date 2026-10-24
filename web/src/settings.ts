import { useCallback, useState } from 'react'

import { remember, remembered } from './storage.ts'

/** The player's preferences, kept on this device. */
export interface Settings {
  /** A flat tray that wraps, or a two-row rack like a physical one. */
  rackStyle: 'flat' | 'classic'
}

const defaults: Settings = { rackStyle: 'flat' }
const KEY = 'rummy.settings'

export function useSettings(): [Settings, (change: Partial<Settings>) => void] {
  const [settings, setSettings] = useState<Settings>(() => ({ ...defaults, ...remembered<Partial<Settings>>(KEY) }))
  const change = useCallback(
    (change: Partial<Settings>) =>
      setSettings((settings) => {
        const next = { ...settings, ...change }
        remember(KEY, next)
        return next
      }),
    [],
  )
  return [settings, change]
}

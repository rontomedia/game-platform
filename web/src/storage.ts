/**
 * Remembered values. Storage can be unavailable (private windows, blocked
 * site data), so reads fall back to nothing and writes may be lost.
 */

export function remembered<T>(key: string, storage: 'local' | 'session' = 'local'): T | null {
  try {
    const text = (storage === 'local' ? localStorage : sessionStorage).getItem(key)
    return text === null ? null : (JSON.parse(text) as T)
  } catch {
    return null
  }
}

export function remember(key: string, value: unknown, storage: 'local' | 'session' = 'local') {
  try {
    const store = storage === 'local' ? localStorage : sessionStorage
    if (value === null) store.removeItem(key)
    else store.setItem(key, JSON.stringify(value))
  } catch {
    // Not remembered, then.
  }
}

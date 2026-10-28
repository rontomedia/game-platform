/** The languages the game is translated into, each named in itself. */
export const LANGUAGES = {
  en: 'English',
  pl: 'Polski',
  es: 'Español',
  fr: 'Français',
  de: 'Deutsch',
  'pt-BR': 'Português (Brasil)',
  tr: 'Türkçe',
  el: 'Ελληνικά',
  sv: 'Svenska',
} as const

export type Language = keyof typeof LANGUAGES

/** A language the player chose, or `auto` to follow the browser. */
export type LanguageChoice = Language | 'auto'

export function isLanguage(code: string): code is Language {
  return Object.hasOwn(LANGUAGES, code)
}

/**
 * The first of the browser's preferred languages (`navigator.languages`,
 * most wanted first) that the game has, matching by language alone when
 * the region differs (`pt-PT` gets `pt-BR`, `en-GB` gets `en`); else
 * English.
 */
export function pickLanguage(preferred: readonly string[]): Language {
  const codes = Object.keys(LANGUAGES) as Language[]
  const base = (code: string) => code.split('-')[0].toLowerCase()
  for (const wanted of preferred) {
    const exact = codes.find((code) => code.toLowerCase() === wanted.toLowerCase())
    const close = exact ?? codes.find((code) => base(code) === base(wanted))
    if (close) return close
  }
  return 'en'
}

/**
 * The language to show: `?lang=xx` in the address (for checking a
 * translation; not remembered), else the player's choice, else the
 * browser's.
 */
export function resolveLanguage(choice: LanguageChoice, preferred: readonly string[], forced: string | null): Language {
  if (forced !== null && isLanguage(forced)) return forced
  return choice === 'auto' ? pickLanguage(preferred) : choice
}

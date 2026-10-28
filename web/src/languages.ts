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

/**
 * A made-up language for checking the layout (`?lang=en-XA`, in
 * development): English with accents, 40% longer, in brackets, so text that
 * would clip or wrap in a long language, or isn't translated at all, shows.
 */
export const PSEUDO = 'en-XA'

const ACCENTED: Record<string, string> = {
  ...{ a: 'á', e: 'é', i: 'í', o: 'ö', u: 'ü', y: 'ý', c: 'ç', n: 'ñ', s: 'š', z: 'ž' },
  ...{ A: 'Å', E: 'É', I: 'Î', O: 'Ø', U: 'Ü' },
}

/** One text in the made-up language; `{{values}}` stay as they are. */
export function pseudoText(text: string): string {
  const parts = text.split(/(\{\{[^}]*\}\})/)
  const accented = parts.map((part, i) => (i % 2 ? part : [...part].map((c) => ACCENTED[c] ?? c).join(''))).join('')
  const letters = parts.filter((_, i) => i % 2 === 0).join('').length
  return `[${accented}${letters > 0 ? ' ' + '~'.repeat(Math.ceil(letters * 0.4)) : ''}]`
}

/** A whole catalogue in the made-up language. */
export function pseudoCatalogue<T extends object>(catalogue: T): T {
  return Object.fromEntries(
    Object.entries(catalogue).map(([key, value]) => [key, typeof value === 'string' ? pseudoText(value) : pseudoCatalogue(value)]),
  ) as T
}

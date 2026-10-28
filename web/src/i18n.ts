import i18n from 'i18next'
import resourcesToBackend from 'i18next-resources-to-backend'
import { initReactI18next } from 'react-i18next'

import { PSEUDO, pseudoCatalogue, resolveLanguage, type LanguageChoice } from './languages.ts'
import en from './locales/en.json'

/** The other catalogues, each a chunk of its own. */
const catalogues = import.meta.glob<object>(['./locales/*.json', '!./locales/en.json'], { import: 'default' })

/** The language to show for the player's choice, here and now. */
export function currentLanguage(choice: LanguageChoice): string {
  const forced = new URLSearchParams(location.search).get('lang')
  if (import.meta.env.DEV && forced === PSEUDO) return PSEUDO
  return resolveLanguage(choice, navigator.languages, forced)
}

function load(language: string): Promise<object> {
  if (language === PSEUDO) return Promise.resolve(pseudoCatalogue(en))
  return catalogues[`./locales/${language}.json`]?.() ?? Promise.resolve({})
}

/**
 * Starts translations in the language for `choice`, loading its catalogue
 * first so the first render is already in it. English is built in (it's
 * the fallback for anything missing); the others are fetched when needed.
 */
export async function initI18n(choice: LanguageChoice) {
  i18n.on('languageChanged', (language) => {
    document.documentElement.lang = language
    // Right to left for Hebrew and Arabic; the board's playing surface keeps left to right.
    document.documentElement.dir = i18n.dir(language)
  })
  await i18n
    .use(initReactI18next)
    .use(resourcesToBackend(load))
    .init({
      lng: currentLanguage(choice),
      fallbackLng: 'en',
      load: 'currentOnly',
      resources: { en: { translation: en } },
      partialBundledLanguages: true,
      // React escapes what it renders.
      interpolation: { escapeValue: false },
    })
}

/** Switches to the language for `choice`, once its catalogue is loaded. */
export function applyLanguage(choice: LanguageChoice) {
  const language = currentLanguage(choice)
  if (language !== i18n.language) void i18n.changeLanguage(language)
}

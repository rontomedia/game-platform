import { useTranslation } from 'react-i18next'

import { LANGUAGES, pickLanguage, type LanguageChoice } from './languages.ts'

/**
 * The game's language, behind a globe: each named in itself, so anyone can
 * find their own whatever the page is in, plus following the browser.
 */
export function LanguagePicker({
  value,
  onChange,
  className = '',
}: {
  value: LanguageChoice
  onChange: (language: LanguageChoice) => void
  className?: string
}) {
  const { t } = useTranslation()
  const browser = LANGUAGES[pickLanguage(navigator.languages)]
  return (
    <label className={`relative flex items-center gap-2 rounded-xl bg-black/30 pl-2.5 ring-1 ring-white/10 focus-within:ring-teal-300/60 ${className}`}>
      <svg viewBox="0 0 24 24" className="size-4 shrink-0 text-slate-400" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden>
        <circle cx="12" cy="12" r="9" />
        <path d="M3 12h18M12 3c2.5 2.7 3.8 5.7 3.8 9s-1.3 6.3-3.8 9c-2.5-2.7-3.8-5.7-3.8-9S9.5 5.7 12 3z" />
      </svg>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as LanguageChoice)}
        aria-label={t('settings.language')}
        className="min-w-0 flex-1 cursor-pointer appearance-none truncate bg-transparent py-1.5 pr-7 text-sm font-medium text-slate-200 [color-scheme:dark] focus:outline-none [&_option]:bg-slate-900 [&_option]:text-slate-100"
      >
        <option value="auto">{t('settings.languageAuto', { language: browser })}</option>
        {Object.entries(LANGUAGES).map(([code, name]) => (
          <option key={code} value={code} lang={code}>
            {name}
          </option>
        ))}
      </select>
      <svg
        viewBox="0 0 16 16"
        className="pointer-events-none absolute right-2.5 size-3 text-slate-400"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        aria-hidden
      >
        <path d="m4 6 4 4 4-4" />
      </svg>
    </label>
  )
}

import { useEffect, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { LanguagePicker } from './platform/LanguagePicker.tsx'
import { useSettings } from './platform/settings.ts'
import type { PagePath } from './pages.ts'

/** The site's front page: every game family, and a link to each of its rule sets' pages. */
export function GamesPage() {
  const { t } = useTranslation()
  const [settings, changeSettings] = useSettings()
  useEffect(() => {
    document.title = t('games.title')
  }, [t])
  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-6 px-4 py-6 sm:px-6 lg:py-10">
      <header className="flex items-center gap-3 sm:gap-4">
        <h1 className="me-auto shrink-0 text-3xl font-bold tracking-tight">{t('games.title')}</h1>
        <LanguagePicker
          value={settings.language}
          onChange={(language) => changeSettings({ language })}
          className="min-w-0 max-w-52 sm:max-w-60"
        />
      </header>
      <p className="-mt-2 text-slate-400">{t('games.intro')}</p>
      <div className="grid gap-5 sm:grid-cols-2">
        <Family
          name={t('games.tileRummy')}
          detail={t('games.tileRummyDetail')}
          art={<TilesArt />}
          pages={[
            ['/rummy', t('home.classic')],
            ['/okey-101', t('home.okey101')],
            ['/okey', t('home.okey')],
          ]}
        />
        <Family
          name={t('games.grid')}
          detail={t('games.gridDetail')}
          art={<GridArt />}
          pages={[
            ['/tic-tac-toe', t('grid.ticTacToe')],
            ['/gomoku', t('grid.gomoku')],
            ['/notakto', t('grid.notakto')],
            ['/sos', t('grid.sos')],
          ]}
        />
      </div>
    </div>
  )
}

/** A game family: a picture, what it is, and its rule sets' pages. */
function Family({
  name,
  detail,
  art,
  pages,
}: {
  name: string
  detail: string
  art: ReactNode
  pages: [PagePath, string][]
}) {
  return (
    <section className="overflow-hidden rounded-2xl border border-white/10 bg-white/5 shadow-2xl backdrop-blur-xl">
      <a href={pages[0][0]} className="block" tabIndex={-1} aria-hidden>
        {art}
      </a>
      <div className="p-5 sm:p-6">
        <h2 className="text-xl font-semibold">{name}</h2>
        <p className="mt-1 text-sm text-slate-400">{detail}</p>
        <div className="mt-4 flex flex-wrap gap-2">
          {pages.map(([path, title]) => (
            <a
              key={path}
              href={path}
              className="rounded-xl bg-teal-500/15 px-3.5 py-2 text-sm font-semibold text-teal-200 transition hover:bg-teal-500 hover:text-slate-950"
            >
              {title}
            </a>
          ))}
        </div>
      </div>
    </section>
  )
}

/** A few tiles on the felt: tile rummy's picture. */
function TilesArt() {
  const tiles: [number, string][] = [
    [7, 'text-sky-600'],
    [7, 'text-rose-600'],
    [7, 'text-slate-900'],
    [10, 'text-orange-500'],
    [11, 'text-orange-500'],
    [12, 'text-orange-500'],
  ]
  return (
    <div className="flex h-36 items-center justify-center gap-1.5 bg-[radial-gradient(circle_at_50%_40%,#1f5f4f,#0d2f28)]" dir="ltr">
      {tiles.map(([value, color], i) => (
        <span
          key={i}
          className={`grid h-16 w-11 place-items-center rounded-lg bg-linear-to-b from-stone-50 to-stone-200 text-2xl font-bold shadow-lg ${color} ${i === 3 ? 'ms-4' : ''}`}
        >
          {value}
        </span>
      ))}
    </div>
  )
}

/** A game of Tic-Tac-Toe won across the middle: grid games' picture. */
function GridArt() {
  const marks = ['o', '', 'x', 'x', 'x', 'x', 'o', '', 'o']
  return (
    <div className="grid h-36 place-items-center bg-[radial-gradient(circle_at_50%_40%,#1f5f4f,#0d2f28)]" dir="ltr">
      <div className="grid size-28 grid-cols-3 gap-[2px] rounded-lg bg-white/15 p-[2px]">
        {marks.map((mark, i) => (
          <span key={i} className={`grid place-items-center rounded-md ${i >= 3 && i < 6 ? 'bg-[#5c5530]' : 'bg-[#163f36]'}`}>
            {mark === 'x' && (
              <svg viewBox="0 0 24 24" className="size-6 text-teal-300" fill="none" stroke="currentColor" strokeWidth="3.2" strokeLinecap="round">
                <path d="M5 5l14 14M19 5L5 19" />
              </svg>
            )}
            {mark === 'o' && (
              <svg viewBox="0 0 24 24" className="size-6 text-amber-300" fill="none" stroke="currentColor" strokeWidth="3.2">
                <circle cx="12" cy="12" r="7.5" />
              </svg>
            )}
          </span>
        ))}
      </div>
    </div>
  )
}

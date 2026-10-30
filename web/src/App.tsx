import { lazy, Suspense } from 'react'
import { useTranslation } from 'react-i18next'

import { GamesPage } from './GamesPage.tsx'
import { pageAt } from './pages.ts'

// Each family's code and engine (WASM) load only on its pages.
const RummyApp = lazy(() => import('./games/tile-rummy/RummyApp.tsx'))

/**
 * The site: the games page, or a rule set's page (`pages.ts`). `?demo` on
 * any page shows tile rummy's demo board, with no server.
 */
export default function App() {
  const page = pageAt(location.pathname)
  const demo = new URLSearchParams(location.search).has('demo')
  if (!page && !demo) return <GamesPage />
  return (
    <Suspense fallback={<Loading />}>
      <RummyApp kind={page?.kind} />
    </Suspense>
  )
}

function Loading() {
  const { t } = useTranslation()
  return <p className="grid min-h-dvh place-items-center text-sm text-slate-400">{t('games.loading')}</p>
}

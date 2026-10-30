/**
 * The site's pages: the games page at `/`, and a page per named rule set
 * (for search and for sharing), each in its game family's app.
 */
export const PAGES = {
  '/rummy': { family: 'tile-rummy', kind: 'classic' },
  '/okey-101': { family: 'tile-rummy', kind: 'okey101' },
  '/okey': { family: 'tile-rummy', kind: 'okey' },
  '/tic-tac-toe': { family: 'grid', kind: 'ticTacToe' },
  '/gomoku': { family: 'grid', kind: 'gomoku' },
  '/notakto': { family: 'grid', kind: 'notakto' },
  '/sos': { family: 'grid', kind: 'sos' },
} as const

export type PagePath = keyof typeof PAGES

/** The page at `path`, if there is one (a trailing slash allowed). */
export function pageAt(path: string) {
  const trimmed = path.replace(/\/+$/, '')
  return Object.hasOwn(PAGES, trimmed) ? PAGES[trimmed as PagePath] : undefined
}

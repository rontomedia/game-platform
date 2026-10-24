import type { Color, Tile } from './wasm/engine.js'

const numeralColor: Record<Color, string> = {
  black: 'text-tile-black',
  blue: 'text-tile-blue',
  orange: 'text-tile-orange',
  red: 'text-tile-red',
}

const sizes = {
  table: 'h-[3.25rem] w-9 text-[1.25rem] rounded-md sm:h-16 sm:w-11 sm:text-2xl short:h-11 short:w-8 short:text-lg',
  rack: 'h-16 w-11 text-2xl rounded-lg sm:h-[4.75rem] sm:w-[3.25rem] sm:text-[1.9rem] short:h-12 short:w-9 short:text-xl',
  /** The two-row rack: a little narrower on phones, so eight fit in a row. */
  ledge: 'h-14 w-10 text-[1.4rem] rounded-md sm:h-[4.75rem] sm:w-[3.25rem] sm:rounded-lg sm:text-[1.9rem] short:h-12 short:w-9 short:text-xl',
}

export type TileSize = keyof typeof sizes

/** A tile's face. Moving it around is up to whoever holds it (see Board). */
export function TileFace({ tile, size, lifted = false }: { tile: Tile; size: TileSize; lifted?: boolean }) {
  return (
    <div
      className={`tile relative flex shrink-0 select-none items-center justify-center font-extrabold tabular-nums tracking-tight ${sizes[size]} ${
        lifted ? 'tile-lifted' : ''
      }`}
    >
      {tile.kind === 'joker' ? <JokerFace /> : <span className={numeralColor[tile.color]}>{tile.value}</span>}
    </div>
  )
}

/** A grinning face in the tile colors. */
function JokerFace() {
  return (
    <svg viewBox="0 0 24 24" className="h-[62%] w-[82%]" aria-hidden>
      <defs>
        <linearGradient id="joker-ring" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="var(--color-tile-red)" />
          <stop offset="0.5" stopColor="var(--color-tile-orange)" />
          <stop offset="1" stopColor="var(--color-tile-blue)" />
        </linearGradient>
      </defs>
      <circle cx="12" cy="12" r="10" fill="none" stroke="url(#joker-ring)" strokeWidth="2.2" />
      <circle cx="8.6" cy="9.6" r="1.5" fill="var(--color-tile-black)" />
      <circle cx="15.4" cy="9.6" r="1.5" fill="var(--color-tile-black)" />
      <path
        d="M7 14c1.3 2.4 3 3.4 5 3.4s3.7-1 5-3.4"
        fill="none"
        stroke="var(--color-tile-red)"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  )
}

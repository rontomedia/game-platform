import { motion } from 'motion/react'

import type { Color, Tile } from './wasm/engine.js'

const numeralColor: Record<Color, string> = {
  black: 'text-tile-black',
  blue: 'text-tile-blue',
  orange: 'text-tile-orange',
  red: 'text-tile-red',
}

function tileName(tile: Tile): string {
  return tile.kind === 'joker' ? 'Joker' : `${tile.color} ${tile.value}`
}

/**
 * A tile, clickable when given `onClick`. `layoutId` lets it glide between
 * places.
 */
export function TileView({ tile, layoutId, onClick }: { tile: Tile; layoutId: string; onClick?: () => void }) {
  return (
    <motion.button
      layoutId={layoutId}
      onClick={onClick}
      disabled={!onClick}
      aria-label={tileName(tile)}
      whileHover={onClick && { y: -4 }}
      whileTap={onClick && { scale: 0.94 }}
      transition={{ type: 'spring', stiffness: 500, damping: 35 }}
      className="flex h-16 w-11 cursor-pointer select-none disabled:cursor-default items-center justify-center rounded-lg bg-linear-to-b from-stone-50 to-stone-200 text-2xl font-extrabold tabular-nums shadow-[inset_0_1px_0_rgb(255_255_255/0.9),0_4px_12px_rgb(0_0_0/0.4)] ring-1 ring-black/10 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-400 sm:h-20 sm:w-14 sm:text-3xl"
    >
      {tile.kind === 'joker' ? (
        <span className="bg-linear-to-br from-red-500 via-amber-400 to-blue-600 bg-clip-text text-transparent">★</span>
      ) : (
        <span className={numeralColor[tile.color]}>{tile.value}</span>
      )}
    </motion.button>
  )
}

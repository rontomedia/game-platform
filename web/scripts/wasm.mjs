// Builds each game's engine as WASM (wasm-pack) into web/src/games/<game>/wasm/:
// `node scripts/wasm.mjs` for release, `--dev` for a quicker debug build.
import { execFileSync } from 'node:child_process'

/** Each game's -wasm crate (in games/) and where its build goes (in src/games/). */
const GAMES = [
  ['tile-rummy-wasm', 'tile-rummy'],
  ['grid-wasm', 'grid'],
]

const dev = process.argv.includes('--dev')
for (const [crate, game] of GAMES) {
  const args = ['build', `../games/${crate}`, '--target', 'web', '--out-dir', `../../web/src/games/${game}/wasm`]
  args.push('--out-name', 'engine', '--no-pack', ...(dev ? ['--dev'] : []))
  execFileSync('wasm-pack', args, { stdio: 'inherit' })
}

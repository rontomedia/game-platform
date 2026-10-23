# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

An online tile-rummy game in the style of Rummikub. The priority is a web
game with a polished, modern look; multiplayer against people and bots.
Design notes, status and the reasons behind the stack live outside the repo,
in `D:\github\second-brain\projects\rummy-game\` (`README.md` for open items,
`decisions.md` for why). Don't reopen those decisions unless something
contradicts them. Don't call the game "Rummikub" anywhere user-facing (it's
a trademark).

## Commands

From the repo root:

```sh
cargo run -p server                 # match server, ws://0.0.0.0:3000/ws
cargo test --workspace              # one test: cargo test -p engine runs
cargo clippy --workspace --all-targets
```

From `web/`:

```sh
npm run dev       # builds the engine WASM (debug), then Vite on http://localhost:5173
npm run build     # release WASM + type check + production build into web/dist
npm run lint      # oxlint
npm run wasm      # only rebuild the engine WASM (needs wasm-pack)
```

Run the server and `npm run dev` together. After changing Rust in
`crates/engine*`, rerun `npm run wasm:dev` (Vite then reloads); `npm run dev`
only builds it on start.

## Architecture

- **`crates/engine`**: the game rules, pure (no I/O, clock or outside
  randomness; a deal is shuffled from a seed). The single source of truth
  for rules, used by the server, bots and the browser. `check_turn` validates
  a turn by tile counts (table after = table before + tiles from the rack),
  so tiles have no IDs; `Game` runs a whole game (play/draw, end, scores) and
  `view(player)` is what that player may see. Its `wasm` feature derives
  `tsify::Tsify` on the types that cross into JS; serde attributes define the
  TS shape (e.g. `Tile` is `{ kind: "number", color, value } | { kind:
  "joker" }`), and JSON field names are camelCase.
- **`crates/protocol`**: `ClientMessage` / `ServerMessage`, one JSON text
  frame each, tagged by `type`. Never serialize `null`: tsify types say
  `field?: T` for an `Option`, so use `skip_serializing_if = "Option::is_none"`
  with `#[tsify(optional)]`. Bump `PROTOCOL_VERSION` on incompatible changes.
- **`crates/engine-wasm`**: the browser API (`wasm-bindgen`): engine
  functions plus `protocolVersion`. wasm-pack writes JS glue and `.d.ts` into
  `web/src/wasm/` (gitignored), so TypeScript types for the engine *and* the
  protocol come from Rust. A crate's tsify types only reach the `.d.ts` if the
  crate is actually used by engine-wasm. Engine types cross as
  `tsify::Ts<T>` and are converted inside the function; don't use tsify's
  `into_wasm_abi`/`from_wasm_abi` (deprecated, they leak on bad input).
- **`crates/server`**: tokio + axum match server; `/ws` WebSocket route, one
  task per connection. Sends `welcome`, then answers each `ClientMessage`
  (no matches yet, so moves get an `error`). The server is the authority;
  the client's engine checks are for instant feedback. Tests start the
  server on a free port and talk to it with `tokio-tungstenite`.
- **`web/`**: Vite + React 19 + TypeScript client. Tailwind v4 (theme tokens
  in `src/index.css`, `@theme`), Motion for animation (`layoutId` makes a
  tile glide between rack and set), Inter via fontsource. `src/main.tsx`
  awaits the WASM init before rendering. The board is HTML elements, not a
  canvas. Planned: dnd-kit for dragging, shadcn/ui for menus,
  `@heroiclabs/nakama-js` for accounts/matchmaking.

The client reaches the server at `/ws` on its own origin: Vite proxies it to
port 3000 in development (`vite.config.ts`), and `VITE_SERVER_URL`
overrides it. Vite listens on all interfaces, so a phone on the LAN can open
`http://<pc-ip>:5173`.

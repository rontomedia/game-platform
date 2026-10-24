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
npm test          # Vitest (web/src/*.test.ts); one test: npx vitest run -t "name"
npm run wasm      # only rebuild the engine WASM (needs wasm-pack)
```

`http://localhost:5173/?demo` shows the board with a made-up 4-player game
and no server (`src/demo.ts`): the place to work on the board's look.

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
- **`crates/server`**: tokio + axum match server, `/ws` WebSocket route.
  - `rooms.rs`: open rooms by 4-letter code (no I or O).
  - `room.rs`: each room is its own task (an actor) owning its players, the
    `Game` and the turn timer; connections send it `Command`s over a channel,
    so game state has no locks. Players are identified by a stable
    `PlayerId` (seats shift when those who left are dropped at the start)
    and keep their seat across lost connections with a token (`rejoin`).
    `Timing`: turn 60 s (runs out: the player draws), 5 s for a player who
    is away (their full time is given back if they return during that turn),
    room closes after 120 s with nobody connected.
  - `connection.rs`: one task per WebSocket, `select!`ing over the socket
    and the room's messages for it.
  - The server is the authority; the client's engine checks are for
    instant feedback. Tests (`tests.rs`) play matches through real
    WebSockets against a server on a free port, with short `Timing`s.
- **`web/`**: Vite + React 19 + TypeScript client. Tailwind v4 (theme tokens
  and the tile/felt/rack looks in `src/index.css`; a `short:` variant for
  screens under 500 px high, i.e. phones held sideways), Motion for UI
  animation, Inter via fontsource. `src/main.tsx` awaits the WASM init
  before rendering. The board is HTML elements, not a canvas.
  - `useGame.ts` owns the connection: it keeps the seat token in
    `sessionStorage` (per tab, so two tabs are two players), rejoins after a
    reload and retries a lost connection. Screens: home (create/join),
    lobby, `Board.tsx`.
  - `Board.tsx`: everything moves by dragging (dnd-kit core + sortable):
    rack tiles any time; tiles onto and around the table on your turn;
    whole sets any time (by their grip). Tiles live in "holders" (`rack-0`,
    `rack-1`, `set-<key>`); a tile crossing holders moves in `onDragOver`.
    Don't put Motion `layout`/`layoutId` on draggable tiles: both libraries
    drive `transform`. `checkTurn` gives instant feedback; End turn sends
    the table in this player's order.
  - `arrange.ts`: the player's own order of rack and table sets, kept across
    server states (the board remounts on every state, `key={game.version}`)
    by matching tiles and sets by content; saved per tab in sessionStorage.
    Pure, with Vitest tests.
  - `settings.ts` / `SettingsMenu.tsx`: device preferences (rack style: flat
    tray or two-row rack) behind the gear in the top bar.
  - Planned: shadcn/ui for menus, `@heroiclabs/nakama-js` for
    accounts/matchmaking.

The client reaches the server at `/ws` on its own origin: Vite proxies it to
port 3000 in development (`vite.config.ts`), and `VITE_SERVER_URL`
overrides it. Vite listens on all interfaces, so a phone on the LAN can open
`http://<pc-ip>:5173`.

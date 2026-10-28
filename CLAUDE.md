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
cargo run -p server -- --open-racks # the same, every rack shown to everyone (judging bots)
# also: --lookahead OPTIONS,GAMES,ROUNDS (Extra Hard, default 1,24,2),
# --bot-time MS (longest a bot thinks, default 1000), --bot-threads N
# (bots thinking at once, default cores - 1); see `config` in main.rs
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
`?demo=racks` adds the open racks panel; `?demo=watcher` is a bots-only
game seen by its watching host (Pause works). `?lang=de` (any page) shows
a language without changing the setting; `?lang=en-XA` (development only)
is English accented and 40% longer, to find text that clips or isn't
translated.

`cargo test -p engine --release -- --ignored --nocapture strength` plays
the bot levels against each other and prints wins and time per move.

Run the server and `npm run dev` together. After changing Rust in
`crates/engine*`, rerun `npm run wasm:dev` (Vite then reloads); `npm run dev`
only builds it on start.

## Architecture

- **`crates/engine`**: the game rules, pure (no I/O, clock or outside
  randomness; a deal is shuffled from a seed). The single source of truth
  for rules, used by the server, bots and the browser. `check_turn` validates
  a turn by tile counts (table after = table before + tiles from the rack),
  so tiles have no IDs; `Game` runs a whole game (play/draw, end, scores) and
  `view(player)` is what that player may see. `bot_move(view, level)` is a
  computer player's move: `solve.rs` finds the arrangement of table plus
  rack tiles that plays the most (dynamic programming over the numbers
  1-13); Easy adds to the table and on about half its turns
  (`EASY_SPOTS`, chosen by a hash of the position) may change one of its
  sets, Medium may change up to two (`MEDIUM_REACH`), Hard rearranges it
  all but keeps the sets it needn't change. Extra Hard (`bot/extra.rs`)
  plays like Hard but weighs its options (Hard's play, holding a tile back,
  drawing): the rack each leaves, and when another player is down to 1-2
  tiles, how likely each is to let them go out (racks they could hold,
  sampled from the unseen tiles); then it plays its best few options on
  in imagined games (hidden tiles dealt at random, everyone as a quick
  Hard for two rounds, `LOOKAHEAD`) and takes the one that does best.
  It stops when its time is up: `bot_move_with` takes a `spent` function
  (the share of its time used; the engine has no clock, the server
  measures), and it decides on what it has.
  `extra_strength`, `extra_weights` and `extra_lookahead` (ignored tests,
  run in `--release`; `EXTRA_GAMES` for fewer games) measure it against
  Hard on all cores. `salvage` makes a valid turn of what a player left
  when their time ran out (their valid sets kept where it plays as many).
  Its `wasm` feature derives
  `tsify::Tsify` on the types that cross into JS; serde attributes define the
  TS shape (e.g. `Tile` is `{ kind: "number", color, value } | { kind:
  "joker" }`), and JSON field names are camelCase.
- **`crates/protocol`**: `ClientMessage` / `ServerMessage`, one JSON text
  frame each, tagged by `type`. Never serialize `null`: tsify types say
  `field?: T` for an `Option`, so use `skip_serializing_if = "Option::is_none"`
  with `#[tsify(optional)]`. Bump `PROTOCOL_VERSION` on incompatible changes.
  Messages say what happened, never a sentence for players: `Notice`,
  `Refusal` and `TurnError` are enums the client words in the player's
  language (`web/src/messages.ts`); `Error { message }` is only for
  messages the server couldn't read. A new thing to tell players is a new
  variant, not text.
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
    `RoomOptions` (chosen on the home screen): turn length (30 s to 3 min,
    default 1 min; `checked()` keeps it to `TURN_SECONDS`), a time bank
    (2 min a player, used once a turn's own time runs out; not while away;
    `settle_bank` charges it before each move; time left when a player ends
    their turn is added, up to 3 min, less any extra time they were given
    for tiles they took back), extra time for tiles
    (`clock.rs`: 15 s a tile and 20 s a set holding one, once a draft is a
    valid turn, only the amount above the best so far this turn, sent as
    `Clock`; time given for tiles then taken back counts against the
    player: a `Notice` at 1 min, none for the rest of the game at 2), live
    turns, and the stand-in bot (`stand_in`, default Medium, never Extra
    Hard). Leaving for good (`Leave`, the board's door button; not a lost
    connection, `Command::Disconnect`): in the lobby, or watching, the
    member is gone; during a game the stand-in takes over the seat, under
    the player's name, and a turn of theirs starts afresh for it.
    `Timing`: a minute's turn is 60 s, scaled by the room's
    length (when it runs out, the most of the table they left, their last
    `draft`, that makes a valid turn is played, `engine::salvage`; the rest
    goes back to their rack, and a `Notice` says so), 5 s for a player who
    is away (their full time is given back if they return during that turn),
    room closes after 120 s with nobody connected. Bots (`AddBot`, host
    only, before the start) are players without a connection: on their
    turn the room waits, works the move out on a blocking thread
    (`Command::BotMove`, dropped if the turn has passed; at most
    `Config::bot_threads` at once across rooms, a shared semaphore, each
    for `Timing::bot_time`), shows it as a
    draft in rooms with live turns, then plays it. With `--open-racks`
    every `State` carries all racks (`racks`).
  - `connection.rs`: one task per WebSocket, `select!`ing over the socket
    and the room's messages for it.
  - The server is the authority; the client's engine checks are for
    instant feedback. Tests (`tests.rs`) play matches through real
    WebSockets against a server on a free port, with short `Timing`s.
  - Watchers: members without a seat (`Room.watchers`, the same `Player`
    struct and `PlayerId` counter). `JoinRoom { watch }` at any time, or
    `Watch` in the lobby. They get `Game::watcher_view()` (no rack, `you`
    absent; `RoomInfo.watching` instead of `you`), drafts and clocks, and
    every rack when all seats are bots. The host is the connected member
    with the lowest id, seated or watching (`Seat.host`). In a bots-only
    game the host can `Pause` (the clock and bot steps stand still and are
    shifted on resuming) and `Step` (resume, then pause again as the next
    turn starts).
- **`web/`**: Vite + React 19 + TypeScript client. Tailwind v4 (theme tokens
  and the tile/felt/rack looks in `src/index.css`; a `short:` variant for
  screens under 500 px high, i.e. phones held sideways), Motion for UI
  animation, Inter via fontsource. `src/main.tsx` awaits the WASM init
  before rendering. The board is HTML elements, not a canvas.
  - `useGame.ts` owns the connection: it keeps the seat token in
    `sessionStorage` (per tab, so two tabs are two players), rejoins after a
    reload and retries a lost connection. Screens: home (create/join),
    lobby, `Board.tsx`.
  - Room options on the home screen: only turn time is always shown; the
    rest are under "Advanced settings" (`Advanced` in `App.tsx`, folded,
    showing how many differ from the defaults). A new room option goes
    under Advanced unless most players would want to choose it.
  - `Board.tsx`: everything moves by dragging (dnd-kit core + sortable):
    rack tiles any time; tiles onto and around the table on your turn (a
    tile dropped on empty table starts a set there); whole sets any time,
    by their grip, to any cell of the table's grid. Tiles live in
    "holders" (`rack-0`, `rack-1`, `set-<key>`); a tile crossing holders
    moves in `onDragOver`. Drop targets must be in components rendered
    *inside* `DndContext` (hence `TableGrid`), not in `Board` itself. Don't
    put Motion `layout`/`layoutId` on draggable tiles: both libraries drive
    `transform`. `checkTurn` gives instant feedback. Set outlines, in order
    of precedence: amber invalid, green changed or new this turn (moving a
    set doesn't count), blue changed by another player on their last turn
    (`GameState.lastPlay` from `useGame.ts`; flashes when it's news, then
    stays quiet until the table changes).
  - Live turns: the current player's board sends its table after each
    drop (`draft`, in every room: it's what's played if time runs out); the
    server checks it uses only tiles from the table and that player's rack
    (`engine::within_reach`) and keeps it until the next state. In rooms
    created with `liveTurns` (on by default) it also passes it on, and to
    anyone who reconnects. Watchers see it via `withTable` in
    their own arrangement (unchanged sets keep their keys, so nothing they
    hold moves), changed sets pulsing blue. Tiles new to the table glide
    in like dragged ones (Web Animations on the tile's face; the wrapper's
    position is what's recorded), from where they were or from the
    player's avatar (`data-seat`) if they came from the rack (a bot's more
    slowly, `pace`; the server shows its play for `Timing::bot_show`, long
    enough for them); tiles taken back fly off to the avatar. Each player can switch watching
    off in the settings (`watchLive`); there's no per-player opt-out of
    being watched, as that's the room's rule. `?demo=watch` shows it.
    Someone watching (`view.you` undefined) gets `WatchBar` in place of
    the tray, with Pause and Step for the host of a bots-only game.
  - Flights (`flights.ts`, Web Animations; skipped for reduced motion):
    besides live turns, a play not watched live glides in from the
    player's avatar as the board shows it (`lastPlay.tiles`); from the
    pool (`data-pool`) the deal (`GameState.dealt`) and the tile this
    player drew (`drawn`, marked on the rack until their next turn ends).
    Rack flights use a copy of the tile above everything, as the rack
    scrolls. Also: a "Your turn" banner and tray glow (`turnSince`), and
    a warning at 10 s left (turn plus bank). The turn's buttons are
    always laid out, hidden off turn, so the tray keeps its size.
  - Plans: a strip at the top of the tray (`Plans`, holders `plan-<key>`)
    where the player builds sets from their rack any time; on their turn a
    plan goes onto the table whole (its ↑ button or double-click). Kept in
    the arrangement like the rack order (`keepPlans`).
  - Double-clicking a rack tile: on your turn it goes onto the table, else
    into a plan. It joins the set last changed there (by drag or
    double-click) if it fits (`joinAt`: at an end, or into a run of its
    colour at its place by number, gaps left to fill), else starts a new
    one, after the last row. Double-clicking a planned tile off your turn sends it back to
    the rack.
  - `grid.ts`: the table as a grid of cells (one table tile wide, one row
    high; sizes as CSS variables on `.table-grid` in `index.css`, in step
    with the `table` tile size). A set takes a cell for its grip plus one
    per tile. `resolve` shows each set where the player put it if it fits
    the current width and is free, else in the next free place, so
    resizing never loses a placement. Pure, with Vitest tests.
  - `arrange.ts`: the player's own rack order and set positions, kept
    across server states (the board remounts on every state,
    `key={game.version}`) by matching tiles and sets by content; saved per
    tab in sessionStorage. Pure, with Vitest tests.
  - Open racks: when states carry `racks`, `useGame.ts` logs each turn
    (tiles played, tile drawn) and the board shows the others' racks and
    the log above the table (`OpenRacks`). Each of those racks can be
    sorted or reordered by dragging (its own nested `DndContext`), the
    order kept per tab like the player's own.
  - `settings.ts` / `SettingsMenu.tsx`: device preferences (rack style: flat
    tray or two-row rack; watching live turns; language) behind the gear in
    the top bar.
  - Languages: react-i18next, every visible text through `t('key')` (no
    text in components). `src/locales/en.json` is the source and types the
    keys and their `{{values}}` (`i18next.d.ts`), so a wrong key fails the
    type check; the others (listed in `languages.ts`) load as chunks of
    their own, before the first render (`i18n.ts`).
    The language is the setting, else the first of `navigator.languages`
    the game has (by language alone when the region differs), else
    English; `<html lang>` follows it. `LanguagePicker` (home header and
    settings) names each in itself, no flags. Counts use plural keys
    (`_one`, `_few`, ...: each language's own forms from
    `Intl.PluralRules`, Polish has four), lists `Intl.ListFormat`.
    `i18n.test.ts` checks every catalogue against English: keys, plural
    forms, values; it reads them from disk, as Vitest's module cache
    keeps an `import.meta.glob`'s old file list. Adding text: the key in
    `en.json` and every other catalogue (the test says what's missing).
    Adding a language: its code and own name in `LANGUAGES`, and its
    catalogue; a script Inter lacks also needs its Noto font (fontsource,
    imported in `main.tsx`, named in `--font-sans`; `unicode-range` means
    only pages showing it download it). Right-to-left languages (Hebrew,
    Arabic) set `<html dir>`: use logical classes (`ms-`, `pe-`, `start-`,
    `end-`, `text-start`), and the playing surface (table, rack, plans,
    open racks' tiles) stays `dir="ltr"` so runs read 1 2 3.
  - Planned: shadcn/ui for menus, `@heroiclabs/nakama-js` for
    accounts/matchmaking.

The client reaches the server at `/ws` on its own origin: Vite proxies it to
port 3000 in development (`vite.config.ts`), and `VITE_SERVER_URL`
overrides it. Vite listens on all interfaces, so a phone on the LAN can open
`http://<pc-ip>:5173`.

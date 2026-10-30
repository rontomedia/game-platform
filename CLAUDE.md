# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

An online games platform (working name `game-platform`), growing out of a
tile-rummy game in the style of Rummikub, its first game. The priority is
web games with a polished, modern look; multiplayer against people and
bots. Design notes, status and the reasons behind the stack live outside
the repo, in `D:\github\second-brain\projects\game-platform\` (the
platform) and `projects\rummy-game\` (the tile game) (`README.md` for open
items, `decisions.md` for why). Don't reopen those decisions unless
something contradicts them. Don't call the game "Rummikub" anywhere
user-facing (it's a trademark).

Layout: `crates/` holds what all games share (`platform`, the generic
room; `protocol`; `server`, the binary with each game's adapter),
`games/` a crate per game family and its `-wasm` crate (`tile-rummy`,
`grid`). In `web/src/`,
`platform/` is shared (i18n, settings, storage, ui) and `games/<family>/`
a game's app, boards and engine WASM (`tile-rummy`, `grid`); `App.tsx`
routes to the games page or a game's page (`pages.ts`).

## Commands

From the repo root:

```sh
cargo run -p server                 # match server, ws://0.0.0.0:3000/ws/<game>
cargo run -p server -- --open-racks # the same, every rack shown to everyone (judging bots)
# also: --lookahead OPTIONS,GAMES,ROUNDS (Extra Hard, default 1,24,2),
# --bot-time MS (longest a bot thinks, default 1000), --bot-threads N
# (bots thinking at once, default cores - 1), --games tile-rummy,grid
# (the games served, default all); see `config` in main.rs
cargo test --workspace              # one test: cargo test -p tile-rummy runs
cargo clippy --workspace --all-targets
```

From `web/`:

```sh
npm run dev       # builds the engine WASM (debug), then Vite on http://localhost:5173
npm run build     # release WASM + type check + production build into web/dist
npm run lint      # oxlint
npm test          # Vitest (web/src/**/*.test.ts); one test: npx vitest run -t "name"
npm run wasm      # only rebuild every game's engine WASM (scripts/wasm.mjs; needs wasm-pack)
```

`http://localhost:5173/` is the games page; `/rummy`, `/okey-101`, `/okey`,
`/tic-tac-toe` and `/gomoku` a rule set's page each.
`http://localhost:5173/?demo` shows the board with a made-up 4-player game
and no server (`src/games/tile-rummy/demo.ts`): the place to work on the board's look.
`?demo=racks` adds the open racks panel; `?demo=watcher` is a bots-only
game seen by its watching host (Pause works). `?lang=de` (any page) shows
a language without changing the setting; `?lang=en-XA` (development only)
is English accented and 40% longer, to find text that clips or isn't
translated.

`cargo test -p tile-rummy --release -- --ignored --nocapture strength` plays
the bot levels against each other and prints wins and time per move;
`cargo test -p grid --release -- --ignored --nocapture grid_strength` the
grid bots, on Tic-Tac-Toe and Gomoku (`GRID_GAMES`, `GRID_MS`).

Run the server and `npm run dev` together. After changing Rust in
`games/tile-rummy*` or `games/grid*`, rerun `npm run wasm:dev` (Vite then reloads); `npm run dev`
only builds it on start.

## Architecture

- **`games/tile-rummy`** (crate `tile_rummy`): the game rules, pure (no I/O, clock or outside
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
  `okey101` is the Okey 101 rule set (four rule sources compared in its
  header): an indicator picks the wild okey, jokers stand for the okey's
  tile; a `Turn` is sent whole (draw or take the previous player's
  discard, melds, one discard), the table's sets only added to at their
  ends (an okey can be taken back by swapping in its tile), opening with
  101 points or five pairs (`progressive` raises the bar); `Hand` is one
  hand, `Match` several, scored in penalty points, lowest wins.
  `okey101::bot_move` (`okey101/bot.rs`) is a simple bot: the classic
  solver on its rack (okey as its joker) for the best opening or the most
  tiles, pairs if it can't open with sets, single tiles onto the table's
  ends, the discard taken only when it opens or plays more with it, and
  the least linked tile that costs nothing discarded (Easy, Medium). Hard
  and Extra Hard also keep pairs on a pair-heavy rack and, between tiles
  equally linked, discard the one of least use to the next player
  (`Care`); they win 60.5% of matches against Easy. `okey_strength`
  (ignored test, `--release`; `OKEY_GAMES`, `OKEY_CARE`) measures them.
  `plain_okey` is plain Okey ("düz okey", pagat's rules): no table, a
  turn is draw or take the previous discard, then discard; `finish` shows
  the other 14 as runs and groups (12-13-1 allowed here, not in 101) or
  seven pairs (`complete`, its own search as the classic solver has no
  wrap). Points from 20 down: 2 a win, doubled on the okey and for pairs;
  the indicator's twin shown at the deal. `plain_bot_move` finishes when
  it can (on the okey first) and keeps the 14 that cover most.
  Its `wasm` feature derives
  `tsify::Tsify` on the types that cross into JS; serde attributes define the
  TS shape (e.g. `Tile` is `{ kind: "number", color, value } | { kind:
  "joker" }`), and JSON field names are camelCase.
- **`crates/protocol`**: the shared messages, `ClientMessage<R, L, M>` /
  `ServerMessage<R, L, M>` (`R` the game's rules in `RoomOptions.game`,
  `L` its bot levels, `M` its messages), one JSON text frame each, tagged
  by `type`; a game's own ride inside as `{ type: "game", message }`,
  defined in its engine crate (`tile_rummy::protocol`: `RummyRules`,
  `RummyRequest`, `RummyMessage`; `grid::protocol`). Depends on no game.
  `Welcome { protocol, game }`: `PROTOCOL_VERSION` (the shared messages)
  and the game's own (`RUMMY_PROTOCOL`, `GRID_PROTOCOL`); bump the one
  whose messages change incompatibly. Never serialize `null`: tsify types say
  `field?: T` for an `Option`, so use `skip_serializing_if = "Option::is_none"`
  with `#[tsify(optional)]`.
  Messages say what happened, never a sentence for players: `Notice`,
  `Refusal` and `TurnError` are enums the client words in the player's
  language (`web/src/platform/messages.ts`, each game's `messages.ts`; tile rummy's `RummyNotice`,
  `TurnError` and the like inside `game`); `Error { message }` is only for
  messages the server couldn't read. A new thing to tell players is a new
  variant, not text.
- **`games/tile-rummy-wasm`**: the browser API (`wasm-bindgen`): engine
  functions plus `protocolVersion` and `gameProtocolVersion`. wasm-pack writes JS glue and `.d.ts` into
  `web/src/games/tile-rummy/wasm/` (gitignored), so TypeScript types for the engine *and* the
  protocol come from Rust. A tsify type only reaches the `.d.ts` if an
  exported function uses it: `readServerMessage` / `readClientMessage`
  (with `unchecked_return_type` naming the generic types filled in) bring
  the protocol's. `web/src/games/tile-rummy/protocol.ts` names them
  filled in (`RummyRoom`, `RummyOptions`, ...) and `wire` puts a board's
  game requests inside `game`. Engine types cross as
  `tsify::Ts<T>` and are converted inside the function; don't use tsify's
  `into_wasm_abi`/`from_wasm_abi` (deprecated, they leak on bad input).
- **`games/grid`** (crate `grid`): grid placement, pure like `tile_rummy`:
  `GridRules` (named rule sets `TicTacToe` 3×3 line 3 and `Gomoku` 15×15
  line 5, freestyle: a longer line wins too; sides 3-19 and the line as
  house rules, `checked()`), `Game` (X, seat 0, first; `place`,
  `GridError`, the outcome with its line, views, `placings`),
  `check_place`, `outcome_of`. Bots (`bot.rs`, over `board.rs`'s windows
  of `line` cells kept up to date): every level makes a line it can;
  Easy blocks one, else one move ahead with hash noise; Medium two moves,
  Hard four (grids of up to 12 cells searched to the end, so it never
  loses Tic-Tac-Toe), Extra Hard deepening until `spent` (negamax with
  alpha-beta over cells near the marks).
- **`games/grid-wasm`**: `checkPlace`, `outcome`, `botMove`, as
  tile-rummy-wasm, into `web/src/games/grid/wasm/`.
- **`crates/platform`**: what every game's rooms share; depends on no
  game.
  - `engine.rs`: the `GameEngine` trait a game implements for the room:
    `Rules` (the game's part of the room options), `Request` (what a
    player sends, sorted by `request` into a move, draft or aside),
    `Move`, `Level`, `Config` (server setup for its bots and clock),
    `Message`, `PROTOCOL`; `checked`, `stand_in`; `new`,
    `seats`, `current`/`in_turn`/`over`, `ends_turn` (asked before
    applying, so the bank is settled first), `apply`, `time_out`
    (`TimedOut`: extra time kept, a notice), `bot_move` with `spent`,
    `bot_fallback` (only for a refused move that would end the turn),
    `next_hand`, `state` (with `ClockInfo`, `open` for hidden hands),
    `placings`. Hooks with defaults, for games with live turns and extra
    time: `Draft`, `Aside`, `draft_allowed`, `draft_message`, `worth`,
    `move_draft`, `shown`, `aside`.
  - Speaks `protocol`'s shared messages: a connection receives `Out<G>`,
    `ServerMessage` filled in with the game's types. `Rooms::create`
    checks the options (`checked_turn`, the game's `checked` and
    `stand_in`); `RoomInfo` carries the game's `min_seats`/`max_seats`.
  - `rooms.rs`: open rooms of a game by 4-letter code (no I or O);
    `Rooms::new` takes the bot permits (`bot_pool`), so games can share
    them.
  - `room.rs`: `Room<G>`, each room its own task (an actor) owning its
    players, the game and the turn timer; connections send it
    `Command<G>`s over a channel, so game state has no locks.
  - Tested with a counting game (`tests.rs`, a paused clock).
- **`crates/server`**: tokio + axum match server, a WebSocket route per
  game (`/ws/tile-rummy`, `/ws/grid`; `--games` picks which), each with
  its own `Rooms<G>` (room codes are a game's own) and one shared bot
  pool. `connection.rs` is generic over the game. `contest.rs` is tile
  rummy's `GameEngine` (`RummyMove`, `RummyConfig`: Extra Hard's
  `Lookahead` and `clock.rs`'s `Bonus`); `grid.rs` grid placement's
  (`GridMatch`, a newtype: the trait and `grid::Game` are both foreign
  there); `grid_tests.rs` drives it through the room's commands.
  - The room (`platform`'s `room.rs`, as tile rummy uses it): players
    are identified by a stable `PlayerId` (seats shift when those who
    left are dropped at the start)
    and keep their seat across lost connections with a token (`rejoin`).
    `RoomOptions` (chosen on the home screen): turn length (30 s to 3 min,
    default 1 min; `checked_turn()` keeps it to `TURN_SECONDS`), a time bank
    (2 min a player, used once a turn's own time runs out; not while away;
    `settle_bank` charges it before each move; time left when a player ends
    their turn is added, up to 3 min, less any extra time they were given
    for tiles they took back), extra time for tiles
    (server's `clock.rs`: 15 s a tile and 20 s a set holding one, once a draft is a
    valid turn, only the amount above the best so far this turn, sent as
    `Clock`; time given for tiles then taken back counts against the
    player: a `Notice` at 1 min, none for the rest of the game at 2), live
    turns, the stand-in bot (`stand_in`, default Medium, never Extra
    Hard), and the rule set (`rules`: classic, or Okey 101 with its hands,
    rising bar and penalties; no extra time for tiles there), and for
    Okey, marking false okeys (`falseOkeyMarks`, off: a corner numeral
    of the tile they stand for, `FalseOkeyMark` context read by
    `TileFace`). The room
    plays a `Contest`: the classic `Game` or an Okey
    `Match`. In Okey 101 `Draw` doesn't end the turn, `Discard` does;
    states are `OkeyState`, errors `OkeyRejected`; when time runs out the
    player draws if they hadn't, their draft is played if valid and the
    first of the tiles they set aside to discard next (`DiscardNext`, the
    room's `Aside`, up to 3 in order, kept by `Contest`, cleared each
    hand) that they hold
    and that costs no penalty is discarded, else a tile that costs
    nothing (`Hand::timeout_turn`; plain Okey: a winning finish, else the
    first of those held, else an Easy bot's discard,
    `plain_okey::timeout_turn`); between
    hands the scores show for `Timing::between_hands`. Leaving for good (`Leave`, the board's door button; not a lost
    connection, `Command::Disconnect`): in the lobby, or watching, the
    member is gone; during a game the stand-in takes over the seat, under
    the player's name, and a turn of theirs starts afresh for it.
    `Timing`: a minute's turn is 60 s, scaled by the room's
    length (when it runs out, the most of the table they left, their last
    `draft`, that makes a valid turn is played, `tile_rummy::salvage`; the rest
    goes back to their rack, and a `Notice` says so), 5 s for a player who
    is away (their full time is given back if they return during that turn),
    room closes after 120 s with nobody connected. Bots (`AddBot`, host
    only, before the start) are players without a connection: on their
    turn the room waits, works the move out on a blocking thread
    (`Command::BotMove`, dropped if the turn has passed; at most
    `--bot-threads` at once across rooms, a shared semaphore, each
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
  animation, Inter via fontsource. `src/main.tsx` awaits the language
  before rendering. The board is HTML elements, not a canvas.
  - Pages (`pages.ts`, `App.tsx`): `/` the games page (`GamesPage.tsx`: a
    card per family, links to its rule sets' pages), a page per named rule
    set in its family's app, `React.lazy` so a family's code and WASM load
    only on its pages (each app's module awaits its engine's init). A new
    rule set's page: a path in `PAGES` and a link on the games page.
  - `platform/`: what every game's page shares. `useRoom.ts` owns the
    connection to `/ws/<game>`: the seat token in `sessionStorage` per game
    (per tab, so two tabs are two players), rejoining after a reload,
    retrying a lost connection, room requests sent as they are and the
    game's inside `game`, the game's messages to its `onGame`.
    `RoomScreens.tsx`: the page's header (back to all games), the home
    screen (name, create/join around the game's options), the lobby,
    `TurnTime`, `StandIn`, `Advanced`, `Choice`, `Option`; `WatchBar.tsx`;
    `protocol.ts` (the shared types, from tile rummy's WASM build, types
    only); `messages.ts` (refusals, the room's notices).
  - `games/tile-rummy/`: `RummyApp.tsx` (its home options, lobby rules,
    boards and `?demo`s), `useRummy.ts` (its states, on `useRoom`),
    `protocol.ts` (the generic types filled in), `messages.ts`, the boards.
  - `games/grid/`: `GridApp.tsx`, `useGrid.ts`, `GridBoard.tsx`: the grid
    on paper (`.paper` in `index.css`; light, or dark as a setting,
    `Settings.paper`), the winning line highlighted; a click places a
    mark (checked first, `checkPlace`). Tic-Tac-Toe: lines by hand, X and
    O drawn in pen in the cells (`marks.tsx`, Motion `pathLength`; rows
    fixed and the mark laid over its cell, so drawing never resizes it). Gomoku
    (`StoneBoard.tsx`, any grid over 25 cells): black and white stones on
    the intersections (black first; a rim, `--rim-*`, shows each on either
    sheet), star points, the last stone dotted; its lines are the paper's
    own drawn thicker: the board's spacing is a whole even number of
    pixels and the page's paper takes that spacing and offset
    (`onPaper`), re-measured as the board's area changes. Paper games
    keep this look; the tile games keep the felt and tray.
  - Room options on the home screen: only turn time is always shown; the
    rest are under "Advanced settings" (`Advanced` in `platform/RoomScreens.tsx`, folded,
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
    (`GameState.lastPlay` from `useRummy.ts`; flashes when it's news, then
    stays quiet until the table changes).
  - Live turns: the current player's board sends its table after each
    drop (`draft`, in every room: it's what's played if time runs out); the
    server checks it uses only tiles from the table and that player's rack
    (`tile_rummy::within_reach`) and keeps it until the next state. In rooms
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
  - Double-clicking a rack tile: on your turn it goes where the player
    last changed a set, the table or a plan (`lastIn`), else into a plan.
    It joins the set last changed there (by drag or double-click) if it
    fits (`joinAt`: at an end, or into a run of its
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
  - Open racks: when states carry `racks`, `useRummy.ts` logs each turn
    (tiles played, tile drawn) and the board shows the others' racks and
    the log above the table (`OpenRacks`). Each of those racks can be
    sorted or reordered by dragging (its own nested `DndContext`), the
    order kept per tab like the player's own.
  - `settings.ts` / `SettingsMenu.tsx`: device preferences (rack style: flat
    tray or two-row rack; watching live turns; the paper games' sheet;
    language) behind the gear in the top bar, each game's own shown
    (`tiles`, `paper`).
  - "How to play" under the home and lobby box (`platform/HowToPlay.tsx`:
    the button, the rules unfolding below it as wide as the page, only
    the box itself `max-w-lg`; `RuleSection`s numbered, two to a row on
    wide screens; `RuleText`, `RuleList`; `useScene` plays a scene's
    frames while it's in view, the last frame for reduced motion). Tile
    rummy's (`Rules.tsx`, rule sets in `RULES`): scenes of real tiles
    gliding between frames (Motion `layoutId`; fine here, nothing is
    draggable), the board's outlines. The grid's (`games/grid/GridRules.tsx`):
    scenes on a little sheet of paper, marks drawn and stones set a move at
    a time, the winning line highlighted; Gomoku's paper lines aligned
    with its board's as on the real one (`usePaperLines`). Texts under
    `rules` in the catalogues.
  - Languages: react-i18next, every visible text through `t('key')` (no
    text in components). `src/platform/locales/en.json` is the source and types the
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
    only pages showing it download it). Japanese, Korean and Chinese fonts
    are loaded with their language instead (`fonts` in `i18n.ts`; their
    CSS alone is 100 kB each), each set by `:lang()` in `index.css`, as
    they draw shared characters differently. Chinese is `zh-CN` and
    `zh-TW` (Hong Kong and `zh-Hant` get Traditional). Don't build
    sentences by gluing texts with spaces (`board.sentences` joins two;
    Chinese and Japanese put none). Right-to-left languages (Hebrew,
    Arabic) set `<html dir>`: use logical classes (`ms-`, `pe-`, `start-`,
    `end-`, `text-start`), and the playing surface (table, rack, plans,
    open racks' tiles) stays `dir="ltr"` so runs read 1 2 3.
  - `OkeyBoard.tsx`: Okey 101's board (rooms whose `rules` are Okey 101
    get `okeyState`, kept as `OkeyGame` in `useRummy.ts`). The turn is built
    on the board and sent whole as `discard` when a tile is dropped on the
    discard spot (checked first in the browser, `checkOkeyTurn`): draw
    (button or pool), or drag the previous player's discard in; drop tiles
    on a set to add them at its nearer end (or in an okey's place, taking
    the okey back), or on empty table to start a set there. Only this
    turn's tiles move again. The table is the classic board's grid
    (`TableGrid`, `useTableMetrics`): sets move by their grip, any time,
    for this player only; their places are kept by table index (the
    table only grows at its end; a new set left empty keeps its slot in
    `sets` until the turn is sent), per tab for the hand (`TABLE_KEY`).
    Plans as in the classic game (`Plans`, kept with the rack order),
    with what the valid ones would count towards opening. Double-click:
    once the player may lay tiles down, a rack tile onto the table, into
    the set last changed if it fits (ends only for sets already there),
    else a new set, and a planned tile's whole plan onto the table;
    otherwise a rack tile into a plan and a planned tile back; this
    turn's tile on the table back to the rack. The rule behind it, for
    other games: the second brain's
    `projects/rummy-game/notes/board-controls.md`.
    Indicator, okey and pool top left, everyone's discards top right, the
    opening total as it builds (`okeyProgress`), the scores between hands
    and at the end. Shares `TopBar` and `WatchBar` with `Board.tsx`; the
    rack order is kept per tab. `?demo=okey`, `?demo=okey-scores` and
    `?demo=okey-marks`.
  - `PlainOkeyBoard.tsx`: plain Okey (`rules` kind `okey`, states
    `plainOkeyState`): no table; draw or take the discard, then drop a
    tile on the discard spot, or on the finish spot in the middle to show
    the other 14 as a winning hand (checked first, `okeyComplete`). Shares
    the Okey board's parts (`okeyParts.ts`, `Middle`, `DiscardPile`,
    `Rack`, `DiscardSpot`, `Overlay`, `Plans`: a rack tile double-clicked
    goes into a plan, any time, plans checked as plain Okey's sets with
    12-13-1, `plainSet`). Both Okey boards mark the tile last drawn
    (`OkeyGame.drawn`, from the phase going draw to play in `useRummy.ts`),
    and have a slot beside the discard spot for up to 3 tiles to discard
    next (`NextSpot`, the first nearest the spot; a tile goes in where
    it's let go, `enqueue`), told to the server as they change
    (`discardNext`); plans and those tiles are kept per tab for the hand.
    `?demo=plain`, `?demo=plain-scores`,
    `?demo=plain-marks`.
  - Planned: shadcn/ui for menus, `@heroiclabs/nakama-js` for
    accounts/matchmaking.

The client reaches the server at `/ws/<game>` on its own origin: Vite proxies `/ws` to
port 3000 in development (`vite.config.ts`), and `VITE_SERVER_URL` (up to `/ws`)
overrides it. Vite listens on all interfaces, so a phone on the LAN can open
`http://<pc-ip>:5173`.

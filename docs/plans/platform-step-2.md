# Migration step 2: the grid family and the generic platform

A plan to work from in a fresh session (e.g. Claude Code in the cloud,
which has this repo but not Tom's second brain). Written 2026-10-29 from
`crates/server/src/room.rs`, `contest.rs`, `connection.rs`, `rooms.rs` and
`crates/protocol/src/lib.rs` at `99cf318`.

## The task (Tom's words, condensed)

- A new engine crate `games/grid` (plus `games/grid-wasm`), pure like
  `tile_rummy`: Tic-Tac-Toe and Gomoku as named rule sets first (board
  size and line length as settings), a minimax/MCTS bot per level.
- Extract `crates/platform` from `crates/server`: a `GameEngine` trait and
  a generic `Room<G>`, from `room.rs` and `contest.rs`. Tile rummy's
  drafts, extra time for tiles and salvage become hooks with defaults.
  Shape the trait on both families, not just rummy.
- Tile rummy behaves exactly as before: `crates/server/src/tests.rs`
  passes unchanged, or with mechanical edits only (imports, struct
  literals).
- The protocol split (step 3) and the web board (step 4) are not part of
  this, unless the trait can't be tested without them. It can (see
  "Testing the trait" below), so the wire format doesn't change and
  `PROTOCOL_VERSION` stays 12.
- Commit each step separately once `cargo test --workspace`,
  `cargo clippy --workspace --all-targets` and `npm run lint` (in `web/`)
  pass. Stage files by name, never `git add -A`. Work on `main` and push
  after each commit.

## Background from the second brain (decisions already made; don't reopen)

From `projects/game-platform/decisions.md`:

- **2026-10-28, one repo, a generic room over a `GameEngine` trait.**
  Crates: `platform` (generic `Room<G>` actor, rooms, clock and bank,
  connections, watchers, bot pool; later saving rooms, Nakama), `protocol`
  (shared messages plus a payload per game), `server` (the binary), and
  `games/<family>` + `games/<family>-wasm`. The trait: rules (a named
  rule set plus house rules) and a seed make a game; it says who acts
  (one seat; later several, as in Mahjong's claims, or all), applies
  moves, handles a timeout (rummy's salvage), gives each seat's or a
  watcher's view, the placings at the end, and a bot move with a time
  budget. Rummy-only features become hooks with defaults (drafts for live
  turns, extra time for tiles). The rooms map will hold type-erased
  handles, connections passing the game payload as raw JSON (step 3).
  Platform protocol and each game's protocol are versioned separately
  (step 3). Rejected: making code generic before the second game exists
  (a trait shaped by one game carries its assumptions).
- **2026-10-28, paper games.** Grid placement is a family: Tic-Tac-Toe,
  Gomoku, later Notakto, Order and Chaos, S.O.S, Ultimate Tic-Tac-Toe;
  settings change board size, line length, which marks a player may
  write, whether a line wins or loses, nested boards. Nothing hidden or
  random, so bots are plain minimax or MCTS. It is the cheap test of the
  trait; a trait tested on Tic-Tac-Toe and later Tysiąc is less likely to
  carry card-game assumptions.
- **2026-10-28, one match server for every game**, a `--games` flag later
  to pick which a process serves. The bot-thread limit protects the
  machine only if every game's bots share it (one semaphore).
- **2026-10-29, the migration order.** Step 1 (`99cf318`) moved files
  only. Step 2 is this. Step 3: protocol split into shared messages plus
  a payload per game. Step 4: grid board, a games page, a route per game;
  split `web/src/App.tsx`, `useGame.ts`, `messages.ts`. A commit of
  nothing but moves is easy to review and keeps `git log --follow`
  useful, so moves and logic changes go in separate commits here too.

## What the room does today, and who owns it after

`room.rs` (1183 lines) is almost all generic already; `contest.rs` is the
seam. Sorted:

**Platform (generic `Room<G>`):** room code, members, seats and watchers,
`PlayerId`s and tokens, join/rejoin/disconnect/leave, the host, the
lobby (watch, add/remove bot, start), the stand-in bot for a player who
leaves, pause and step, the turn counter and timer, the short turn for
an absent player and its give-back on return, the time bank
(`settle_bank`, `deadline`, banking time left at `end_turn`), the bot
pipeline (think → choose on a blocking thread under the shared
semaphore with a `spent` budget → show → play, late moves dropped by
turn number), closing an empty room, broadcasting room info and states,
time between hands (`next_hand_at`), and the bookkeeping of extra time
(granted this turn, `unearned`, warning, no more for the game), since
"extra time a player was given and then took back" is game-agnostic
once a hook says what a draft is worth.

**Tile rummy only (behind hooks or in its adapter):** what a move is
(`Play`, `Draw`, `Discard`), drawing in Okey not ending a turn, the draft
being a table of tiles and `within_reach`, what a table is worth in
extra time (`clock.rs`'s `Bonus`), salvage and the Okey timeouts, the
`TimeRanOut { played, back }` count of tiles, the queue of tiles to
discard next (`Player.discard_next`, at most 3, cleared each hand), open
racks' contents, Extra Hard's `Lookahead`, the bot's fallback when its
move is refused, which bot moves are shown as a draft, and the three
state messages (`State`, `OkeyState`, `PlainOkeyState`) and two
rejections (`Rejected`, `OkeyRejected`).

## The `GameEngine` trait (proposal)

In `crates/platform/src/engine.rs`. Associated type defaults are
unstable, so every engine names its hook types; a game without drafts
uses `()` and keeps the default methods. Names below are proposals.

```rust
pub trait GameEngine: Clone + Send + Sync + 'static {
    /// The room's rule set for this family (named rule set + house rules).
    type Rules: Clone + Send;
    /// A move as the room receives it from a player or a bot.
    type Move: Send;
    /// How well a bot plays (each family its own enum; both have
    /// Easy, Medium, Hard, ExtraHard today).
    type Level: Copy + Eq + Send;
    /// Server-wide setup for this game's bots and clock (rummy:
    /// `Lookahead`, `Bonus`).
    type Config: Clone + Copy + Send + Sync;
    /// What the room sends for this game: its state, its rejections,
    /// its drafts and notices.
    type Message: Clone + Send;
    /// The current player's work in progress (rummy: the table). `()`
    /// for games without live turns.
    type Draft: Clone + Send;
    /// Something a seated player may say any time that isn't a move
    /// (rummy: tiles to discard next). `()` if none.
    type Aside: Send;

    fn new(rules: &Self::Rules, players: usize, seed: u64) -> Self;
    /// How many may sit at the table (rummy 2..=4, grid 2..=2).
    fn seats(rules: &Self::Rules) -> RangeInclusive<usize>;
    fn players(&self) -> usize;

    /// Who acts now. One seat for both families; an enum leaves room for
    /// several (Mahjong's claims) or all (simultaneous moves) later.
    fn phase(&self) -> Phase; // Turn(seat) | Between (hands) | Over
    // convenience: current(), in_turn(), over() as default methods on top

    /// Whether this move ends the turn (Okey's draw doesn't), asked
    /// before applying: the room settles the bank first, as today.
    fn ends_turn(&self, _mv: &Self::Move) -> bool { true }
    /// Applies a move by `seat`; `Err` is the message to send back.
    fn apply(&mut self, seat: usize, mv: Self::Move) -> Result<(), Self::Message>;
    /// Time ran out for the current player: play something for them
    /// (rummy: salvage the draft, else draw; Okey: draw, timeout_turn;
    /// grid: an Easy bot's move). Returns a notice for that player.
    fn time_out(&mut self, draft: Option<Self::Draft>) -> TimedOut<Self>;
    /// A bot's move for the current seat. `spent` is the share of its
    /// time used (the engine has no clock).
    fn bot_move(&self, level: Self::Level, config: &Self::Config, spent: &dyn Fn() -> f64) -> Self::Move;
    /// A bot's move was refused: make some legal move instead (rummy:
    /// draw; Okey: draw if needed, then a safe discard).
    fn bot_fallback(&mut self);
    /// Deals the next hand after `Between`; false if there's none.
    fn next_hand(&mut self) -> bool { false }

    /// The state message for a seat, or a watcher (`None`). `open`:
    /// show every hidden hand (--open-racks, or watchers of bots only).
    fn state(&self, seat: Option<usize>, clock: &ClockInfo, open: bool) -> Self::Message;
    /// Places at the end, 0 best, ties sharing a place: for results and
    /// ratings later. None until over.
    fn placings(&self) -> Option<Vec<u32>>;

    // Hooks with defaults: live turns and extra time.
    /// Whether this draft uses only what the current player may use.
    fn draft_allowed(&self, _draft: &Self::Draft) -> bool { false }
    /// The message relaying a draft to the others.
    fn draft_message(&self, _by: usize, _draft: &Self::Draft) -> Option<Self::Message> { None }
    /// Extra time this draft (or move) would earn its player.
    fn worth(&self, _config: &Self::Config, _seat: usize, _draft: &Self::Draft) -> Duration { Duration::ZERO }
    fn move_draft(_mv: &Self::Move) -> Option<&Self::Draft> { None }
    /// What of a bot's move to show as a live draft before playing it.
    fn shown(&self, _mv: &Self::Move) -> Option<Self::Draft> { None }
    /// A seated player's aside (rummy: discard-next queue, truncated
    /// to 3 and kept per seat in the adapter, cleared by next_hand).
    fn aside(&mut self, _seat: usize, _aside: Self::Aside) {}
}
```

`ClockInfo` carries `turn_ms`, `banks`, `next_hand_ms`, which the room
computes as now. `TimedOut<G>` holds the extra time the played part
kept (for unearned accounting) and an optional `G::Message` notice.

Tile rummy's adapter is today's `Contest` (stays in
`crates/server/src/contest.rs`): `impl GameEngine for Contest`, with
`Move = RummyMove { Play(table), Draw, Discard(Discard) }` (today's
`Action`), `Message = protocol::ServerMessage`, `Draft = Vec<Vec<Tile>>`,
`Aside = Vec<Tile>`, `Level = tile_rummy::BotLevel`,
`Config = { lookahead, bonus }`. The discard-next queue moves from
`Player` into `Contest`. `okey_time_out`, `plain_time_out` and the
classic salvage in `room.rs` move into `Contest::time_out`; the three
fallbacks in `bot_moves` into `bot_fallback`.

### Messages without the protocol split

The room also says things of its own: `Joined`, `Room`, `Refused`,
`Clock`, and the extra-time notices `UnearnedWarning` / `NoMoreBonus`.
Proposal (decision 1 below): `platform` defines these as its own types
(`platform::Out<G::Message>` = `Shared(SharedMessage) | Game(G::Message)`,
plus `Refusal`, `Seat`, `RoomInfo` generic over the options and level),
serde-identical to protocol's. `platform` depends on neither `protocol`
nor `tile_rummy`. The server's connection converts `Out<ServerMessage>`
into `protocol::ServerMessage` one to one, so the wire is byte-for-byte
the same. Step 3 then removes the duplicates by making `protocol`'s
shared part *be* these types.

Room options split the same way: `platform::RoomOptions<R, L>` with
`turn_seconds`, `live_turns`, `time_bank`, `stand_in: L`, and the game's
part (`R`: rummy's `rules`, `tile_bonus`, `false_okey_marks`); the server
converts from `protocol::RoomOptions` after `checked()`. `RoomInfo.options`
must still serialize as today (flatten or convert back in the server).

`Command<G>` keeps today's lobby commands and replaces `Play`, `Draw`,
`Discard`, `Draft`, `DiscardNext`, `BotMove` with
`Move { player, mv: G::Move }`, `Draft { player, draft: G::Draft }`,
`Aside { player, aside: G::Aside }`, `BotMove { turn, mv: G::Move }`.
`connection.rs` (stays in `server`, rummy-only until step 3) maps
`ClientMessage` to these. `Rooms<G>` is generic for now; type erasure
across games is step 3, when the server serves a second one. The bot
semaphore is still one `Arc<Semaphore>`, passed in, so it can be shared
across `Rooms<A>` and `Rooms<B>` then.

### Behaviour to keep exactly (check each after the move)

- `make_move`: `settle_bank` runs before applying when the move would
  end the turn, even if the move is then refused; `left` is measured
  before applying; `kept` is the move's worth before applying.
- Okey's draw: state broadcast, turn goes on; if the hand ended on that
  draw (pool empty), settle and end the turn. A bot's draw: think again
  after `bot_think`.
- Drafts: only from the current seat while in turn; refused with
  `NotYourTiles` when not allowed; plain Okey allows only an empty draft;
  relayed only with `live_turns`, but always kept for the timeout;
  `send_draft_to` on join/rejoin, never to the drafting player.
- A bot's move is shown as a draft only for a classic `Play` and an Okey
  `Turn` whose table differs from the current one, and only with
  `live_turns`; the show time counts from the pause start if paused.
- Extra time only in classic rooms with `tile_bonus`; `granted` reset at
  each turn start; `count_unearned` after banking.
- Timeouts: classic notice `TimeRanOut` only if tiles were put down;
  Okey draws first and may end the hand; plain Okey has no notice.
- Start: drop absent players, then `start_turn`, `broadcast_room`,
  `broadcast_state`, in that order. `TooFewPlayers { min: 2 }` and
  `RoomFull { max: 4 }` now come from `G::seats`.
- Open racks: `--open-racks`, or a watcher when every seat is a bot.
- Stand-in never Extra Hard (`checked()` in protocol; keep it there).

## The grid engine (`games/grid`, crate `grid`)

Pure: no I/O, clock or outside randomness; serde types with the same
`wasm` feature pattern as `tile_rummy` (camelCase JSON, `tsify`).

- `GridRules { width, height, line, rule_set }` with named rule sets:
  `TicTacToe` (3×3, line 3) and `Gomoku` (15×15, line 5), and house
  rules: board size 3..=19, line 3..=min(width, height). A line of
  `line` or more wins (freestyle; see decision 4).
- `Game::new(rules, players, seed)`: 2 players; marks X and O by seat.
  `place(seat, cell) -> Result<(), GridError>` (`NotYourTurn`,
  `Occupied`, `OffBoard`, `Over`). `outcome()`: a winner with the
  winning line's cells, or a draw when the board is full. `view(seat)`
  and `watcher_view()` (everything is public; `you` absent for
  watchers). `placings()`.
- `check_place(view, cell)` for the browser's instant feedback.
- Bots, `bot_move_with(view, level, spent)`:
  - Tic-Tac-Toe (and any small board): full negamax with alpha-beta and
    a transposition table; Hard and Extra Hard perfect; Easy picks a
    random legal move but takes an immediate win and blocks an immediate
    loss; Medium searches 2 plies. "Random" from a hash of the position
    and level, so the engine stays pure (as rummy's `EASY_SPOTS`).
  - Gomoku: candidate moves within 2 cells of a stone, ordered by a
    pattern score (fives, open fours, fours, open threes...); negamax
    with alpha-beta and iterative deepening, stopping when `spent()`
    reaches 1. Easy: 1 ply plus noise; Medium: 2 plies; Hard: 4 plies;
    Extra Hard: deepening to its time. MCTS only if alpha-beta proves
    weak at Extra Hard (measure first).
  - Tests: Hard never loses Tic-Tac-Toe from any first move; two perfect
    bots draw; Gomoku bots take a win in one and block an open four; an
    ignored `grid_strength` test (`--release`) plays levels against each
    other and prints wins and time per move, like `strength`.
- `games/grid-wasm`: `wasm-bindgen` functions `checkPlace`, `outcome`,
  `botMove` (for offline play later), tsify types. It builds with
  `cargo build -p grid-wasm --target wasm32-unknown-unknown`; wiring it
  into `web/` (`npm run wasm`, a `web/src/games/grid/wasm/` output) is
  step 4. Add a `[profile.release.package.grid-wasm] opt-level = "s"`.

The grid adapter, `impl GameEngine for grid::Game` with
`Draft = ()`, `Aside = ()`, `Message` a small serde enum
(`State { view, clock }`, `Rejected { error }`), `time_out` = an Easy
bot's move, `bot_fallback` = first free cell, lives in
`crates/server/src/grid.rs` (not served yet; no route, no protocol
variant).

## Testing the trait

- `crates/platform` unit tests with a tiny fake engine (a counter game)
  for the room's own logic where it's easier than through rummy: absent
  turn and give-back, bank, pause/step, late bot moves dropped.
- `crates/server/src/grid_tests.rs`: `Room<grid::Game>` driven through
  its command channel (no WebSocket, no protocol change): two bots play
  Tic-Tac-Toe to a draw at Hard; a person against a bot, with a refused
  move (occupied cell) and a timeout played for them; a watcher sees
  states; pause and step in a bots-only game; leaving hands the seat to
  the stand-in.
- `crates/server/src/tests.rs` (rummy through real WebSockets) passes
  unchanged except imports and `Timing { bonus, .. }` / `Config {
  lookahead, .. }` literals moving into the rummy config.

## Commits

Each passes `cargo test --workspace`, `cargo clippy --workspace
--all-targets` (no warnings) and `npm run lint` in `web/`.

1. **Grid engine: Tic-Tac-Toe and Gomoku rules.** `games/grid`: rules
   and named rule sets, `Game`, views, errors, `check_place`, tests.
2. **Grid bots.** Alpha-beta per level with a time budget; tests;
   ignored `grid_strength`.
3. **Grid WASM crate.** `games/grid-wasm` with its bindings; builds for
   `wasm32-unknown-unknown`; not yet used by `web/`.
4. **Platform crate, moves only.** `git mv` `room.rs`, `rooms.rs`,
   `clock.rs` into `crates/platform/src/`; `platform` temporarily
   depends on `tile_rummy` and `protocol`; `server` uses it. No logic
   changes (imports and `pub` only), so `--follow` works.
5. **`GameEngine` trait, rummy behind it.** The trait; `Contest`
   implements it (moves, timeouts, fallbacks, drafts, worth, aside,
   discard-next queue moved out of `Player`); `Room` calls only trait
   methods but still names `Contest`.
6. **Generic `Room<G>`.** `Room<G>`, `Command<G>`, `Rooms<G>`;
   `platform`'s own message, refusal, seat, room-info and options
   types; `server` converts to `protocol`; `platform` drops its
   `tile_rummy` and `protocol` dependencies; `Bonus` and `Lookahead`
   into rummy's `Config`. Mechanical edits to `tests.rs` only.
7. **Grid rooms.** The grid adapter and `Room<grid::Game>` tests.
8. **Docs.** `CLAUDE.md`: layout (`crates/platform`, `games/grid*`),
   commands (`cargo test -p grid`, `grid_strength`), the architecture
   section (the trait and its hooks; what moved out of `room.rs`).
   Delete this plan (`docs/plans/`) in the same commit: once done, its
   content lives in the code, `CLAUDE.md` and the history. Not the
   second brain (not in this repo): give Tom the log and decision
   entries for it in the final message instead.

## Decisions for Tom

Each has a recommended default; ask at the start of the session, and go
with the default for any he leaves open.

1. **Messages in step 2.** (Recommended) `platform` owns its shared
   message types and the server converts to `protocol`, wire unchanged;
   or: do the shared/game protocol split now (step 3 pulled in).
2. **Bot levels.** (Recommended) An associated `Level` per game, grid
   copying rummy's four names; or one shared `BotLevel` in a small pure
   crate both engines and `protocol` use.
3. **Where adapters live.** (Recommended) In `server`
   (`contest.rs`, `grid.rs`) until step 3; or in the engine crates
   behind a `platform` feature.
4. **Gomoku's line.** (Recommended) Freestyle: five or more wins, 15×15;
   or standard: exactly five (overlines don't count), as a later house
   rule either way.
5. **Who moves first.** (Recommended) Seat 0 (the host orders seats),
   alternating across a match later; or chosen by the seed.
6. **A grid turn's length.** The room's 30 s to 3 min choice as for
   rummy, or shorter choices (10 s, 20 s) for the grid family's rooms.
   (Recommended: decide in step 4, with the home screen; the room takes
   whatever `turn_seconds` it's given.)

## Setting up a cloud session

Linux is fine; nothing here needs Windows. `rustup` stable with
`clippy`; `rustup target add wasm32-unknown-unknown` for commit 3;
Node 22+ and `npm ci` in `web/` for `npm run lint`. `wasm-pack` is only
needed for `npm run build`/`npm run dev`, which this step doesn't
require. The Extra Hard and strength tests are `--ignored`; don't run
them in the normal loop.

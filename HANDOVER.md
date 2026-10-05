# HANDOVER — Catan multiplayer server

Developer handover for the self-hosted Settlers of Catan app. Read this first if you are
picking the project up. The user-facing/deployment guide is in `README.md`.

---

## 1. Status summary

| Item | State |
|------|-------|
| Rules engine (base game, complete) | ✅ done, 22 unit tests passing (18 engine + 4 bot) |
| Random board generation each game | ✅ done |
| Multiplayer rooms (2–4 players) | ✅ done (cookie sessions + SSE) |
| Web UI (server-rendered HTML + SVG board + colonist-style fixed layout) | ✅ done, browser-tested (desktop + mobile) |
| Docker + systemd + docs | ✅ done |
| **AI bots** | ✅ done — three difficulty tiers, paced turns (see §9) |
| Expansions (Seafarers / Cities & Knights) | ❌ not started (architected to allow) |
| Release binary size | ~1.1 MB (Windows); similar on Linux musl |

Verified end-to-end via HTTP/curl and in a real browser (Chrome DevTools MCP): create →
join → add bot → start → setup placement → roll → build/trade/dev → robber → turn
enforcement → SSE fragments → disk snapshot. Desktop and mobile viewports both pass.

---

## 2. How to build, run, test

```bash
cargo run                 # http://localhost:8080
cargo test                # 22 tests (18 engine + 4 bot)
cargo build --release      # optimized, ~1.1 MB binary
```

Toolchain: Rust stable, edition 2024 (developed against 1.99). No nightly features.

Environment:

| Variable | Default | Meaning |
|----------|---------|---------|
| `CATAN_ADDR` | `0.0.0.0:8080` | listen address |
| `CATAN_DATA_DIR` | `data` | room snapshot directory |

Deploy: `docker compose up -d --build`, or the systemd unit in `deploy/catan.service`
(details in `README.md`).

---

## 3. Repository layout

```
src/
  main.rs        Axum bootstrap, router mount, background sweeper/timer/bot tasks
  handlers.rs    HTTP routes, cookie sessions, SSE stream, action parsing/dispatch
  render.rs      ALL HTML (maud) + inline SVG board + per-viewer fragments + bottom dock/sheets
  state.rs       AppState, Room, Member, room codes, join/start, disk persistence, bot driver
  bot.rs         AI bot policy — choose_action() returns one legal Action per call
  game/          pure synchronous rules engine (no async, no IO)
    mod.rs       module list
    board.rs     hex/vertex/edge geometry, random generation, ports
    state.rs     GameState, Player, Phase, TradeOffer, BotLevel, RNG field
    actions.rs   Action enum, apply(), all validation, legal-move enumeration
    resources.rs Resource, ResourceHand, Bundle, costs, DevCard, deck
    scoring.rs   Longest Road, Largest Army, victory points, win check
    rng.rs       Rng64 (serializable splitmix64) — enables exact snapshot resume
static/          htmx.min.js + sse.js (vendored) + app.css + fonts/ + ui/trade/discard/timer/lobby/pwa/audio.js + sw.js
Cargo.toml       deps + [profile.release] size tuning
Dockerfile, docker-compose.yml, deploy/catan.service, README.md
```

---

## 4. Architecture & data flow

```
browser ──htmx POST──▶ /room/{code}/action ──▶ GameState::apply() ──▶ room.bump()
   ▲                                                                      │
   └──────────── SSE named HTML fragments ◀── watch channel ─────────────┘
```

- **Actions** are form/`hx-vals` POSTs to a single `action` endpoint. `parse_action`
  (handlers.rs) maps form fields → a typed `game::Action`, then `GameState::apply`
  validates and mutates. Errors come back as a small HTML body swapped into `#toasts`;
  success returns an empty 200 (clears the toast). Bots reach the same `apply` path via
  `state::tick_bots` → `bot::choose_action`, so bot and human actions are validated identically.
- **Live updates**: each room has a `tokio::sync::watch<u64>` version counter. Any change
  calls `Room::bump()`. Each connected viewer has its own SSE stream that, on every version
  change, **re-renders its own personalised fragments** (board, panels, controls, hand, log,
  trades, status, turn) and emits them as named SSE events. htmx's SSE extension swaps each
  event into the element with the matching `sse-swap="eventname"`.
- Because rendering is per-viewer, private info (your hand, your legal moves/controls) is
  never leaked to opponents. Never render the whole `GameState`; always go through
  `render::fragments(game, data, viewer)`.
- **Server-authoritative**: the client can only send action names + indices; all legality is
  decided in `apply()`.

### The engine is isolated on purpose
`game/` has no dependency on axum/tokio/HTML. This is what makes the 18 unit tests trivial
and what makes bots easy (§9). Keep it that way — put transport concerns in `handlers.rs`.

---

## 5. Rules engine notes (things a future dev must know)

- **Board geometry**: pointy-top hexes, axial coords, radius 2 (19 hexes). Vertices/edges are
  de-duplicated by rounding pixel coords ×1000 to `i64` keys (`board.rs::generate`). Produces
  the expected 54 vertices / 72 edges / 30 coastal edges / 9 ports (18 port vertices).
- **RNG**: `Rng64` is a serializable splitmix64 so a `GameState` snapshot (including RNG
  position) resumes exactly. **Do not** swap it for `ThreadRng`/`StdRng` without solving
  serialization.
- **Phases** (`state.rs::Phase`): `Lobby → Setup → Play`, with transient `Discard`,
  `MoveRobber{after_knight}`, `Steal{hex}`, `GameOver`. Building/trading is allowed before the
  roll (`Phase::Play` + `dice.is_none()`), but you cannot `EndTurn` before rolling.
- **Setup order** is snake: 4p `[0,1,2,3,3,2,1,0]`, 3p `[0,1,2,2,1,0]`, 2p `[0,1,1,0]`.
  The first player in that queue (`players[0]`) takes the first turn. (Convention chosen;
  revisit if you want strict official ordering.)
- **Dev cards**: `dev_cards` = playable, `new_dev_cards` = bought this turn (moved over in
  `end_turn`). One non-VP dev card per turn (`played_dev_this_turn`). VP cards auto-count.
- **Longest Road**: DFS longest trail over a player's edges; an opponent's building blocks
  traversal *through* its vertex (you may still end a road there). Card needs ≥5; ties keep
  the current holder, otherwise no award. See `scoring.rs::longest_road_length`.
- **Largest Army**: ≥3 knights, same tie/holder rule.
- **Discard on 7**: `floor(hand/2)`; multiple players can be pending simultaneously — the
  acting player in `Phase::Discard` is whoever is discarding, **not** `current`.
- **Bank is finite** (19 per resource); production and Year of Plenty respect it.
- Legal-move helpers used by the UI: `legal_settlement_vertices`, `legal_city_vertices`,
  `legal_road_edges`, `legal_robber_hexes`, `best_maritime_ratio`, `steal_candidates`.

---

## 6. Session & persistence model

- On create/join a random token is set as cookie `catan_{CODE}` (Path=/, HttpOnly, Lax).
  A user can be in several rooms (one cookie per room code).
- `Member` order == `Player` order, so `member_index(token)` **is** the `player_id`.
- `view_mode` (`Normal`/`PlaceRoad`/`PlaceSettlement`/`PlaceCity`) is per-member UI state for
  "click-the-board" building; it lives in `Member`, not in the engine.
- Snapshots: `{CATAN_DATA_DIR}/{CODE}.json`, written atomically after each successful action.
  Loaded on startup. A background task (`main.rs`) deletes rooms idle > 24 h. Bots live in
  memory only.
- Bot pacing lives on `RoomData` as `bot_next_ms` / `bot_key` (both `#[serde(skip)]`): the key
  identifies the current decision context (player + turn + phase + setup step), and a change
  re-arms the "first action" delay so each bot turn is watchable rather than instant.

---

## 7. Key files to touch for common changes

| Want to… | Edit |
|----------|------|
| Add/change a game rule or action | `game/actions.rs` (+ a test in `game/tests.rs`) |
| Change board shape/distribution/ports | `game/board.rs` |
| Change look/layout/interaction (dock, sheets, board art) | `render.rs` + `static/app.css` + `static/ui.js` |
| Change bot strategy/difficulty | `src/bot.rs` (+ a test in `bot.rs`) |
| Change bot pacing / turn timer | `state.rs` (`tick_bots`, `tick_turn_timers`) |
| Add an endpoint or change form parsing | `handlers.rs` |
| Change rooms/sessions/persistence | `state.rs` |
| Change deployment | `Dockerfile`, `docker-compose.yml`, `deploy/` |

---

## 8. Known gaps & gotchas

1. **SVG interaction needs dispatched events.** htmx attributes are placed directly on SVG
   elements (`circle`/`line`/`polygon`); `SVGElement` has no `.click()`, so harnesses must
   dispatch a bubbling `MouseEvent` (see `docs/BROWSER-TESTING.md`). Action feedback is
   retargeted server-side with `HX-Retarget: #toasts`, not an inherited target.
2. **Icons** — chrome/UI icons are inline SVG (Lucide, MIT) via `render.rs::ic`. Resource
   icons are Icons8 PNGs vendored under `static/icons/` (wood/brick/wheat/ore/sheep) and
   referenced by `res_icon_src`: as an HTML `<img>` via `res_glyph`, or an SVG `<image>` via
   `res_image` (board hexes + port badges). The desert keeps a small hand-drawn cactus. The
   UI font is self-hosted Nunito (`static/fonts/nunito-latin.woff2`). Note the Icons8 free
   tier expects attribution — swap for licensed assets before any commercial use.
3. **`connected` flag is approximate** — set on page load/SSE connect, not cleared in real
   time when a socket drops. There is no "player disconnected" UI yet.
4. **Single port per vertex.** Ports are stored as one `Option<PortKind>` per vertex; if two
   port edges ever shared a vertex the last write would win. With the current even spacing
   this never happens, but a custom port layout should store a list.
5. **Number tokens use the official balanced "spiral" placement** (`board.rs::spiral_order`):
   the fixed token sequence is dealt outer-ring-inward, counter-clockwise from a randomly
   chosen corner, skipping the desert. Terrain, ports, and the starting corner are still
   randomized, so boards differ each game while the number layout stays fair.
6. **Spectators**: a visitor with no cookie to an already-started game gets an error page,
   not a read-only view. Easy to relax if desired.
7. **2-player game** uses standard rules with 2 players (no official 2-player variant / no
   neutral-player rules).
8. **`README.md` is user-facing; this file is dev-facing.** Keep both updated.

### Toolchain gotchas encountered (so you don't re-hit them)
- `rand` 0.10 renamed traits: implement **`TryRng`** (`Error = Infallible`), not `RngCore`.
  `random_range` comes from the `RngExt` extension trait; `next_u64` from core `Rng`.
- `maud` supports hyphen/colon attribute names (e.g. `hx-post`, `sse-swap`) directly.
- `axum` 0.8 path params use `{code}`, not `:code`.
- `tokio-stream`'s `WatchStream` needs the `sync` feature.
- `uuid` needs the `v4` feature.
- `serde` needs the `derive` feature.
- On Windows, MinGW (`x86_64-pc-windows-gnu`) toolchain was used; `cargo` is at
  `~/.cargo/bin` (not on PATH by default in this environment).

---

## 9. AI bots

Implemented in `src/bot.rs` (policy) + `state.rs::tick_bots` (driver).

- `bot::choose_action(game, pid, level) -> Action` is **pure and deterministic** — it only
  reads `GameState` and returns one action, no randomness, so bot games are reproducible and
  ties break deterministically on index-ordered legal-move lists.
- It handles **every** phase, not just `Play`: setup placement, rolls, building, buying/playing
  dev cards, discard-on-7, robber placement, steal choice, and responding to shared trade offers.
- Three tiers share one scoring core; the tier decides how much the bot considers:
  - **Easy** — naive greed: expand first, cities last; no trading, no Year of Plenty/Road Building.
  - **Medium** — sound priority order (settlement → city → dev → road); uses ports, dev cards, trades.
  - **Hard** — scores every option by expected value and adds a Longest-Road lookahead.
- `bot::fallback_action` guarantees a legal action if the policy ever returns one the engine
  rejects, so a bug can't wedge a room.
- **Driver/pacing** (`state::tick_bots`, called every 700 ms from `main.rs`): one bot action per
  tick, gated by `RoomData::bot_next_ms`. A new decision context (via `bot_context_key`) applies
  a `BOT_FIRST_DELAY_MS` (4.5 s) pause, then `BOT_STEP_DELAY_MS` (2.5 s) between steps, so bot
  turns take roughly 10–20 s and are watchable rather than instant.
- The host adds bots in the lobby (Easy/Medium/Hard); `RoomData::add_bot` creates the `Member`.
- Tests in `bot.rs`: every tier plays full all-bot games to a winner across several seeds, plus
  setup/play move generation and exact-half discard.

---

## 10. Testing

- `cargo test` — 22 tests total.
  - 18 engine tests in `src/game/tests.rs`: board geometry & distribution, setup snake order,
    production (settlement=1/city=2), robber blocking, building costs/connection/distance rules,
    dev cards (Monopoly, one-per-turn, fresh-card rule), Longest Road (ring + break + ≥5
    threshold), bank & player trades, and the 10-VP win.
  - 4 bot tests in `src/bot.rs`: full all-bot games finish with a winner for every difficulty
    across several seeds, setup/play produce legal moves, and discard returns exactly half.
- `game::tests::auto_setup` is a reusable helper that plays the whole setup phase.
- Add a test whenever you add a rule. The engine is pure, so tests are fast and deterministic
  (seeded via `Rng64::new(seed)`).
- There are **no** server/HTTP tests yet. If you add them, `tower::ServiceExt::oneshot` against
  `handlers::router(app)` is the natural approach.
- **Browser testing:** see `docs/BROWSER-TESTING.md` — the step-by-step workflow for driving
  the running app with the opencode Chrome DevTools MCP (launch the server detached via
  `scripts/serve-bg.ps1`, two isolated browser contexts, SVG click gotcha, trade flow, mobile,
  and the checklist). Prefer this over curl when verifying UI/SSE behaviour.

---

## 11. Handy manual test sequence (curl)

```bash
# create (capture cookie + room code from the redirect)
curl -i -c /tmp/a.txt -d "name=Alice" http://localhost:8080/create
CODE=XXXX
# join as a second player
curl -c /tmp/b.txt -d "name=Bob" -d "code=$CODE" http://localhost:8080/join
# start, then inspect the board page
curl -b /tmp/a.txt -X POST http://localhost:8080/room/$CODE/start
curl -b /tmp/a.txt http://localhost:8080/room/$CODE | grep -o "place_vertex" | wc -l
# place first setup settlement, then roll once setup is done
curl -b /tmp/a.txt -d "action=place_vertex" -d "vertex=0" http://localhost:8080/room/$CODE/action
curl -b /tmp/a.txt -N http://localhost:8080/room/$CODE/events   # watch SSE
```

---

## 12. Definition of done for the original task

- [x] Small app (Rust + minimal frontend) — ✅ 1.1 MB binary, htmx/SSE, no DB
- [x] All base-game features — ✅ (list in `README.md` §Features)
- [x] Board changes every game — ✅ random terrain/numbers/ports
- [x] Online multiplayer for players who live apart — ✅ rooms + join codes + SSE
- [x] 2–4 players — ✅
- [x] Deployable via git / Docker — ✅ Dockerfile + compose + systemd unit
- [x] Bots for single-player — ✅ three difficulty tiers (§9)
- [x] Real-browser acceptance pass — ✅ desktop + mobile via `docs/BROWSER-TESTING.md`

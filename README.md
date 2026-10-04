# Catan

A small, self-hosted multiplayer **Settlers of Catan**. Rust backend, server-rendered
HTML + SVG board, live updates over SSE. No JavaScript framework, no database, no bloat.

> Picking up development? Read **[HANDOVER.md](HANDOVER.md)** — status, architecture,
> known gaps, and next steps.

- **2–4 players** per room, join by 4-letter room code
- **Fresh board every game** — terrain, ports, and the spiral start corner are rerolled each
  start; number tokens follow the official balanced spiral layout
- **Full base-game rules** (see below)
- Tiny footprint: single static binary, ~tens of MB of RAM, no external services

---

## Features

Complete base-game ruleset:

- Random 19-hex board with the standard resource mix (4 wood / 4 wheat / 4 sheep / 3 brick / 3 ore / 1 desert)
- Number tokens (2–12, no 7), nine ports (four × 3:1, five × 2:1)
- Snake-order initial placement (settlement + road)
- Dice production with a limited bank
- Roads, settlements, cities with correct costs and connectivity/distance rules
- The robber: roll a 7 → discard half (rounded down), move robber, steal a random card
- Player-to-player trades (propose / accept / decline / withdraw) and bank & maritime trading (4:1 / 3:1 / 2:1)
- All five development cards: Knight, Victory Point, Road Building, Year of Plenty, Monopoly
- Longest Road (≥5) and Largest Army (≥3), including correct ties and road-breaking by opponents
- Win at 10 victory points
- **AI bots** at three difficulty levels (Easy / Medium / Hard), added by the host in the lobby
  for solo or co-op games. Bots play setup, building, dev cards, trades, and the robber, and
  pace their turns so you can watch the game unfold.

Also included: room codes + join links, per-room snapshots persisted to disk (survives restarts),
reconnect via SSE, and a health endpoint.

Planned: optional expansion rules (Seafarers / Cities & Knights).

---

## Quick start (local)

Requires a Rust toolchain (1.85+).

```bash
cargo run
# open http://localhost:8080
```

Environment variables:

| Variable          | Default        | Meaning                          |
|-------------------|----------------|----------------------------------|
| `CATAN_ADDR`      | `0.0.0.0:8080` | Listen address                   |
| `CATAN_DATA_DIR`  | `data`         | Where room snapshots are written |

Run the tests:

```bash
cargo test
```

To test in a real browser (agent/harness workflow with the Chrome DevTools MCP),
see [`docs/BROWSER-TESTING.md`](docs/BROWSER-TESTING.md) and
[`scripts/serve-bg.ps1`](scripts/serve-bg.ps1).

---

## Deploy with Docker (recommended)

```bash
docker compose up -d --build
```

The image is a multi-stage build (Rust on Alpine → Alpine runtime), producing a small
container. Room data is kept in `./data` on the host; `/data` is a volume.

To update after pulling new code:

```bash
git pull
docker compose up -d --build
```

---

## Deploy as a plain binary (systemd)

Smallest possible footprint — no Docker.

```bash
# Build a static-ish release binary
cargo build --release

# On the server
sudo useradd --system --home /var/lib/catan --create-home catan
sudo mkdir -p /opt/catan && sudo cp target/release/catan /opt/catan/
sudo cp -r static /opt/catan/
sudo chown -R catan:catan /opt/catan
sudo cp deploy/catan.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now catan
```

The service binds to `127.0.0.1:8080`. Put a reverse proxy in front for HTTPS.

### Reverse proxy note

SSE needs proxying to be disabled and long timeouts. Caddy:

```
catan.example.com {
    reverse_proxy 127.0.0.1:8080 {
        flush_interval -1
    }
}
```

nginx:

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    proxy_buffering off;
    proxy_read_timeout 3600s;
}
```

---

## How to play online

1. One player creates a room and shares the 4-letter code (or the `/room/CODE` link).
2. Everyone joins with the code and a name — or the host uses **Add bot** to fill seats with
   Easy / Medium / Hard AI opponents.
3. The host presses **Start** (at least 2 players).
4. Place your two starting settlements + roads when prompted.
5. On your turn: **Roll**, then **build / trade / play dev cards**, then **End turn**. While it
   is your turn a guided bar at the bottom of the screen offers build, trade, dev-card, and
   end-turn actions, plus a compact view of your hand.

Everything is server-authoritative, so there is no way to cheat from the client.

---

## Architecture

```
src/
  main.rs        Axum server, background room sweeper
  handlers.rs    HTTP routes, cookies/sessions, SSE, action dispatch
  render.rs      maud HTML templates + inline SVG board + guided turn bar
  state.rs       Room registry, membership, on-disk snapshots, bot driver
  bot.rs         AI bot policy (Easy / Medium / Hard), one Action per call
  game/          Pure, synchronous, fully unit-tested rules engine
    board.rs       hex/vertex/edge geometry + random generation
    state.rs       GameState, players, phases
    actions.rs     Action enum + apply() + validation + legal moves
    resources.rs   resources, costs, development cards
    scoring.rs     longest road, largest army, victory points
    rng.rs         tiny serializable PRNG (exact snapshot resume)
static/          htmx + SSE extension (vendored) + guide/trade/timer scripts
```

- Player actions are `htmx` POSTs; the whole room is re-rendered and pushed to every
  player over SSE. Each connection renders its own private view (hand, controls, turn bar).
- The rules engine never touches async or IO, so it is trivially testable, and bots are just
  another source of `Action`s against `GameState` (see `src/bot.rs`).

---

## License

Provided as-is for personal use.

# UI Revamp — "offline board game" look (issue #9)

Tracking issue: <https://github.com/ukirdeomkar/Catan-Game/issues/9>

> **Status: analysis + plan only.** No behaviour or layout code is changed in this PR.
> Implementation lands on this branch (`ui/offline-board-revamp`) in follow-up commits.

## 1. What is being asked

The current in-game screen is the "colonist-style" flat/dark theme: solid colour hexes,
simple SVG pieces, small coloured resource chips. The request (from a player who prefers
the physical box) is to move the UI toward the look of the **offline board game**:

- **Hex tiles** with painted terrain art — forest (wood), hills (brick), pasture (sheep),
  fields (wheat), mountains (ore), desert.
- **Board background / ocean** — blue water with a coast, and port markers.
- **Cards** — resource cards and development cards with card-like art and a proper
  card face (not a flat colour chip).
- **Player pieces** — settlements, cities and roads that read like the wooden pieces.
- Must keep working across **all screen sizes**, be **mobile-first**, and stay a working
  **PWA** (install + offline shell).

Reference images in the issue are photos of the official 4th-edition components. They are
**style references, not assets to copy** — see §3.

## 2. Feasibility

**High.** The renderer was deliberately built so this is a styling/reskin exercise, not a
rewrite:

| Area | Verdict | Why |
|------|---------|-----|
| Board tiles → painted art | **Easy** | Hexes are already drawn as SVG `<polygon>` at known centres; art goes in via `<image>` + a per-hex `clipPath`. |
| Ocean / board background | **Easy** | `body.game` is already a blue gradient; swap for a water image/pattern layer behind the tiles. |
| Number tokens | **Trivial** | Already SVG circles + pips; only needs restyle. |
| Pieces (settlement/city/road) | **Medium** | Already SVG (`house_points`, road `<line>`); restyle to wood-like shapes. Best kept as SVG (crisp, recolourable, tiny). |
| Resource + dev cards | **Medium** | Card chrome already exists (`.rcard`, `.devrow`, `.devchip`); add art backgrounds + card-face layout. |
| Ports | **Easy** | Ports already drawn (`port_mark_svg`, `port_boat`); restyle badge/boat. |
| Responsive, mobile-first | **Low risk** | Shell is fixed/no-scroll and the board already letterboxes via `preserveAspectRatio="xMidYMid meet"`. |
| PWA | **Low risk** | Only needs the new image files added to the SW pre-cache list and a cache-version bump. |

**The real cost is asset production, not code.** The integration touchpoints are small and
centralised in `src/render.rs` + `static/app.css` (§4).

## 3. Asset sourcing & rights — decision needed before art is built

The reference images are **copyrighted Catan GmbH / Klaus Teuber artwork** and the game name
and its trade dress are trademarked. We must **not** ship the official scans/photos in the
repo or the deployed site (`playcatanou.duckdns.org`).

To get the requested look one of these is needed (pick one):

1. **Original art in the same style** (recommended) — commission or generate our own painted
   tiles/cards/ocean/pieces that evoke the board game without copying it. All files land in
   repo and are safe to ship.
2. **A permissively-licensed asset pack** (CC0/CC-BY) whose licence allows redistribution and
   commercial hosting; we record the licence + attribution in the asset folder.
3. **Code-drawn art** — richer inline SVG for tiles/cards/pieces (gradients, textures,
   stylised silhouettes). No external assets, smallest payload, but less "painted".

Everything below is written so it works with **any** of the three; the code paths differ only
in how `render.rs` fills a tile/card (image href vs inline SVG).

## 4. Technical approach (how, in this codebase)

Key files:

- `src/render.rs` — all HTML (maud) + inline SVG board. Integration points:
  - `terrain_fill()` (render.rs:14) — per-terrain solid colour → replaced by tile art.
  - `board_terrain()` (render.rs:114) — currently drops one icon per hex → becomes the
    full-bleed tile image clipped to the hex.
  - `board_frag()` (render.rs:1017) — the SVG. Add `<defs>` with a `<clipPath>` per hex
    shape + an ocean background layer; keep the existing highlight/overlay logic untouched.
  - `res_glyph()` (render.rs:98) / `res_icon_src()` (render.rs:87) — resource art.
  - `house_points()` (render.rs:1226), road `<line>` loop (render.rs:1101–1116) — pieces.
  - `port_mark_svg()`/`port_boat()` (render.rs:1260–1303) — port styling.
  - `resource_color()` (render.rs:35), `color_hex()` (render.rs:25) — palette tune-ups.
- `static/app.css` — `.board`, `.rcard`, `.devchip`, `.devrow`, `.reschip`, `.pstrip`,
  `.topbar`, `.dock`, `.sheet` themes; responsive breakpoints at 860/760/430px.
- `static/sw.js` — `SHELL` pre-cache list (sw.js:12) + `CACHE` version (sw.js:8).
- `static/branding/site.webmanifest` — `theme_color` / `background_color`.

### 4.1 Board
- Wrap the hex loop in `board_frag()`; draw one ocean rect/pattern for the viewBox before the
  hexes, then per hex: `polygon` (keep a thin dark border for tile gaps) + `<image>` with a
  `clip-path="url(#hex-{i})"` so art fills the hex exactly.
- Keep `hex_points()` geometry as-is; reuse it to build the clip paths.
- Recolour `terrain_fill()` to slightly darker edge tints so unpainted/partially-loaded
  states still read correctly, and keep number tokens/pieces on top with their own shadow.
- Number tokens: cream disc, pips, red for 6/8 (already close) — restyle to match the box.

### 4.2 Pieces
- Preferred: original SVG wood/plastic look (drop shadow + darker rim). Roads become short
  rounded "plank" bars along each edge; settlements/cities become roofed-house silhouettes.
- Player colours keep `color_hex()` (red/blue/orange/white) but tuned to the piece tones.

### 4.3 Cards
- Introduce a card art asset per resource and per dev type.
- `.rcard` (hand strip, trade picker, discard) gets: parchment face, art image area, name
  plate, count badge. Same markup stays; only CSS + `res_glyph()` art change.
- `.devrow`/`.devchip` (dev sheet + hand strip) get dev-card art thumbnails.
- Keep counts/titles/`data-*` attributes so `trade.js`/`discard.js` keep working unchanged.

### 4.4 Chrome + PWA
- Warm "tabletop" theme for topbar/player strip/dock/sheets while keeping the fixed no-scroll
  shell. Verify small-screen legibility of number tokens and chips (contrast/drop shadow).
- Add every new image to `sw.js` `SHELL`, bump `CACHE` (e.g. `catanou-v6`), and re-check
  installed/standalone layout per `fix/pwa-standalone-layout`.

### 4.5 Proposed asset manifest (names/format, to be produced)
```
static/board/ocean.webp            # tileable water texture (or code-drawn pattern)
static/board/tile-wood.webp        # per-terrain hex art, 
static/board/tile-brick.webp       #   square, bleed to hex bounds,
static/board/tile-wheat.webp       #   ~512px, WebP
static/board/tile-ore.webp
static/board/tile-sheep.webp
static/board/tile-desert.webp
static/cards/resource-wood.webp    # card face art (or full card images)
static/cards/resource-brick.webp
static/cards/resource-wheat.webp
static/cards/resource-ore.webp
static/cards/resource-sheep.webp
static/cards/dev-knight.webp
static/cards/dev-monopoly.webp
static/cards/dev-roadbuilding.webp
static/cards/dev-yearofplenty.webp
static/cards/dev-victorypoint.webp
```
Keep individual images small (WebP, ~512² tiles / ~300×420 cards) so first load and the
offline cache stay reasonable.

## 5. Phased plan (checklist for implementation)

- [ ] **Phase 0 — decide art source (§3)** and produce the manifest assets. Blocks Phases 1–3.
- [ ] **Phase 1 — board:** ocean layer + clipped per-hex tile art + token/robber restyle.
- [ ] **Phase 2 — pieces:** wood-styled settlement/city/road SVG.
- [ ] **Phase 3 — cards:** resource + dev card faces in hand strip, trade, discard, dev sheet.
- [ ] **Phase 4 — chrome + PWA:** topbar/strip/dock/sheet theme; `sw.js` list + cache bump.
- [ ] **Phase 5 — verify:** desktop + mobile browser pass (see `docs/BROWSER-TESTING.md`),
      installed-PWA/standalone pass, `cargo test` (engine untouched), and a load-size check.

## 6. Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| Shipping official copyrighted art | §3 — original/licensed art only; never commit issue reference photos. |
| Payload / first-load size grows | WebP, sensible dimensions, reuse; measure in Phase 5. |
| Painted tiles hurt legibility | Keep dark tile borders, shadows under tokens/pieces, test 6/8 contrast. |
| Offline cache misses new assets | Add to `sw.js` `SHELL` + bump `CACHE`; failures already tolerated per-file. |
| Regressions when solver/bot views change | Rendering is per-viewer; only presentational functions change. Confirm no `Action`/`GameState` edits. |
| Big-bang PR hard to review | Ship in the 4 phases above as separate commits on this branch. |

## 7. Testing

- `cargo test` — should stay green (no engine changes expected).
- Browser: run through setup → roll → build/trade/dev/robber → end turn at mobile and desktop
  sizes (per `docs/BROWSER-TESTING.md`), plus an installed/standalone PWA check.

## 8. Open questions

1. Art source: original commission, AI-generated, licensed pack, or code-drawn SVG? (§3)
2. Keep player-colour scheme (red/blue/orange/white) or move to the box's wood/plastic tones?
3. How far to take card art — full card faces, or just art thumbnails on the existing cards?
4. Keep the dark branded theme for non-game pages (home/lobby), or warm those up too?

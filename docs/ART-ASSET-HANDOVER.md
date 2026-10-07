# Art asset handover — offline board-game UI revamp

**For the art-generation session.** Generate and download every art asset needed for the
offline-board-game reskin, drop them in the repo at the paths below, and record provenance.

- Tracking issue: <https://github.com/ukirdeomkar/Catan-Game/issues/9>
- Analysis / integration plan: [`docs/UI-REVAMP.md`](UI-REVAMP.md)
- Plan branch / draft PR: `ui/offline-board-revamp` / #12

> **Style target:** the photos in issue #9 (official 4th-edition Catan components) are the
> **look** to match — painted realistic terrain, ocean board, card art, wooden pieces.
> Do **not** copy or embed those photos. Everything we ship must be **original art in that
> style** (see §7). The style prompt in §3 is designed to reproduce the *feel*, not the art.

---

## 1. Tooling reality check (read first)

Verified in the MCP session that produced this doc:

| Tool | Result | Use it? |
|------|--------|---------|
| `create-design` (Canva AI design generation) | **Works** — completed and returned a real design (`DAHXTuaWmSg`) | ✅ **Yes — generate each asset this way** |
| `generate-image` (standalone AI image generator) | Returned an `acs-quota-v1.…` token every time; `get-generate-image-job` rejected it ("Unexpected image job ID") | ⚠️ Try once; if it still fails, fall back to `create-design` |
| `export-design` | Standard Canva export (png / jpg / pdf / …) | ✅ Use to download |
| SVG export | **Not supported by Canva** | ❌ Author SVG assets separately (§5) |

**Therefore:** produce raster art with `create-design`, then export + download it with
`export-design`. Produce the vector pieces as SVG outside Canva (§5).

If `generate-image` *does* work in your session (it may work when the tool's UI widget is
shown), prefer it for tiles/cards — it yields a clean image with no design chrome. Use the
same prompts in §4.

---

## 2. Output contract (names + destinations)

Raster art goes in two new folders. Keep exact names — `render.rs` will look for these.

```
static/board/ocean.webp            # tileable ocean/water texture (board background)
static/board/tile-wood.webp        # forest  (wood)
static/board/tile-brick.webp       # hills   (brick)
static/board/tile-wheat.webp       # fields  (wheat)
static/board/tile-ore.webp         # mountains (ore)
static/board/tile-sheep.webp       # pasture (sheep)
static/board/tile-desert.webp      # desert  (no number token)

static/cards/resource-wood.webp
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

SVG art (hand-authored) goes in:

```
static/art/road.svg          # one road/plank shape, currentColor
static/art/settlement.svg    # house on a base
static/art/city.svg          # city (larger, with tower)
static/art/robber.svg        # dark robber token
static/art/port-boat.svg     # little boat for harbours
```

> The implementer may inline these SVGs in `render.rs` instead of shipping files — either is
> fine. What matters is that the **shapes/format** exist and are recolourable (pieces) and
> crisp at any size.

---

## 3. Global style prompt (prepend to every prompt)

Use this exact style block so all assets look like one set:

> **Top-down painted illustration in classic Settlers-of-Catan board-game style — warm
> hand-painted oil texture, rich saturated colours, soft golden sunlight, soft shadows,
> crisp focused detail, fills the frame edge to edge, no text, no labels, no border, no
> frame, no watermark.**

Then append the per-asset subject from §4. Keep the wording identical between assets except
for the subject — that is what keeps the set cohesive.

---

## 4. Asset manifest + prompts

**Aspect / size:** tiles square; ocean square (tileable); cards portrait 4:5.
Generate at the largest Canva gives, then downscale on export (§6).

### 4.1 Board tiles (square)

| File | Prompt subject (append to §3) |
|------|-------------------------------|
| `tile-wood.webp` | a dense pine-and-deciduous forest with a winding dirt path, deep greens with a few autumn-orange trees, sandy ochre ground at the edges |
| `tile-brick.webp` | rolling red-clay hills with terraced pits and small wooden mine carts, warm orange-red earth, sparse dry grass |
| `tile-wheat.webp` | golden wheat fields with bundled sheaves and a small fence, warm yellow-gold, a single tree at the corner |
| `tile-ore.webp` | a grey rocky mountain peak with purple-grey stone, snow at the top, a small mine entrance |
| `tile-sheep.webp` | a lush green pasture with grazing white sheep and small rocky outcrops, bright meadow green |
| `tile-desert.webp` | pale golden desert dunes, cracked dry earth, a couple of bare shrubs, no number token |

### 4.2 Ocean

| File | Prompt subject |
|------|----------------|
| `ocean.webp` | deep blue ocean water surface with gentle painted waves and lighter turquoise highlights, seamless tileable texture, top-down, no land |

### 4.3 Resource cards (portrait card face)

Base prompt: *a Catan resource card face, cream parchment border with a thin ornate gold
rule, the resource art centred, no text, no numbers.* Then:

| File | Art subject |
|------|-------------|
| `resource-wood.webp` | a stack of cut logs in a green forest clearing |
| `resource-brick.webp` | a pile of red clay bricks by a terracotta pit |
| `resource-wheat.webp` | golden wheat sheaves tied with twine |
| `resource-ore.webp` | a dark grey ore rock with mineral veins |
| `resource-sheep.webp` | a white sheep in a green meadow |

### 4.4 Development cards (portrait card face)

Same card-face base. Distinct framed look is fine (purple banner like the box).

| File | Art subject |
|------|-------------|
| `dev-knight.webp` | a medieval knight in armour on horseback, blue sky |
| `dev-monopoly.webp` | an old parchment scroll with a red wax seal |
| `dev-roadbuilding.webp` | two wooden road segments laid across green countryside |
| `dev-yearofplenty.webp` | a sunlit field of plenty with a rising sun |
| `dev-victorypoint.webp` | a painted round viking-style shield (red, yellow and blue bands with a silver boss) |

---

## 5. SVG assets (hand-authored — Canva cannot export SVG)

Author these as vector SVG so pieces stay crisp and can be tinted per player by swapping
`currentColor` / `fill`. Keep each in a 100×100 viewBox, flat-grouped, no embedded raster.

- **`road.svg`** — a short rounded "plank" bar (like a wooden road segment), with a slightly
  darker rim; designed to be drawn between two vertices.
- **`settlement.svg`** — a tiny house on a hex base: pentagon body + roof, a small chimney.
- **`city.svg`** — a bigger settlement with a second tower and a flag; same palette.
- **`robber.svg`** — the dark robber token (round base + hooded figure), reused from the
  current board art but upgraded.
- **`port-boat.svg`** — the little sailboat used on harbour markers.

If authoring by hand is slow, they can also be **code-drawn SVG** (the current code already
draws pieces this way in `src/render.rs` — `house_points()`, road `<line>`, `port_boat()`).
That is the fallback and is perfectly acceptable for v1.

---

## 6. Step-by-step Canva workflow (per raster asset)

1. **Generate.** Call `create-design` with a brief that is `§3 style` + `§4 subject`, and a
   format matching the aspect:
   - tiles / ocean → `Instagram Post` (square)
   - cards → a portrait format (e.g. `Instagram Post` portrait / `Presentation`)
   State **"image only, no text, no title, no logo"** in the brief.
2. **Find the design.** Use the returned `design.id` (from `get-create-design-async-job`, or
   `search-designs`). Confirm it is the right one (thumbnail/title).
3. **Check formats.** Call `get-export-formats` on the design; confirm `jpg`/`png`.
4. **Export.** Call `export-design` with `format: { type: "png" }` (use `png` so you can
   keep transparency if needed; jpg is fine for opaque tiles). Grab the download URL.
5. **Download + convert.** Save locally, then convert to **WebP** at target size:
   - tiles `512×512`, cards `~420×600`, ocean `1024×1024`.
   - Any image tool works (ImageMagick, `cwebp`, Squoosh). Keep each file **< ~120 KB**.
6. **Name exactly** per §2 and place in `static/board/` or `static/cards/`.

> Batch tip: generate all tiles in one sitting before moving to cards, and reuse the exact
> §3 wording, so the whole set stays stylistically consistent.

---

## 7. Licensing / provenance (must do)

- Art must be **original** (generated here) or from a licence that permits redistribution and
  self-hosting. Never commit the issue #9 photos or any official Catan scans.
- Record for each file: generator (Canva `create-design`), the prompt used, and the date.
  Append this to `docs/UI-REVAMP.md` §3 or a new `static/art/PROVENANCE.md`.
- Do not use the trademarked name/logo as art.

---

## 8. Definition of done (hand back to the integration session)

- [ ] All 6 tiles + ocean present and visually consistent (compare side by side).
- [ ] All 5 resource cards + 5 dev cards present, same border/card style.
- [ ] 5 SVG pieces present (or code-drawn equivalents agreed).
- [ ] Files named/destination exactly per §2, each within size budget.
- [ ] Provenance recorded per §7.
- [ ] A quick contact sheet / screenshot of the full set attached to the PR comment.

## 9. What happens after art lands (integration, not this session)

1. `src/render.rs`: replace `terrain_fill()`/`board_terrain()` with clipped `<image>` tiles;
   add an ocean layer in `board_frag()`; point `res_glyph()`/dev art at the card files; swap
   pieces to the SVG shapes.
2. `static/app.css`: card/board/chrome theming.
3. `static/sw.js`: add the new files to `SHELL`, bump `CACHE` (e.g. `catanou-v6`).
4. Verify desktop + mobile + installed PWA (see `docs/BROWSER-TESTING.md`).

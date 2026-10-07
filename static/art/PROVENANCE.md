# Art provenance

All raster art in this repo is **original, AI-generated** (no official Catan scans/photos).

- **Generator:** Canva `create-design` (agent connector), exported PNG via `export-design`.
- **Date:** 2026-10-07
- **Style block** (prepended to every subject below):

  > Top-down painted illustration in classic Settlers-of-Catan board-game style — warm
  > hand-painted oil texture, rich saturated colours, soft golden sunlight, soft shadows,
  > crisp focused detail, fills the frame edge to edge, no text, no labels, no border, no
  > frame, no watermark.

## Board tiles — `static/board/*.webp`

| File | Subject appended to the style block |
|------|-------------------------------------|
| `tile-wood.webp` | a dense pine-and-deciduous forest with a winding dirt path, deep greens with a few autumn-orange trees, sandy ochre ground at the edges |
| `tile-brick.webp` | rolling red-clay hills with terraced pits and small wooden mine carts, warm orange-red earth, sparse dry grass |
| `tile-wheat.webp` | golden wheat fields with bundled sheaves and a small fence, warm yellow-gold, a single tree at the corner |
| `tile-ore.webp` | a grey rocky mountain peak with purple-grey stone, snow at the top, a small mine entrance |
| `tile-sheep.webp` | a lush green pasture with grazing white sheep and small rocky outcrops, bright meadow green |
| `tile-desert.webp` | pale golden desert dunes, cracked dry earth, a couple of bare shrubs, no number token |
| `ocean.webp` | flat calm still ocean, solid vivid azure blue water surface (≈ #019DDC), no waves, no ripples, no foam, minimal texture, seamless tileable, top-down, no land |

## Resource + development cards — `static/cards/*.webp`

Base: *a Catan resource card face, cream parchment border with a thin ornate gold rule, the
resource art centred, no text, no numbers.* Plus:

| File | Art subject |
|------|-------------|
| `resource-wood.webp` | a stack of cut logs in a green forest clearing |
| `resource-brick.webp` | a pile of red clay bricks by a terracotta pit |
| `resource-wheat.webp` | golden wheat sheaves tied with twine |
| `resource-ore.webp` | a dark grey ore rock with mineral veins |
| `resource-sheep.webp` | a white sheep in a green meadow |
| `dev-knight.webp` | a medieval knight in armour on horseback, blue sky |
| `dev-monopoly.webp` | an old parchment scroll with a red wax seal |
| `dev-roadbuilding.webp` | two wooden road segments laid across green countryside |
| `dev-yearofplenty.webp` | a sunlit field of plenty with a rising sun |
| `dev-victorypoint.webp` | a painted round viking-style shield (red, yellow and blue bands with a silver boss) |

## Harbour pieces — `static/art/*.webp` (transparent)

Generated with `create-design` on a plain white background, then the white keyed out to
transparency off-line (Pillow; Canva `remove-background` is the equivalent but its download is
signature-locked). Subjects:

| File | Subject |
|------|---------|
| `port-boat.webp` | a small wooden boat with one very large cream linen square sail fully unfurled and **billowing with wind** (soft convex bow, folds, curved edges), facing the viewer, high three-quarter top-down, ropes only along the sail edges so the middle stays clear for the ratio/icon overlay |
| `port-bridge.webp` | a short weathered-timber plank jetty / pier walkway with round support posts and a tied rope, seen from directly above |

## Island coast — `static/board/*.webp`

| File | Subject | Notes |
|------|---------|-------|
| `beach.webp` | seamless sandy beach texture — fine warm pale-golden sand with faint wind ripples, no water / foam / objects | opaque, 1024². Used for the per-tile sand border and the island's coast ring. |

> A `shore-foam.webp` was also generated but is **not shipped** — the coast uses a
> stroke-based sand ring instead, so the file was removed.

## SVG pieces — `static/art/*.svg`

Hand-authored / code-drawn vector shapes (road, settlement, city, robber). Not AI-generated.

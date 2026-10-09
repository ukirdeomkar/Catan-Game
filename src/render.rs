use crate::game::board::{Board, Building, PortKind, Terrain};
use crate::game::resources::{
    ALL_RESOURCES, Bundle, COST_CITY, COST_DEV, COST_ROAD, COST_SETTLEMENT, DevCard, Resource,
    ResourceHand,
};
use crate::game::state::{BotLevel, Color, GameState, Phase, PlayerId, TradeResponse};
use crate::state::{Layout, RoomData, ViewMode};
use maud::{DOCTYPE, Markup, html};

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

fn color_hex(c: Color) -> &'static str {
    match c {
        Color::Red => "#d2453a",
        Color::Blue => "#2f7fd6",
        Color::Orange => "#e2892a",
        Color::White => "#eef1f4",
    }
}

/// Per-resource accent colour, used on cards and chips.
fn resource_color(r: Resource) -> &'static str {
    match r {
        Resource::Wood => "#3f8f4f",
        Resource::Brick => "#bd5a30",
        Resource::Wheat => "#d9a72c",
        Resource::Ore => "#7d8794",
        Resource::Sheep => "#8dc35a",
    }
}

// ---------------------------------------------------------------------------
// Icons — Lucide-style chrome icons (MIT) + custom Catan resource glyphs.
// Everything is inline SVG, so there is no font/emoji dependency and it stays
// identical across platforms and offline.
// ---------------------------------------------------------------------------

/// A stroke-based UI icon by name.
fn ic(name: &str) -> Markup {
    let inner: &str = match name {
        "menu" => r#"<path d="M4 6h16"/><path d="M4 12h16"/><path d="M4 18h16"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="9.5"/><path d="M12 16v-4"/><path d="M12 8h.01"/>"#,
        "settings" => r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>"#,
        "dice" => r#"<rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8.5" cy="8.5" r="1.3" fill="currentColor" stroke="none"/><circle cx="15.5" cy="8.5" r="1.3" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.3" fill="currentColor" stroke="none"/><circle cx="8.5" cy="15.5" r="1.3" fill="currentColor" stroke="none"/><circle cx="15.5" cy="15.5" r="1.3" fill="currentColor" stroke="none"/>"#,
        "build" => r#"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>"#,
        "trade" => r#"<path d="M8 3 4 7l4 4"/><path d="M4 7h16"/><path d="m16 21 4-4-4-4"/><path d="M20 17H4"/>"#,
        "dev" => r#"<path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z"/><path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"/><path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65"/>"#,
        "check" => r#"<path d="M20 6 9 17l-5-5"/>"#,
        "x" => r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
        "log" => r#"<path d="M8 6h13"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M3 6h.01"/><path d="M3 12h.01"/><path d="M3 18h.01"/>"#,
        "trophy" => r#"<path d="M6 9H4.5a2.5 2.5 0 0 1 0-5H6"/><path d="M18 9h1.5a2.5 2.5 0 0 0 0-5H18"/><path d="M4 22h16"/><path d="M10 14.66V17c0 .55-.47.98-.97 1.21C7.85 18.75 7 20.24 7 22"/><path d="M14 14.66V17c0 .55.47.98.97 1.21C16.15 18.75 17 20.24 17 22"/><path d="M18 2H6v7a6 6 0 0 0 12 0V2z"/>"#,
        "shield" => r#"<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>"#,
        "army" => r#"<path d="M14.5 17.5 3 6V3h3l11.5 11.5"/><path d="m13 19 6-6"/><path d="m16 16 4 4"/><path d="m19 21 2-2"/><path d="M6 3 3 6"/>"#,
        "users" => r#"<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>"#,
        "road" => r#"<circle cx="6" cy="19" r="2.6"/><path d="M9 19h8.5a3.5 3.5 0 0 0 0-7h-11a3.5 3.5 0 0 1 0-7H15"/><circle cx="18" cy="5" r="2.6"/>"#,
        "home" => r#"<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M9 22V12h6v10"/>"#,
        "city" => r#"<path d="M6 22V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v18z"/><path d="M6 12H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2"/><path d="M18 9h2a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-2"/><path d="M10 6h4"/><path d="M10 10h4"/><path d="M10 14h4"/><path d="M10 18h4"/>"#,
        "bank" => r#"<path d="M3 22h18"/><path d="M6 18v-7"/><path d="M10 18v-7"/><path d="M14 18v-7"/><path d="M18 18v-7"/><path d="m12 2 9 5H3z"/>"#,
        "play" => r#"<path d="m9 18 6-6-6-6"/>"#,
        "offer" => r#"<path d="M14.54 21.69a.5.5 0 0 0 .94-.03l6.5-19a.5.5 0 0 0-.64-.63l-19 6.5a.5.5 0 0 0-.02.94l7.93 3.18a2 2 0 0 1 1.11 1.11z"/><path d="m21.85 2.15-10.94 10.94"/>"#,
        "swap" => r#"<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>"#,
        "trash" => r#"<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"#,
        "person" => r#"<circle cx="12" cy="8" r="4.4"/><path d="M4.5 21a7.5 7.5 0 0 1 15 0"/>"#,
        "palette" => r#"<circle cx="13.5" cy="6.5" r=".6" fill="currentColor" stroke="none"/><circle cx="17.5" cy="10.5" r=".6" fill="currentColor" stroke="none"/><circle cx="8.5" cy="7.5" r=".6" fill="currentColor" stroke="none"/><circle cx="6.5" cy="12.5" r=".6" fill="currentColor" stroke="none"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"/>"#,
        "angry" => r#"<circle cx="12" cy="12" r="9.5"/><path d="M16 16s-1.5-2-4-2-4 2-4 2"/><path d="M7.5 8 10 9"/><path d="m14 9 2.5-1"/><path d="M9 10h.01"/><path d="M15 10h.01"/>"#,
        _ => "",
    };
    let s = format!(
        r#"<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{inner}</svg>"#
    );
    maud::PreEscaped(s)
}

/// Vendored resource icons (Icons8 PNGs under /static/icons).
fn res_icon_src(r: Resource) -> &'static str {
    match r {
        Resource::Wood => "/static/icons/wood.png",
        Resource::Brick => "/static/icons/brick.png",
        Resource::Wheat => "/static/icons/wheat.png",
        Resource::Ore => "/static/icons/ore.png",
        Resource::Sheep => "/static/icons/sheep.png",
    }
}

/// A resource icon for HTML contexts (cards, chips, costs, stat rows).
fn res_glyph(r: Resource) -> Markup {
    html! {
        img.glyph src=(res_icon_src(r)) alt=(r.name()) draggable="false";
    }
}

/// A resource icon as an SVG `<image>` (for the board hexes and port badges).
fn res_image(r: Resource, x: f64, y: f64, size: f64) -> Markup {
    html! {
        image href=(res_icon_src(r)) x=(format!("{x:.3}")) y=(format!("{y:.3}"))
            width=(format!("{size:.3}")) height=(format!("{size:.3}")) preserveAspectRatio="xMidYMid meet" {}
    }
}

/// Painted tile art per terrain (see `static/art/PROVENANCE.md`). Full-bleed
/// square art, cropped to the hexagon at render time.
fn terrain_tile(t: Terrain) -> &'static str {
    match t {
        Terrain::Wood => "/static/board/tile-wood.webp",
        Terrain::Brick => "/static/board/tile-brick.webp",
        Terrain::Wheat => "/static/board/tile-wheat.webp",
        Terrain::Ore => "/static/board/tile-ore.webp",
        Terrain::Sheep => "/static/board/tile-sheep.webp",
        Terrain::Desert => "/static/board/tile-desert.webp",
    }
}

/// Classic flat terrain colour (the pre-revamp look).
fn terrain_fill(t: Terrain) -> &'static str {
    match t {
        Terrain::Wood => "#3f8f4f",
        Terrain::Brick => "#bd5a30",
        Terrain::Wheat => "#d9a72c",
        Terrain::Ore => "#7d8794",
        Terrain::Sheep => "#8dc35a",
        Terrain::Desert => "#e6d3a3",
    }
}

/// Classic tile glyph: one vendored resource icon (desert keeps a small cactus).
fn flat_tile_icon(t: Terrain, cx: f64, cy: f64) -> Markup {
    match t.resource() {
        Some(r) => res_image(r, cx - 0.36, cy - 0.36, 0.72),
        None => maud::PreEscaped(format!(
            r##"<g transform="translate({cx:.3} {cy:.3}) scale(0.028) translate(-12 -12)" fill="#8a6d3b" opacity="0.9"><rect x="10.6" y="3.5" width="3" height="17" rx="1.5"/><rect x="6.2" y="7.5" width="3" height="6" rx="1.5"/><rect x="6.2" y="11.5" width="6" height="3" rx="1.5"/><rect x="15" y="5.5" width="3" height="7" rx="1.5"/><rect x="12" y="10" width="6" height="3" rx="1.5"/></g>"##
        )),
    }
}

/// Painted card art per resource.
fn res_card_art(r: Resource) -> &'static str {
    match r {
        Resource::Wood => "/static/cards/resource-wood.webp",
        Resource::Brick => "/static/cards/resource-brick.webp",
        Resource::Wheat => "/static/cards/resource-wheat.webp",
        Resource::Ore => "/static/cards/resource-ore.webp",
        Resource::Sheep => "/static/cards/resource-sheep.webp",
    }
}

/// Painted art per development card.
fn dev_card_art(c: DevCard) -> &'static str {
    match c {
        DevCard::Knight => "/static/cards/dev-knight.webp",
        DevCard::Monopoly => "/static/cards/dev-monopoly.webp",
        DevCard::RoadBuilding => "/static/cards/dev-roadbuilding.webp",
        DevCard::YearOfPlenty => "/static/cards/dev-yearofplenty.webp",
        DevCard::VictoryPoint => "/static/cards/dev-victorypoint.webp",
    }
}

/// CSS vars for a resource card: accent colour, plus card art on the board layout.
fn res_card_style(r: Resource, layout: Layout) -> String {
    if layout == Layout::Board {
        format!("--rc:{};--card:url({})", resource_color(r), res_card_art(r))
    } else {
        format!("--rc:{}", resource_color(r))
    }
}

// Hand-authored piece art, embedded at build time so it can be inlined and
// tinted via `currentColor` (an external <image> cannot inherit it).
const SETTLEMENT_SVG: &str = include_str!("../static/art/settlement.svg");
const CITY_SVG: &str = include_str!("../static/art/city.svg");
const ROAD_SVG: &str = include_str!("../static/art/road.svg");
const ROBBER_SVG: &str = include_str!("../static/art/robber.svg");

/// Inner markup of an embedded piece SVG, without its outer `<svg>` wrapper.
fn svg_inner(src: &'static str) -> &'static str {
    let start = src.find('>').map(|i| i + 1).unwrap_or(0);
    let end = src.rfind("</svg>").unwrap_or(src.len());
    &src[start..end]
}

/// Place a 100x100 piece SVG at `(cx,cy)`, scaled (100 units = `1/scale`) and
/// tinted to `color` (picked up by `currentColor` inside the art).
fn piece(src: &'static str, cx: f64, cy: f64, scale: f64, color: &str) -> Markup {
    maud::PreEscaped(format!(
        r##"<g transform="translate({cx:.3} {cy:.3}) scale({scale:.4}) translate(-50 -50)" color="{color}" fill="none">{}</g>"##,
        svg_inner(src)
    ))
}

/// Place a road plank along the edge (ax,ay)-(bx,by), tinted to `color`.
fn road_piece(ax: f64, ay: f64, bx: f64, by: f64, color: &str) -> Markup {
    let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
    let deg = (by - ay).atan2(bx - ax).to_degrees();
    maud::PreEscaped(format!(
        r##"<g transform="translate({mx:.3} {my:.3}) rotate({deg:.2}) scale(0.0112 0.0052) translate(-50 -50)" color="{color}" fill="none">{}</g>"##,
        svg_inner(ROAD_SVG)
    ))
}

/// The robber: painted token art on the board, or the classic drawn token.
fn board_robber(cx: f64, cy: f64, board: bool) -> Markup {
    if board {
        return piece(ROBBER_SVG, cx, cy, 0.0075, "#2b2b30");
    }
    maud::PreEscaped(format!(
        r##"<circle cx="{cx:.3}" cy="{cy:.3}" r="0.21" fill="#2b2b30" stroke="#c79a4e" stroke-width="0.03"/><g transform="translate({cx:.3} {cy:.3}) scale(0.0135) translate(-12 -12)"><path d="M12 3c-3.6 0-6.2 2.8-6.2 6.6 0 3 1.6 5.3 3.9 6.4L9 21.5h6l-.7-5.5c2.3-1.1 3.9-3.4 3.9-6.4C18.2 5.8 15.6 3 12 3z" fill="#d7cfbf"/><ellipse cx="12" cy="9.6" rx="3.1" ry="3.5" fill="#141316"/></g>"##
    ))
}

// ---------------------------------------------------------------------------
// Shells
// ---------------------------------------------------------------------------

fn head_common(title: &str) -> Markup {
    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no";
            meta name="theme-color" content="#1470a8";
            meta name="description" content="The classic game of Catan, now online to play with your friends.";
            meta property="og:title" content="Play Catanou";
            meta property="og:description" content="Create a room, share the code, and play Settlers of Catan with friends.";
            meta property="og:image" content="/static/branding/og.jpg";
            meta property="og:type" content="website";
            title { (title) }
            link rel="icon" href="/static/branding/favicon.ico" sizes="any";
            link rel="icon" type="image/png" sizes="32x32" href="/static/branding/favicon-32.png";
            link rel="icon" type="image/png" sizes="16x16" href="/static/branding/favicon-16.png";
            link rel="apple-touch-icon" href="/static/branding/apple-touch-icon.png";
            link rel="manifest" href="/static/branding/site.webmanifest";
            link rel="preload" as="font" type="font/woff2" href="/static/fonts/nunito-latin.woff2" crossorigin;
            link rel="stylesheet" href="/static/app.css";
            meta name="apple-mobile-web-app-capable" content="yes";
            meta name="mobile-web-app-capable" content="yes";
            meta name="apple-mobile-web-app-title" content="Play Catanou";
            meta name="apple-mobile-web-app-status-bar-style" content="black-translucent";
            script src="/static/htmx.min.js" defer {}
            script src="/static/sse.js" defer {}
            script src="/static/timer.js" defer {}
            script src="/static/trade.js" defer {}
            script src="/static/ui.js" defer {}
            script src="/static/discard.js" defer {}
            script src="/static/audio.js" defer {}
            script src="/static/pwa.js" defer {}
        }
    }
}

fn shell(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            (head_common(title))
            body {
                header.site {
                    div.wrap {
                        a.brand href="/" {
                            img src="/static/branding/logo-128.webp" alt="Play Catanou logo" width="42" height="42";
                            span.bt { "Play Catanou" }
                        }
                        button #audio-toggle type="button" title="Toggle sound" aria-label="Toggle sound" { "🔊" }
                    }
                }
                (body)
                footer.site {
                    div.wrap {
                        span.fnote {
                            "Made with ♥ by "
                            a href="https://github.com/ukirdeomkar" target="_blank" rel="noopener" { "Omkar" }
                        }
                    }
                }
            }
        }
    }
}

/// Full-viewport shell for the in-game screen: no header/footer, no scroll.
fn shell_game(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            (head_common(title))
            body.game { (body) }
        }
    }
}

// ---------------------------------------------------------------------------
// Public pages
// ---------------------------------------------------------------------------

pub fn home_page(error: Option<&str>) -> Markup {
    shell("Play Catanou", html! {
        div.wrap.page {
            div.hero {
                p.muted { "The classic game of Catan, online with friends." }
            }
            @if let Some(e) = error { div.card #toasts { (e) } }
            div.card.howto-cta {
                h3 { (ic("info")) "New to Catan?" }
                p.muted style="margin:0 0 10px" { "Read the quick guide before your first game." }
                a.btn.sec href="/how-to-play" { (ic("info")) "How to play" }
            }
            div.card {
                h3 { (ic("play")) "Create a new game" }
                form method="post" action="/create" {
                    label.field {
                        span { "Your name" }
                        input type="text" name="name" placeholder="Max 5 characters" maxlength="5";
                    }
                    div.formrow {
                        label.field {
                            span { "Turn length" }
                            select name="turn_seconds" {
                                option value="60" { "1 min" }
                                option value="180" selected { "3 min" }
                                option value="300" { "5 min" }
                                option value="600" { "10 min" }
                            }
                        }
                        button.btn type="submit" { "Create room" }
                    }
                }
            }
            div.card {
                h3 { (ic("users")) "Join a game" }
                form method="post" action="/join" {
                    label.field {
                        span { "Room code" }
                        input type="text" name="code" placeholder="e.g. ABC123" maxlength="6" style="text-transform:uppercase";
                    }
                    label.field {
                        span { "Your name" }
                        input type="text" name="name" placeholder="Max 5 characters" maxlength="5";
                    }
                    button.btn.sec type="submit" { "Join room" }
                }
            }
        }
    })
}

/// Numbered heading for one how-to-play beat card.
fn beat_head(n: u8, title: &str) -> Markup {
    html! {
        div.beat-head {
            span.beat-num { (n) }
            h3 { (title) }
        }
    }
}

/// Static, scrollable "How to play" reference — one card per design beat 0–6.
/// Reachable from the home screen and the in-game menu; needs no live game.
/// Build costs come from the engine cost constants, never hard-coded copy.
pub fn how_to_play_page() -> Markup {
    shell("How to play", html! {
        div.wrap.page.howto {
            div.hero {
                h1 { "How to play" }
                p.muted { "A quick reference for Catan — from the first roll to 10 victory points." }
            }

            div.card.beat {
                (beat_head(0, "Goal"))
                p {
                    "Settle the island of Catan. The first player to reach "
                    b { "10 victory points" } " wins."
                }
                p.muted.small {
                    "Points come from settlements and cities, plus 2 each for Longest Road "
                    "(5+ connected roads) and Largest Army (3+ knights played)."
                }
            }

            div.card.beat {
                (beat_head(1, "The board"))
                p { "Each hex is a terrain that produces one resource:" }
                div.beat-chips {
                    @for r in ALL_RESOURCES {
                        span.chip { (res_glyph(r)) (r.name()) }
                    }
                }
                p {
                    "Every non-desert hex has a " b { "number token" } " (2–12). "
                    "The small dots under the number are its " b { "pips" }
                    " — more pips means that number is rolled more often."
                }
                p.muted.small {
                    "When the dice total matches a hex's number, everyone with a building on that hex collects its resource."
                }
            }

            div.card.beat {
                (beat_head(2, "Your turn"))
                p {
                    "On your turn: roll the dice, collect resources, then build, trade, "
                    "or buy cards. Tap " (ic("dice")) " Roll, and finish with "
                    (ic("play")) " End turn."
                }
                p.muted.small {
                    "You may build and trade before or after rolling, but you must roll before ending your turn."
                }
            }

            div.card.beat {
                (beat_head(3, "Build & costs"))
                p {
                    "Spend resources to build. Tap " (ic("build"))
                    " Build, then place your piece on a highlighted spot or edge."
                }
                (build_guide())
            }

            div.card.beat {
                (beat_head(4, "Resources & trading"))
                p {
                    "Hold resource cards in your hand. Trade with other players from the "
                    (ic("trade")) " Trade sheet, or exchange with the bank at a fixed rate:"
                }
                ul.beat-list {
                    li { "4 of one resource → 1 of any resource (no port)" }
                    li { "3 of one resource → 1 of any resource (generic 3:1 port)" }
                    li { "2 of one resource → 1 of that resource (matching 2:1 port)" }
                }
            }

            div.card.beat {
                (beat_head(5, "Development cards"))
                p {
                    "Buy one for " (cost_span(COST_DEV))
                    " and play at most one per turn (a hidden Victory Point card can always be revealed)."
                }
                ul.beat-list {
                    @for c in [DevCard::Knight, DevCard::RoadBuilding, DevCard::YearOfPlenty, DevCard::Monopoly, DevCard::VictoryPoint] {
                        li { b { (c.name()) } " — " (c.description()) }
                    }
                }
            }

            div.card.beat {
                (beat_head(6, "Winning"))
                p {
                    "The " (ic("trophy")) " badge on each player shows their public victory points. "
                    "Grow your score with upgrades and bonuses — and win at "
                    b { "10 victory points" } "."
                }
            }

            p.howto-back { a.btn.sec href="/" { "Back to home" } }
        }
    })
}

pub fn lobby_page(code: &str, data: &RoomData, token: &str) -> Markup {
    shell("Lobby", html! {
        div.wrap.page {
            div.hero {
                h1 { "Lobby" }
                p.muted { "Share this room code with friends" }
            }
            div.card.codecard {
                span.codelabel { "Room code" }
                code.roomcode { (code) }
            }
            div hx-ext="sse" sse-connect=(format!("/room/{code}/events")) {
                div #lobbybox sse-swap="lobby" { (lobby_frag(code, data, token)) }
            }
            script src="/static/lobby.js" defer {}
        }
    })
}

pub fn lobby_frag(code: &str, data: &RoomData, token: &str) -> Markup {
    let is_host = data.is_host(token);
    html! {
        div.card {
            h3 { (ic("users")) "Players (" (data.members.len()) "/4)" }
            div.players {
                @for m in &data.members {
                    div.pcard {
                        span.pdot style=(format!("background:{}", color_hex(m.color))) {}
                        div.pname { (m.name) }
                        @if data.is_host(&m.token) || m.is_bot {
                            div.pbadges {
                                @if data.is_host(&m.token) { span { "host" } }
                                @if m.is_bot { span { "bot " (m.level.label()) } }
                            }
                        }
                    }
                }
            }
        }
        @if is_host {
            div.card {
                h3 { (ic("settings")) "Host controls" }
                form method="post" action=(format!("/room/{code}/add_bot")) {
                    div.formrow {
                        label.field {
                            span { "Bot difficulty" }
                            select name="level" {
                                @for level in BotLevel::ALL {
                                    option value=(level.slug()) selected[level == BotLevel::default()] { (level.label()) }
                                }
                            }
                        }
                        button.btn.sec type="submit" disabled[data.members.len() >= 4] { "Add bot" }
                    }
                }
                form method="post" action=(format!("/room/{code}/start")) {
                    button.btn type="submit" disabled[!data.can_start()] {
                        "Start game (" (data.members.len()) "/4)"
                    }
                }
                @if data.members.len() < 2 {
                    p.muted style="margin:10px 0 0" { "Need at least 2 players to start." }
                }
            }
        } @else {
            div.card {
                p.muted style="margin:0" { "Waiting for the host to start…" }
            }
        }
    }
}

pub fn error_page(msg: &str) -> Markup {
    shell("Catan", html! {
        div.wrap.page {
            div.card {
                h1 { "Oops" }
                p { (msg) }
                p { a href="/" { "Back to home" } }
            }
        }
    })
}

pub fn join_page(code: &str) -> Markup {
    shell("Join Catan", html! {
        div.wrap.page {
            div.card {
                h1 { "Join game " code { (code) } }
                form method="post" action=(format!("/room/{code}/join")) {
                    label.field {
                        span { "Your name" }
                        input type="text" name="name" placeholder="Max 5 characters" maxlength="5";
                    }
                    button.btn type="submit" { "Join" }
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Game page + live fragments
// ---------------------------------------------------------------------------

pub struct Fragments {
    pub status: Markup,
    pub board: Markup,
    pub players: Markup,
    pub hand: Markup,
    pub controls: Markup,
    pub trades: Markup,
    pub log: Markup,
    pub turn: Markup,
}

pub fn fragments(game: &GameState, data: &RoomData, viewer: Option<PlayerId>) -> Fragments {
    let layout = viewer.map(|v| member_layout(data, v)).unwrap_or_default();
    Fragments {
        status: status_frag(game, data, viewer),
        board: board_frag(game, data, viewer),
        players: players_frag(game, data, viewer),
        hand: viewer
            .map(|v| hand_frag(game, &data.code, v, layout))
            .unwrap_or_default(),
        controls: controls_frag(game, data, viewer),
        trades: trades_frag(game, &data.code, viewer),
        log: log_frag(game),
        turn: turn_frag(game, data, viewer),
    }
}

/// The visual layout a viewer has chosen (defaults to the painted board).
fn member_layout(data: &RoomData, v: PlayerId) -> Layout {
    data.members.get(v).map(|m| m.layout).unwrap_or_default()
}

pub fn game_page(code: &str, data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    let game = data.game.as_ref().expect("started game");
    let f = fragments(game, data, viewer);
    shell_game("Catan game", html! {
        div #app hx-ext="sse" sse-connect=(format!("/room/{code}/events")) {
            div #toasts.toasts {}
            div #notif.notif {}
            div.pwa-titlebar {
                img src="/static/branding/logo-128.webp" alt="Play Catanou" width="20" height="20";
                span { "Play Catanou" }
            }
            div #status.topbar sse-swap="status" { (f.status) }
            div #players.pstrip sse-swap="players" { (f.players) }
            div #board.board sse-swap="board" { (f.board) }
            div #controls.ctl sse-swap="controls" { (f.controls) }
            div #trades.trades sse-swap="trades" { (f.trades) }
            div #turn.dockwrap sse-swap="turn" { (f.turn) }
            div #hand.handstrip sse-swap="hand" { (f.hand) }
            footer.gamefoot {
                "Made with ♥ by "
                a href="https://github.com/ukirdeomkar" target="_blank" rel="noopener" { "Omkar" }
            }
            (menu_sheet(code, data, game, viewer))
            @if let Some(v) = viewer {
                (trade_modal(game, data, v))
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Top bar: menu + bank + your dev/VP
// ---------------------------------------------------------------------------

fn status_frag(game: &GameState, data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    let layout = viewer.map(|v| member_layout(data, v)).unwrap_or_default();
    let next = if layout == Layout::Board { "classic" } else { "board" };
    let layout_url = format!("/room/{}/layout", data.code);
    let next_vals = format!(r#"{{"layout":"{next}"}}"#);
    html! {
        button.tb-btn type="button" data-sheet-open="menu" title="Menu & info" aria-label="Menu" { (ic("menu")) }
        div.bank title="Resources left in the bank" {
            span.bank-ic title="Bank" { (ic("bank")) }
            @for r in ALL_RESOURCES {
                div.reschip style=(format!("--rc:{}", resource_color(r))) title=(r.name()) {
                    span.rc-glyphbox { (res_glyph(r)) }
                    span.rc-num { (game.bank.get(r)) }
                }
            }
        }
        div.tb-right {
            button.tb-btn.round type="button" title="Switch board style" aria-label="Switch board style"
                hx-post=(layout_url) hx-vals=(next_vals) { (ic("palette")) }
            button #audio-toggle type="button" title="Toggle sound" aria-label="Toggle sound" { "🔊" }
        }
    }
}

// ---------------------------------------------------------------------------
// Player strip (viewer rotated to the left — colonist style)
// ---------------------------------------------------------------------------

fn players_frag(game: &GameState, data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    let n = game.players.len();
    let order: Vec<PlayerId> = match viewer {
        Some(v) => (0..n).map(|i| (v + i) % n).collect(),
        None => (0..n).collect(),
    };
    html! {
        @for pid in order {
            @let p = &game.players[pid];
            @let is_me = Some(pid) == viewer;
            @let is_turn = game.current == pid;
            @let is_light = matches!(p.color, Color::White);
            @let is_bot = data.members.get(pid).map(|m| m.is_bot).unwrap_or(false);
            @let shown_vp = if game.is_over() {
                game.total_victory_points(pid)
            } else {
                game.public_victory_points(pid)
            };
            div.pcard.me[is_me].turn[is_turn].light[is_light] style=(format!("--pc:{}", color_hex(p.color))) {
                div.phead {
                    span.pvp.revealed[game.is_over()] title="Victory points" { (ic("trophy")) (shown_vp) }
                    span.pname title=(p.name.as_str()) { (p.name) }
                }
                div.prow {
                    span.stat title="Resource cards" { (res_glyph(Resource::Wood)) (p.resources.total()) }
                    span.stat title="Development cards" { (ic("dev")) (p.all_dev_cards().count()) }
                    span.stat title="Knights played" { (ic("shield")) (p.played_knights) }
                    @if game.longest_road == Some(pid) { span.stat title="Longest Road" { (ic("road")) } }
                    @if game.largest_army == Some(pid) { span.stat title="Largest Army" { (ic("army")) } }
                }
                @if is_turn && data.turn_seconds > 0 && data.turn_deadline_ms > 0
                    && matches!(game.phase, Phase::Play) {
                    span.ptimer data-deadline=(data.turn_deadline_ms) { "⏱ " span data-secs { "" } }
                }
                @if is_bot && matches!(game.phase, Phase::Play) {
                    @let bot_level = data.members.get(pid).map(|m| m.level).unwrap_or_default();
                    @let holds_grudges = bot_level != BotLevel::Easy;
                    @let grudge = if holds_grudges { viewer.map(|v| game.anger(pid, v)).unwrap_or(0) } else { 0 };
                    @let any_grudge = holds_grudges && game.max_anger(pid) > 0;
                    @if grudge > 0 {
                        div.phint.angry title="This bot is still cross with you" {
                            span.ph-row { (ic("angry")) span.ph-angry { "Still cross with you" } }
                        }
                    } @else if !any_grudge {
                        @let (need, gives) = crate::bot::trade_hint(game, pid, bot_level);
                        @if need.is_some() && !gives.is_empty() {
                            div.phint title="A rough idea of what this bot needs and will give" {
                                @if let Some(r) = need {
                                    span.ph-row { span.ph-l { "needs" } span.ph-r { (res_glyph(r)) } }
                                }
                                span.ph-row {
                                    span.ph-l { "gives" }
                                    @for r in &gives { span.ph-r { (res_glyph(*r)) } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Hand strip
// ---------------------------------------------------------------------------

fn hand_frag(game: &GameState, _code: &str, v: PlayerId, layout: Layout) -> Markup {
    let p = &game.players[v];
    let dev_total = p.all_dev_cards().count();
    let board = layout == Layout::Board;
    html! {
        @for r in ALL_RESOURCES {
            @let n = p.resources.get(r);
            div.rcard.zero[n == 0].art[board] data-res=(r.slug()) style=(res_card_style(r, layout)) title=(r.name()) {
                span.rcard-count { (n) }
                @if board {
                    span.rcard-name { (r.name()) }
                } @else {
                    span.rcard-icon { (res_glyph(r)) }
                }
            }
        }
        @if dev_total > 0 {
            div.devchip title="Development cards" { (ic("dev")) (dev_total) }
        }
        div.hspace {}
    }
}

// ---------------------------------------------------------------------------
// Floating status pill
// ---------------------------------------------------------------------------

fn controls_frag(game: &GameState, _data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    if matches!(game.phase, Phase::GameOver) {
        let w = game.winner.unwrap_or(0);
        return html! {
            div.pill.win { (ic("trophy")) (game.players[w].name) " wins with " (game.total_victory_points(w)) " VP!" }
        };
    }
    let Some(v) = viewer else { return Markup::default() };
    let my_turn = game.current == v;
    let text: Option<String> = match &game.phase {
        Phase::Setup => Some(if my_turn {
            if game.setup_road_from.is_some() {
                "Place your road — tap a glowing edge.".to_string()
            } else {
                "Place your settlement — tap a glowing spot.".to_string()
            }
        } else {
            format!("{} is placing…", game.players[game.current].name)
        }),
        Phase::Discard => {
            if game.pending_discards.contains(&v) {
                None
            } else {
                Some("Waiting for players to discard…".to_string())
            }
        }
        Phase::MoveRobber { .. } => Some(if my_turn {
            "Tap a highlighted hex to move the robber.".to_string()
        } else {
            "The robber is being moved…".to_string()
        }),
        Phase::Steal { .. } => Some(if my_turn {
            "Choose who to rob.".to_string()
        } else {
            "Choosing who to rob…".to_string()
        }),
        Phase::Play => {
            if my_turn {
                None
            } else {
                Some(format!("Waiting for {}…", game.players[game.current].name))
            }
        }
        Phase::Lobby => Some("Starting…".to_string()),
        Phase::GameOver => None,
    };
    match text {
        Some(t) => html! { div.pill { (t) } },
        None => Markup::default(),
    }
}

// ---------------------------------------------------------------------------
// Dock + bottom sheets
// ---------------------------------------------------------------------------

fn turn_frag(game: &GameState, data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    let Some(v) = viewer else {
        return html! { div.dock { div.dock-wait { "Spectating" } } };
    };
    let url = format!("/room/{}/action", data.code);
    let mode_url = format!("/room/{}/mode", data.code);
    let mode = data.members.get(v).map(|m| m.mode).unwrap_or_default();
    let layout = member_layout(data, v);
    let p = &game.players[v];
    let my = game.current == v;
    let my_play = my && matches!(game.phase, Phase::Play);
    let my_setup = my && matches!(game.phase, Phase::Setup);
    let my_robber = my && matches!(game.phase, Phase::MoveRobber { .. });

    let need_discard = matches!(game.phase, Phase::Discard) && game.pending_discards.contains(&v);
    let need_steal = my && matches!(game.phase, Phase::Steal { .. });

    let can_trade = my_play && p.resources.total() > 0 && game.trade.is_none();
    // The development sheet is reachable when the player can play a card, or
    // when they still hold a hidden Victory Point card: revealing is a free
    // action, so it stays available even after another dev card was played.
    let has_hidden_vp = p.dev_cards.contains(&DevCard::VictoryPoint);
    let has_dev =
        my_play && ((!game.played_dev_this_turn && !p.dev_cards.is_empty()) || has_hidden_vp);
    let rolled = game.dice.is_some();

    html! {
        div.dock data-my-turn[my_play] {
            div.dock-dice { (dock_dice(game, data)) }
            @if my_setup {
                div.dock-wait { "Place your piece on the board." }
            } @else if my_robber {
                div.dock-wait { "Move the robber." }
            } @else if my_play {
                div.dock-actions {
                    @if mode != ViewMode::Normal {
                        button.dockbtn type="button" data-sheet-open="build" title="Build" { (ic("build")) }
                        button.dockbtn hx-post=(mode_url) hx-vals=(r#"{"mode":"normal"}"#) title="Cancel" { (ic("x")) }
                    } @else if !rolled {
                        button.dockbtn.primary type="button" x-roll hx-post=(url) hx-vals=(r#"{"action":"roll"}"#) { (ic("dice")) "Roll" }
                    } @else {
                        button.dockbtn type="button" data-sheet-open="build" title="Build" { (ic("build")) }
                        @if can_trade {
                            button.dockbtn type="button" data-open-trade title="Trade" { (ic("trade")) }
                        }
                        @if has_dev {
                            button.dockbtn type="button" data-sheet-open="dev" title="Play development card" { (ic("dev")) }
                        }
                        button.dockbtn.end hx-post=(url) hx-vals=(r#"{"action":"end_turn"}"#) title="End turn" { (ic("play")) }
                    }
                }
            } @else if !my {
                div.dock-wait { "Waiting…" }
            }
        }
        (build_sheet(game, data, v, my_play))
        (dev_sheet(game, data, v, has_dev))
        @if need_steal {
            (steal_sheet(game, data, v))
        }
        @if need_discard {
            div.sheet data-sheet="discard" data-auto {
                div.sheet-head { h3 { "Discard" } }
                (discard_form(game, &data.code, v, layout))
            }
        }
    }
}

fn dock_dice(game: &GameState, data: &RoomData) -> Markup {
    let cur_is_bot = data.members.get(game.current).map(|m| m.is_bot).unwrap_or(false);
    let bot_rolling = cur_is_bot && matches!(game.phase, Phase::Play) && game.dice.is_none();
    html! {
        @if let Some((a, b)) = game.dice {
            span.dice-wrap title=(format!("{a} + {b}")) {
                (dice_anim())
                span.dice-real { (dice_face(a)) (dice_face(b)) span.dice-total { (a + b) } }
            }
        } @else if bot_rolling {
            span.dice-wrap.rolling title="Rolling…" { (dice_anim()) }
        }         @else {
            span.dice-total style="color:#dbeaf4" { "—" }
        }
    }
}

fn build_sheet(game: &GameState, data: &RoomData, v: PlayerId, active: bool) -> Markup {
    if !active {
        return Markup::default();
    }
    let url = format!("/room/{}/action", data.code);
    let mode_url = format!("/room/{}/mode", data.code);
    let mode = data.members.get(v).map(|m| m.mode).unwrap_or_default();
    let p = &game.players[v];
    let free_roads = game.free_roads_left > 0;
    let road_dis = mode != ViewMode::Normal
        || (!free_roads && !COST_ROAD.can_pay(&p.resources))
        || p.roads_left == 0;
    let set_dis =
        mode != ViewMode::Normal || !COST_SETTLEMENT.can_pay(&p.resources) || p.settlements_left == 0;
    let city_dis =
        mode != ViewMode::Normal || !COST_CITY.can_pay(&p.resources) || p.cities_left == 0;
    let dev_dis = game.dev_deck.is_empty() || !COST_DEV.can_pay(&p.resources);
    html! {
        div.sheet data-sheet="build" hidden {
            div.sheet-head {
                h3 { "Build" }
                div.grow {}
                button.sheet-close type="button" data-sheet-close { (ic("x")) }
            }
            div.build-grid {
                button.build-opt hx-post=(mode_url) hx-vals=(r#"{"mode":"road"}"#) disabled[road_dis] {
                    span.bo-top { (ic("road")) @if free_roads { "Road (free)" } @else { "Road" } }
                    @if free_roads {
                        span.cost-free { (game.free_roads_left) " free" }
                    } @else {
                        (cost_span(COST_ROAD))
                    }
                }
                button.build-opt hx-post=(mode_url) hx-vals=(r#"{"mode":"settlement"}"#) disabled[set_dis] {
                    span.bo-top { (ic("home")) "Settlement" }
                    (cost_span(COST_SETTLEMENT))
                }
                button.build-opt hx-post=(mode_url) hx-vals=(r#"{"mode":"city"}"#) disabled[city_dis] {
                    span.bo-top { (ic("city")) "City" }
                    (cost_span(COST_CITY))
                }
                button.build-opt hx-post=(url) hx-vals=(r#"{"action":"buy_dev"}"#) disabled[dev_dis] {
                    span.bo-top { (ic("dev")) "Dev card" }
                    (cost_span(COST_DEV))
                }
            }
            p.muted.small style="margin:8px 2px 0" {
                @if free_roads {
                    "Road Building: " (game.free_roads_left) " free road"
                    @if game.free_roads_left > 1 { "s" }
                    " available. "
                }
                "Settlement and city pieces left: " (p.settlements_left) " / " (p.cities_left) " · Roads left: " (p.roads_left)
            }
        }
    }
}

fn dev_sheet(game: &GameState, data: &RoomData, v: PlayerId, active: bool) -> Markup {
    if !active {
        return Markup::default();
    }
    let p = &game.players[v];
    let url = format!("/room/{}/action", data.code);
    let has = |c: DevCard| p.dev_cards.contains(&c);
    let board = member_layout(data, v) == Layout::Board;
    html! {
        div.sheet data-sheet="dev" hidden {
            div.sheet-head {
                h3 { "Play a development card" }
                div.grow {}
                button.sheet-close type="button" data-sheet-close { (ic("x")) }
            }
            @if game.played_dev_this_turn {
                p.muted.small style="margin:6px 2px 0" {
                    "You already played a development card this turn. You can still reveal a Victory Point card."
                }
            }
            @if has(DevCard::Knight) {
                div.devrow {
                    div.dev-ic.art[board] style=(format!("--card:url({})", dev_card_art(DevCard::Knight))) { (ic("shield")) }
                    div.dev-body {
                        div.dev-name { "Knight" }
                        div.dev-desc { (DevCard::Knight.description()) }
                    }
                    button.dockbtn disabled[game.played_dev_this_turn] hx-post=(url) hx-vals=(r#"{"action":"play_knight"}"#) title="Play Knight" { (ic("play")) }
                }
            }
            @if has(DevCard::RoadBuilding) {
                @let road_ok = p.roads_left > 0 && !game.legal_road_edges(v).is_empty();
                div.devrow {
                    div.dev-ic.art[board] style=(format!("--card:url({})", dev_card_art(DevCard::RoadBuilding))) { (ic("road")) }
                    div.dev-body {
                        div.dev-name { "Road Building" }
                        div.dev-desc { (DevCard::RoadBuilding.description()) }
                        @if !road_ok {
                            div.dev-note { "No legal place to put a road right now." }
                        }
                    }
                    button.dockbtn disabled[!road_ok || game.played_dev_this_turn] hx-post=(url) hx-vals=(r#"{"action":"play_road_building"}"#) title="Play Road Building" { (ic("play")) }
                }
            }
            @if has(DevCard::YearOfPlenty) {
                div.devrow {
                    div.dev-ic.art[board] style=(format!("--card:url({})", dev_card_art(DevCard::YearOfPlenty))) { (ic("offer")) }
                    div.dev-body {
                        div.dev-name { "Year of Plenty" }
                        div.dev-desc { (DevCard::YearOfPlenty.description()) }
                        form hx-post=(url) {
                            input type="hidden" name="action" value="play_yop";
                            select name="first" title="First resource" {
                                @for r in ALL_RESOURCES { option value=(r.slug()) { (r.name()) } }
                            }
                            select name="second" title="Second resource" {
                                @for r in ALL_RESOURCES { option value=(r.slug()) { (r.name()) } }
                            }
                            button.dockbtn disabled[game.played_dev_this_turn] type="submit" title="Play" { (ic("play")) }
                        }
                    }
                }
            }
            @if has(DevCard::Monopoly) {
                div.devrow {
                    div.dev-ic.art[board] style=(format!("--card:url({})", dev_card_art(DevCard::Monopoly))) { (ic("trophy")) }
                    div.dev-body {
                        div.dev-name { "Monopoly" }
                        div.dev-desc { (DevCard::Monopoly.description()) }
                        form hx-post=(url) {
                            input type="hidden" name="action" value="play_monopoly";
                            select name="resource" title="Resource" {
                                @for r in ALL_RESOURCES { option value=(r.slug()) { (r.name()) } }
                            }
                            button.dockbtn disabled[game.played_dev_this_turn] type="submit" title="Play" { (ic("play")) }
                        }
                    }
                }
            }
            @let vp_hidden = p
                .dev_cards
                .iter()
                .filter(|c| **c == DevCard::VictoryPoint)
                .count();
            @let vp_revealed = p.revealed_vp as usize;
            @if vp_hidden > 0 || vp_revealed > 0 {
                div.devrow {
                    div.dev-ic.art[board] style=(format!("--card:url({})", dev_card_art(DevCard::VictoryPoint))) { (ic("trophy")) }
                    div.dev-body {
                        div.dev-name {
                            "Victory Point"
                            @if vp_hidden + vp_revealed > 1 { " ×" (vp_hidden + vp_revealed) }
                        }
                        @if vp_revealed > 0 {
                            div.dev-desc { (vp_revealed) " revealed — visible to everyone." }
                        }
                        @if vp_hidden > 0 {
                            div.dev-desc {
                                "Hidden. Reveal " @if vp_hidden > 1 { "one of these" } @else { "it" }
                                " to show the point publicly. It still counts automatically if it would win the game."
                            }
                        }
                    }
                    @if vp_hidden > 0 {
                        button.dockbtn hx-post=(url) hx-vals=(r#"{"action":"reveal_vp"}"#) title="Reveal a Victory Point card" { (ic("play")) }
                    } @else {
                        span.dev-auto { "Revealed" }
                    }
                }
            }
            @if !p.new_dev_cards.is_empty() {
                p.muted.small { "Cards bought this turn can be played next turn." }
            }
        }
    }
}

fn steal_sheet(game: &GameState, data: &RoomData, v: PlayerId) -> Markup {
    let hex = match &game.phase {
        Phase::Steal { hex } => *hex,
        _ => return Markup::default(),
    };
    let url = format!("/room/{}/action", data.code);
    html! {
        div.sheet data-sheet="steal" data-auto {
            div.sheet-head { h3 { "Choose who to rob" } }
            div.steal-grid {
                @for c in game.steal_candidates(hex, v) {
                    button.steal-opt hx-post=(url) hx-vals=(format!(r#"{{"action":"steal","player":"{c}"}}"#)) {
                        span.pdot style=(format!("background:{}", color_hex(game.players[c].color))) {}
                        (game.players[c].name)
                        span.cnt { (game.players[c].resources.total()) }
                    }
                }
                button.steal-opt hx-post=(url) hx-vals=(r#"{"action":"steal","player":"none"}"#) { "Nobody" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Menu sheet (info / rules / log)
// ---------------------------------------------------------------------------

fn menu_sheet(code: &str, data: &RoomData, game: &GameState, _viewer: Option<PlayerId>) -> Markup {
    let url = format!("/room/{code}");
    html! {
        div.sheet data-sheet="menu" hidden {
            div.sheet-head {
                h3 { "Game info" }
                div.grow {}
                button.sheet-close type="button" data-sheet-close { (ic("x")) }
            }
            p style="margin:0 0 10px" {
                "Room " code.roomcode { (code) }
                @if data.turn_seconds > 0 { span.muted { " · " (data.turn_seconds / 60) " min turns" } }
            }
            h3 { "Build guide" }
            (build_guide())
            h3 style="margin-top:14px" { "How to play" }
            a.btn.sec href="/how-to-play" { (ic("info")) "Open the how-to-play guide" }
            h3 style="margin-top:14px" { "Log" }
            div #log.logbox sse-swap="log" { (log_frag(game)) }
            p.muted.small style="margin-top:12px" {
                a href=(url) { "Refresh" }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Trade modal (bottom sheet)
// ---------------------------------------------------------------------------

fn trade_modal(game: &GameState, data: &RoomData, v: PlayerId) -> Markup {
    let url = format!("/room/{}/action", data.code);
    let p = &game.players[v];
    let layout = member_layout(data, v);
    html! {
        div #trade-modal.modal hidden {
            div.modal-card {
                div.sheet-head {
                    h3 { "Trade" }
                    div.grow {}
                    button.sheet-close type="button" data-close-trade title="Close" { (ic("x")) }
                }
                p.muted.small { "Tap cards to build your offer. Shared with everyone — first to accept trades. Ends in 30s or when all decline." }
                strong { "You give" }
                div.picker { (pick_cards("give", Some(&p.resources), layout)) }
                strong { "You want" }
                div.picker { (pick_cards("want", None, layout)) }
                p.muted.small style="margin:-8px 0 10px" {
                    "Tap a card to add it; tap it 5 times to cycle back to 0."
                }
                div.trade-preview {
                    span #preview-give { "—" }
                    span.arrow { "⇄" }
                    span #preview-want { "—" }
                }
                form #trade-form hx-post=(url) {
                    input type="hidden" name="action" value="propose_trade";
                    input type="hidden" name="give" value="";
                    input type="hidden" name="want" value="";
                    @for r in ALL_RESOURCES {
                        input type="hidden" name=(format!("give_{}", r.slug())) value="0";
                        input type="hidden" name=(format!("want_{}", r.slug())) value="0";
                    }
                    div.row {
                        button #offer-btn.btn.sheetbtn type="submit" title="Offer to all players" disabled { (ic("offer")) "Offer" }
                        button #bank-btn.btn.sec.sheetbtn type="submit" title="Exchange with bank / port" disabled { (ic("swap")) "Bank" }
                    }
                }
            }
        }
    }
}

fn discard_form(game: &GameState, code: &str, v: PlayerId, layout: Layout) -> Markup {
    let p = &game.players[v];
    let total = p.resources.total();
    let need = total / 2;
    let board = layout == Layout::Board;
    html! {
        p { "A 7 was rolled and you hold " b { (total) } " cards — discard " b { (need) } " down to half. Tap cards to choose." }
        form #discard-form data-need=(need) hx-post=(format!("/room/{code}/action")) {
            input type="hidden" name="action" value="discard";
            div.picker {
                @for r in ALL_RESOURCES {
                    @let have = p.resources.get(r);
                    button.rcard.pick.disabled[have == 0].art[board] type="button" data-res=(r.slug())
                        data-group="discard" data-max=(have) title=(r.name())
                        style=(res_card_style(r, layout)) {
                        span.pick-count data-count="0" { "0" }
                        @if !board { span.rcard-icon { (res_glyph(r)) } }
                        span.rcard-name { (r.name()) }
                    }
                }
            }
            @for r in ALL_RESOURCES {
                input type="hidden" name=(r.slug()) value="0";
            }
            div.row {
                span.muted #discard-status { "0 / " (need) " selected" }
                button #discard-btn.btn.warn.sheetbtn type="submit" disabled { (ic("trash")) "Discard " (need) }
            }
        }
    }
}

fn pick_cards(group: &str, max: Option<&ResourceHand>, layout: Layout) -> Markup {
    let board = layout == Layout::Board;
    html! {
        @for r in ALL_RESOURCES {
            @let cap = max.map(|h| h.get(r));
            @let none_left = cap == Some(0);
            button.rcard.pick.disabled[none_left].art[board] type="button" data-res=(r.slug()) data-group=(group)
                data-max=[cap] title=(r.name()) style=(res_card_style(r, layout)) {
                span.pick-count data-count="0" { "0" }
                @if !board { span.rcard-icon { (res_glyph(r)) } }
                span.rcard-name { (r.name()) }
            }
        }
    }
}

fn trades_frag(game: &GameState, code: &str, viewer: Option<PlayerId>) -> Markup {
    let Some(o) = &game.trade else {
        return Markup::default();
    };
    let from = &game.players[o.from];
    let url = format!("/room/{code}/action");
    html! {
        div.trade-card data-deadline=(o.deadline_ms) {
            div.trade-head {
                h3 { (ic("trade")) " Trade offer" }
                @if o.deadline_ms > 0 {
                    span.trade-timer { "⏱ " span data-secs { "30s" } }
                }
            }
            div.trade-line {
                b { (from.name) } " gives " (cost_span(o.give))
                span.arrow { "⇄" }
                span.muted { "wants" } (cost_span(o.want))
            }
            @if Some(o.from) == viewer {
                button.btn.icon.warn title="Withdraw offer" hx-post=(url) hx-vals=(r#"{"action":"cancel_trade"}"#) { (ic("x")) " Withdraw" }
            } @else if let Some(v) = viewer {
                @let my = o.responses.iter().find(|(p, _)| *p == v).map(|(_, r)| *r);
                @let can_pay = o.want.can_pay(&game.players[v].resources);
                @if my == Some(TradeResponse::Pending) && can_pay {
                    div.row {
                        button.btn title="Accept (first to accept trades)" hx-post=(url) hx-vals=(r#"{"action":"respond_trade","accept":"1"}"#) { (ic("check")) " Accept" }
                        button.btn.sec title="Decline" hx-post=(url) hx-vals=(r#"{"action":"respond_trade","accept":"0"}"#) { (ic("x")) " Decline" }
                    }
                } @else if my == Some(TradeResponse::Pending) {
                    div.row {
                        span.muted { "You don't have what they want." }
                        button.btn.sec title="Decline" hx-post=(url) hx-vals=(r#"{"action":"respond_trade","accept":"0"}"#) { (ic("x")) " Decline" }
                    }
                } @else if my == Some(TradeResponse::Declined) {
                    @if can_pay {
                        p.muted { "You declined this offer." }
                    } @else {
                        p.muted { "You don't have what they want — auto-declined." }
                    }
                } @else {
                    p.muted { "Waiting for players to respond." }
                }
            } @else {
                p.muted { "Waiting for players to respond." }
            }
        }
    }
}

fn log_frag(game: &GameState) -> Markup {
    html! {
        @for entry in game.log.iter().rev().take(60) {
            div { (entry.text) }
        }
    }
}

// ---------------------------------------------------------------------------
// Board SVG
// ---------------------------------------------------------------------------

fn board_frag(game: &GameState, data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    let board = &game.board;
    let (minx, miny, w, h) = bounds(board);
    let mut highlights: Vec<Overlay> = Vec::new();

    if let Some(v) = viewer {
        let member_mode = data.members.get(v).map(|m| m.mode).unwrap_or_default();
        let is_setup_player = matches!(game.phase, Phase::Setup) && game.current == v;
        let is_my_turn = game.current == v;

        if is_setup_player {
            if game.setup_road_from.is_some() {
                for e in game.legal_road_edges(v) {
                    highlights.push(Overlay::Edge(e));
                }
            } else {
                for vert in game.legal_settlement_vertices(v) {
                    highlights.push(Overlay::Vertex(vert));
                }
            }
        } else if is_my_turn && matches!(game.phase, Phase::Play) {
            match member_mode {
                ViewMode::PlaceRoad => {
                    for e in game.legal_road_edges(v) {
                        highlights.push(Overlay::Edge(e));
                    }
                }
                ViewMode::PlaceSettlement => {
                    for vert in game.legal_settlement_vertices(v) {
                        highlights.push(Overlay::Vertex(vert));
                    }
                }
                ViewMode::PlaceCity => {
                    for vert in game.legal_city_vertices(v) {
                        highlights.push(Overlay::Vertex(vert));
                    }
                }
                ViewMode::Normal => {}
            }
        } else if is_my_turn && matches!(game.phase, Phase::MoveRobber { .. }) {
            for hex in game.legal_robber_hexes() {
                highlights.push(Overlay::Hex(hex));
            }
        }
    }

    let action_url = format!("/room/{}/action", data.code);
    let layout = viewer
        .and_then(|v| data.members.get(v))
        .map(|m| m.layout)
        .unwrap_or_default();
    // Inset hexagon shared by every tile: the terrain art is clipped to it so the
    // sandy base shows through as a thin light-brown border inside each hex.
    let hex_clip = hex_points_at(0.0, 0.0, 0.94);

    html! {
        svg viewBox=(format!("{:.2} {:.2} {:.2} {:.2}", minx, miny, w, h))
            preserveAspectRatio="xMidYMid meet" xmlns="http://www.w3.org/2000/svg" {
            defs {
                clipPath #hexClip { polygon points=(hex_clip) {} }
                // Seamless sandy base - the light-brown border between tiles.
                pattern #sand patternUnits="userSpaceOnUse" width="3" height="3" {
                    image href="/static/board/beach.webp" x="0" y="0" width="3" height="3" preserveAspectRatio="xMidYMid slice" {}
                }
            }
            // Sandy beach ring around the island's outer coast (board look only;
            // drawn first so the tiles cover the inner half of each stroke).
            @if layout == Layout::Board {
                @for e in &board.edges {
                    @if e.hexes.len() == 1 {
                        @let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
                        @let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
                        line x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                            stroke="url(#sand)" stroke-width="0.22" stroke-linecap="round" {}
                    }
                }
            }
            @for hex in &board.hexes {
                @if layout == Layout::Board {
                    // Painted tile: a sandy base, then the tile image clipped to an
                    // inset hexagon so the sand is the light-brown border.
                    polygon points=(hex_points(hex)) fill="url(#sand)" stroke="#4b5058" stroke-opacity="0.7" stroke-width="0.03" {}
                    g transform=(format!("translate({:.3} {:.3})", hex.cx, hex.cy)) {
                        image href=(terrain_tile(hex.terrain)) x="-1" y="-1" width="2" height="2"
                            preserveAspectRatio="xMidYMid slice" clip-path="url(#hexClip)" {}
                        // Lift the painted art so pieces, roads and tokens read clearly.
                        rect x="-1" y="-1" width="2" height="2" fill="#ffffff" fill-opacity="0.2" clip-path="url(#hexClip)" {}
                    }
                } @else {
                    // Classic flat colour tile with a single resource glyph.
                    polygon points=(hex_points(hex)) fill=(terrain_fill(hex.terrain)) stroke="#3e444b" stroke-width="0.055" {}
                    (flat_tile_icon(hex.terrain, hex.cx, hex.cy - 0.45))
                }
                @if let Some(n) = hex.number {
                    circle cx=(format!("{:.3}", hex.cx)) cy=(format!("{:.3}", hex.cy + 0.05)) r="0.27" fill="#f4efe3" stroke="#b9ab8d" stroke-width="0.02" {}
                    text x=(format!("{:.3}", hex.cx)) y=(format!("{:.3}", hex.cy + 0.075))
                        text-anchor="middle" font-size="0.27" font-weight="800"
                        fill=(if n == 6 || n == 8 { "#c0392b" } else { "#2b2b2b" }) { (n) }
                    @let pips = 6 - (n as i32 - 7).abs();
                    @let start = -(pips as f64 - 1.0) / 2.0;
                    @let pip_fill = if n == 6 || n == 8 { "#c0392b" } else { "#6b5a3a" };
                    @for i in 0..pips {
                        circle cx=(format!("{:.3}", hex.cx + (start + i as f64) * 0.052)) cy=(format!("{:.3}", hex.cy + 0.2))
                            r="0.024" fill=(pip_fill) {}
                    }
                }
            }

            // Ports: a jetty from the coast, a ratio badge, and a little boat.
            @for p in port_marks(board) {
                (port_mark_svg(&p))
            }

            // Robber
            @if robber_placed(board, game) {
                @let rh = &board.hexes[game.robber_hex];
                (board_robber(rh.cx + 0.5, rh.cy + 0.46, layout == Layout::Board))
            }

            // Roads: painted planks (board look), or the classic casing + fill.
            @if layout == Layout::Board {
                @for e in &board.edges {
                    @if let Some(owner) = e.owner {
                        @let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
                        @let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
                        (road_piece(ax, ay, bx, by, color_hex(game.players[owner].color)))
                    }
                }
            } @else {
                @for e in &board.edges {
                    @if e.owner.is_some() {
                        @let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
                        @let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
                        line x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                            stroke="#2b2f35" stroke-width="0.175" stroke-linecap="round" {}
                    }
                }
                @for e in &board.edges {
                    @if let Some(owner) = e.owner {
                        @let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
                        @let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
                        line x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                            stroke=(color_hex(game.players[owner].color)) stroke-width="0.105" stroke-linecap="round" {}
                    }
                }
            }

            // Buildings: painted pieces (board look) or the classic flat polygons.
            @for v in &board.vertices {
                @if let (Some(owner), b) = (v.owner, v.building) {
                    @let col = color_hex(game.players[owner].color);
                    @if b == Building::Settlement {
                        @if layout == Layout::Board {
                            (piece(SETTLEMENT_SVG, v.x, v.y, 0.0062, col))
                        } @else {
                            polygon points=(house_points(v.x, v.y, 0.17)) fill=(col) stroke="#2b2f35" stroke-width="0.035" {}
                        }
                    } @else if b == Building::City {
                        @if layout == Layout::Board {
                            (piece(CITY_SVG, v.x, v.y, 0.009, col))
                        } @else {
                            polygon points=(house_points(v.x, v.y, 0.25)) fill=(col) stroke="#2b2f35" stroke-width="0.05" {}
                        }
                    }
                }
            }

            // Interaction overlays
            @for ov in &highlights {
                @match ov {
                    Overlay::Vertex(vi) => {
                        @let v = &board.vertices[*vi];
                        circle.spot cx=(format!("{:.3}", v.x)) cy=(format!("{:.3}", v.y)) r="0.30"
                            fill="transparent" stroke="none"
                            hx-post=(action_url) hx-vals=(format!(r#"{{"action":"place_vertex","vertex":"{vi}"}}"#)) {}
                        circle cx=(format!("{:.3}", v.x)) cy=(format!("{:.3}", v.y)) r="0.18" pointer-events="none"
                            fill="#ffffff55" stroke="#ffffff" stroke-width="0.03" stroke-dasharray="0.08 0.06" {}
                    }
                    Overlay::Edge(ei) => {
                        @let e = &board.edges[*ei];
                        @let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
                        @let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
                        line.spot x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                            stroke="transparent" stroke-width="0.5" stroke-linecap="round"
                            hx-post=(action_url) hx-vals=(format!(r#"{{"action":"place_edge","edge":"{ei}"}}"#)) {}
                        line x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                            stroke="#ffffffaa" stroke-width="0.17" stroke-linecap="round" pointer-events="none" {}
                    }
                    Overlay::Hex(hi) => {
                        @let hex = &board.hexes[*hi];
                        polygon.spot points=(hex_points(hex)) fill="#ff000033" stroke="#ff6b6b" stroke-width="0.05"
                            hx-post=(action_url) hx-vals=(format!(r#"{{"action":"move_robber","hex":"{hi}"}}"#)) {}
                    }
                }
            }
        }
    }
}

enum Overlay {
    Vertex(usize),
    Edge(usize),
    Hex(usize),
}

fn robber_placed(board: &Board, game: &GameState) -> bool {
    board
        .hexes
        .iter()
        .position(|h| h.terrain == Terrain::Desert)
        .map(|desert| desert != game.robber_hex)
        .unwrap_or(true)
}

/// Terrain → the resource it produces, for player-facing labels.
fn terrain_resource(t: Terrain) -> &'static str {
    match t {
        Terrain::Wood => "Wood",
        Terrain::Brick => "Brick",
        Terrain::Wheat => "Wheat",
        Terrain::Ore => "Ore",
        Terrain::Sheep => "Sheep",
        Terrain::Desert => "Desert",
    }
}

/// Human-readable robber location, e.g. `8 - Wood` (desert has no number).
fn robber_label(game: &GameState) -> String {
    let hex = &game.board.hexes[game.robber_hex];
    match hex.number {
        Some(n) => format!("{n} - {}", terrain_resource(hex.terrain)),
        None => terrain_resource(hex.terrain).to_string(),
    }
}

fn bounds(board: &Board) -> (f64, f64, f64, f64) {
    let mut minx = f64::MAX;
    let mut miny = f64::MAX;
    let mut maxx = f64::MIN;
    let mut maxy = f64::MIN;
    for v in &board.vertices {
        minx = minx.min(v.x);
        miny = miny.min(v.y);
        maxx = maxx.max(v.x);
        maxy = maxy.max(v.y);
    }
    let m = 2.0;
    (minx - m, miny - m, (maxx - minx) + 2.0 * m, (maxy - miny) + 2.0 * m)
}

fn hex_points(hex: &crate::game::board::Hex) -> String {
    hex_points_at(hex.cx, hex.cy, 1.0)
}

/// Pointy-top hexagon outline (6 `x,y` pairs) centred on `(cx,cy)` with circumradius `r`.
fn hex_points_at(cx: f64, cy: f64, r: f64) -> String {
    let mut pts = String::new();
    for i in 0..6 {
        let ang = (60.0 * i as f64 - 30.0f64).to_radians();
        let x = cx + r * ang.cos();
        let y = cy + r * ang.sin();
        if i > 0 {
            pts.push(' ');
        }
        pts.push_str(&format!("{x:.3},{y:.3}"));
    }
    pts
}

fn house_points(cx: f64, cy: f64, s: f64) -> String {
    let pts = [
        (0.0, -1.05),
        (0.95, -0.15),
        (0.72, -0.15),
        (0.72, 0.85),
        (-0.72, 0.85),
        (-0.72, -0.15),
        (-0.95, -0.15),
    ];
    pts.iter()
        .map(|(x, y)| format!("{:.3},{:.3}", cx + x * s, cy + y * s))
        .collect::<Vec<_>>()
        .join(" ")
}

struct PortMark {
    kind: PortKind,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    ux: f64,
    uy: f64,
    badge_x: f64,
    badge_y: f64,
    boat_x: f64,
    boat_y: f64,
}

fn port_ratio(kind: PortKind) -> &'static str {
    if kind.resource().is_some() { "2:1" } else { "3:1" }
}

/// The little boat marker (painted art), shared by both board styles. The sail is
/// intentionally large so the harbour's ratio/resource can be drawn on it.
fn port_boat(x: f64, y: f64) -> Markup {
    maud::PreEscaped(format!(
        r##"<image href="/static/art/port-boat.webp" x="{:.3}" y="{:.3}" width="0.98" height="0.98" preserveAspectRatio="xMidYMid meet"/>"##,
        x - 0.49,
        y - 0.49
    ))
}

/// A straight jetty plank, rotated along the harbour's outward axis.
fn port_bridge(cx: f64, cy: f64, deg: f64, w: f64, h: f64) -> Markup {
    maud::PreEscaped(format!(
        r##"<g transform="translate({cx:.3} {cy:.3}) rotate({deg:.1})"><image href="/static/art/port-bridge.webp" x="{:.3}" y="{:.3}" width="{w:.3}" height="{h:.3}" preserveAspectRatio="xMidYMid slice"/></g>"##,
        -w / 2.0,
        -h / 2.0
    ))
}

/// One harbour: two small jetty legs (one per coastal vertex) and a boat at the
/// end of each, with the exchange ratio (and resource) drawn on the sails.
fn port_mark_svg(p: &PortMark) -> Markup {
    let ratio = port_ratio(p.kind);
    let ink = "#2b2f35";
    let deg = p.uy.atan2(p.ux).to_degrees();
    let leg = 0.5_f64;
    let boat_d = leg + 0.3;
    let (acx, acy) = (p.ax + p.ux * leg / 2.0, p.ay + p.uy * leg / 2.0);
    let (bcx, bcy) = (p.bx + p.ux * leg / 2.0, p.by + p.uy * leg / 2.0);
    let (ax, ay) = (p.ax + p.ux * boat_d, p.ay + p.uy * boat_d);
    let (bx, by) = (p.bx + p.ux * boat_d, p.by + p.uy * boat_d);
    html! {
        (port_bridge(acx, acy, deg, leg, 0.16))
        (port_bridge(bcx, bcy, deg, leg, 0.16))
        (port_boat(ax, ay))
        (port_boat(bx, by))
        @if let Some(r) = p.kind.resource() {
            (res_image(r, ax - 0.13, ay - 0.4, 0.26))
            (res_image(r, bx - 0.13, by - 0.4, 0.26))
            text x=(format!("{ax:.3}")) y=(format!("{:.3}", ay + 0.05)) text-anchor="middle" font-size="0.18" font-weight="800" fill=(ink) { (ratio) }
            text x=(format!("{bx:.3}")) y=(format!("{:.3}", by + 0.05)) text-anchor="middle" font-size="0.18" font-weight="800" fill=(ink) { (ratio) }
        } @else {
            text x=(format!("{ax:.3}")) y=(format!("{:.3}", ay - 0.01)) text-anchor="middle" font-size="0.22" font-weight="800" fill=(ink) { (ratio) }
            text x=(format!("{bx:.3}")) y=(format!("{:.3}", by - 0.01)) text-anchor="middle" font-size="0.22" font-weight="800" fill=(ink) { (ratio) }
        }
    }
}

fn port_marks(board: &Board) -> Vec<PortMark> {
    let mut out = Vec::new();
    for e in &board.edges {
        if e.hexes.len() != 1 {
            continue;
        }
        let (Some(pa), Some(pb)) = (board.vertices[e.a].port, board.vertices[e.b].port) else {
            continue;
        };
        if pa != pb {
            continue;
        }
        let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
        let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
        let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
        let len = (mx * mx + my * my).sqrt().max(0.001);
        let (ux, uy) = (mx / len, my / len);
        let badge_r = len + 1.0;
        let boat_r = len + 1.55;
        out.push(PortMark {
            kind: pa,
            ax,
            ay,
            bx,
            by,
            ux,
            uy,
            badge_x: ux * badge_r,
            badge_y: uy * badge_r,
            boat_x: ux * boat_r,
            boat_y: uy * boat_r,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Dice
// ---------------------------------------------------------------------------

/// A small SVG die face (1–6), drawn so dice look the same everywhere.
fn dice_face(n: u8) -> Markup {
    let pip = |x: i32, y: i32| -> Markup { html! { circle cx=(x) cy=(y) r="2.05" fill="#1c2430" {} } };
    html! {
        svg.die viewBox="0 0 20 20" width="30" height="30" aria-hidden="true" {
            rect x="1" y="1" width="18" height="18" rx="4" fill="#f4efe3" stroke="#b9ab8d" stroke-width="0.9" {}
            @match n {
                1 => { (pip(10, 10)) }
                2 => { (pip(6, 6)) (pip(14, 14)) }
                3 => { (pip(6, 6)) (pip(10, 10)) (pip(14, 14)) }
                4 => { (pip(6, 6)) (pip(14, 6)) (pip(6, 14)) (pip(14, 14)) }
                5 => { (pip(6, 6)) (pip(14, 6)) (pip(10, 10)) (pip(6, 14)) (pip(14, 14)) }
                _ => { (pip(6, 5)) (pip(6, 10)) (pip(6, 15)) (pip(14, 5)) (pip(14, 10)) (pip(14, 15)) }
            }
        }
    }
}

/// Six overlaid die faces cycled by CSS to look like a tumbling die.
fn dice_anim() -> Markup {
    html! {
        span.dice-anim aria-hidden="true" {
            @for n in 1..=6u8 { (dice_face(n)) }
        }
    }
}

// ---------------------------------------------------------------------------
// Costs and build guide
// ---------------------------------------------------------------------------

fn cost_span(cost: Bundle) -> Markup {
    html! {
        span.cost {
            @for (r, n) in cost.iter_nonzero() {
                span.cost-item title=(r.name()) {
                    (res_glyph(r))
                    @if n > 1 { span.cnt { (n) } }
                }
            }
        }
    }
}

fn guide_row(icon_name: &str, name: &str, cost: Bundle) -> Markup {
    html! {
        div.guide-row {
            span.guide-icon { (ic(icon_name)) }
            span.guide-name { (name) }
            (cost_span(cost))
        }
    }
}

fn build_guide() -> Markup {
    html! {
        div.guide {
            (guide_row("road", "Road", COST_ROAD))
            (guide_row("home", "Settlement", COST_SETTLEMENT))
            (guide_row("city", "City", COST_CITY))
            (guide_row("dev", "Dev card", COST_DEV))
        }
    }
}

fn bundle_text(b: &Bundle) -> String {
    let parts: Vec<String> = b
        .iter_nonzero()
        .map(|(r, n)| format!("{n} {}", r.name()))
        .collect();
    if parts.is_empty() { "nothing".to_string() } else { parts.join(", ") }
}

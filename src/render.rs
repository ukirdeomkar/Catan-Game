use crate::game::board::{Board, Building, PortKind, Terrain};
use crate::game::resources::{
    ALL_RESOURCES, Bundle, COST_CITY, COST_DEV, COST_ROAD, COST_SETTLEMENT, DevCard, Resource,
};
use crate::game::state::{Color, GameState, Phase, PlayerId};
use crate::state::{RoomData, ViewMode};
use maud::{DOCTYPE, Markup, html};

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

fn terrain_fill(t: Terrain) -> &'static str {
    match t {
        Terrain::Wood => "#2f6b3a",
        Terrain::Brick => "#a5522f",
        Terrain::Wheat => "#cfa93f",
        Terrain::Ore => "#6d6d78",
        Terrain::Sheep => "#7fae55",
        Terrain::Desert => "#e3d3a8",
    }
}

fn color_hex(c: Color) -> &'static str {
    match c {
        Color::Red => "#c0392b",
        Color::Blue => "#2e7dd1",
        Color::Orange => "#e08a1e",
        Color::White => "#e8ecef",
    }
}

fn terrain_glyph(t: Terrain) -> &'static str {
    match t {
        Terrain::Wood => "🌲",
        Terrain::Brick => "🧱",
        Terrain::Wheat => "🌾",
        Terrain::Ore => "🪨",
        Terrain::Sheep => "🐑",
        Terrain::Desert => "🏜",
    }
}

const CSS: &str = r#"
*{box-sizing:border-box}
body{margin:0;font-family:system-ui,Segoe UI,Roboto,sans-serif;background:radial-gradient(circle at 50% 0%,#1a2430 0%,#12181f 70%);color:#e9eef2;-webkit-text-size-adjust:100%}
a{color:#6cc0ff}
h1,h2,h3{margin:.2em 0}
.wrap{max-width:1180px;margin:0 auto;padding:14px}
.card{background:#1e262e;border:1px solid #2c3742;border-radius:10px;padding:12px;margin-bottom:12px}
.btn{background:#2e7dd1;color:#fff;border:0;border-radius:8px;padding:9px 13px;font-size:14px;cursor:pointer;min-height:38px;touch-action:manipulation;-webkit-tap-highlight-color:transparent}
.btn:hover{background:#3a8fe0}
.btn.sec{background:#37424e}
.btn.sec:hover{background:#44515f}
.btn.warn{background:#c0392b}
.btn:disabled{opacity:.4;cursor:not-allowed}
.row{display:flex;gap:10px;flex-wrap:wrap;align-items:center}
.game{display:grid;grid-template-columns:minmax(0,1fr) 340px;gap:14px}
#board{background:radial-gradient(circle at 50% 42%,#1f6ea0 0%,#14507a 45%,#0a3355 78%,#072742 100%);border-radius:12px;padding:6px}
#board svg{width:100%;height:auto;display:block;touch-action:manipulation}
.spot{cursor:pointer}
.spot:hover{filter:brightness(1.4)}
.players{display:flex;gap:6px;margin-bottom:2px}
.pcard{flex:1 1 0;min-width:0;display:flex;flex-direction:column;align-items:center;gap:3px;padding:7px 4px;border-radius:9px;background:#232c35;border:2px solid transparent}
.pcard.me{border-color:#6cc0ff}
.pcard.turn{border-color:#f0c040}
.pcard .pdot{width:11px;height:11px;border-radius:50%;border:1px solid #0006}
.pcard .pname{font-size:12px;font-weight:700;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.pcard .pstats{display:flex;flex-wrap:wrap;justify-content:center;gap:2px 6px;font-size:10px;color:#b9c6d1}
.pcard .pstats span{white-space:nowrap}
.pcard .pbadges{font-size:10px;line-height:1}
.dot{width:14px;height:14px;border-radius:50%;border:1px solid #0006;flex:0 0 auto}
.hand{display:flex;gap:8px;flex-wrap:wrap}
.rcard{position:relative;width:60px;height:82px;border-radius:8px;background:linear-gradient(#2b3846,#1f2831);border:1px solid #3a4a5a;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:2px;box-shadow:0 2px 5px #0005}
.rcard.zero{opacity:.4}
.rcard .rcard-count{position:absolute;top:2px;left:6px;font-size:14px;font-weight:800;color:#fff}
.rcard .rcard-icon{font-size:26px;line-height:1}
.rcard .rcard-name{font-size:10px;color:#9fb0c0}
.badge{font-size:11px;background:#33414e;border-radius:6px;padding:2px 6px;margin-left:4px}
.chip{display:inline-flex;gap:6px;align-items:center;background:#27313b;border-radius:8px;padding:5px 9px;margin:2px;font-size:13px}
.res{width:12px;height:12px;border-radius:3px;display:inline-block}
.turnbar{display:flex;justify-content:space-between;gap:8px;align-items:center;flex-wrap:wrap}
.log{max-height:220px;overflow:auto;font-size:13px;line-height:1.5}
.log div{border-bottom:1px solid #263039;padding:3px 0}
#toasts{color:#ff9a8a;font-size:13px;min-height:0}
#toasts:not(:empty){min-height:20px;padding:6px 0}
label{font-size:13px;color:#b9c6d1}
select,input[type=text],input[type=number]{background:#0f151b;color:#e9eef2;border:1px solid #35414c;border-radius:6px;padding:5px}
.muted{color:#8ea0af;font-size:13px}
.center{text-align:center}
.win{font-size:20px;font-weight:700;color:#7be08a}
code{background:#0f151b;padding:1px 6px;border-radius:5px}
.guide-row{display:flex;align-items:center;gap:8px;padding:5px 0;border-bottom:1px solid #263039}
.guide-row:last-child{border-bottom:0}
.guide-icon{font-size:17px;width:22px;text-align:center;flex:0 0 auto}
.guide-name{flex:1;font-size:13px}
.cost{display:inline-flex;align-items:baseline;gap:1px;font-size:15px;white-space:nowrap}
.cost-item{display:inline-flex;align-items:baseline;margin-left:3px}
.cost .cnt{font-size:11px;font-weight:700;color:#cfd9e2;margin-left:1px}
.dice{display:inline-flex;gap:6px;align-items:center}
.die{filter:drop-shadow(0 2px 3px #0008);animation:diepop .3s cubic-bezier(.2,1.35,.4,1)}
@keyframes diepop{from{transform:scale(.5) rotate(-18deg);opacity:.3}to{transform:none;opacity:1}}
.dice-total{font-weight:800;font-size:14px;margin-left:2px;color:#f0c040}
.btn.icon{padding:10px 13px;font-size:19px;line-height:1}
.btn.icon small{display:none}
.small{font-size:11px}
.arrow{color:#8ea0af;font-size:18px}
.trade-card{outline:1px solid #f0c04040}
.trade-head{display:flex;justify-content:space-between;align-items:center;gap:8px}
.trade-timer{font-size:13px;font-weight:700;color:#f0c040;background:#2a3340;border-radius:6px;padding:2px 8px;white-space:nowrap}
.trade-timer.expiring{color:#ff9a8a;background:#3a2530}
.trade-line{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin:10px 0;font-size:15px}
.trade-card .cost{font-size:19px}
.trade-line .cost{font-size:19px}
button.rcard{font:inherit;color:inherit;cursor:pointer;padding:0}
.pick{opacity:.55;transition:opacity .15s,border-color .15s}
.pick.sel{opacity:1;border-color:#6cc0ff;box-shadow:0 0 0 1px #6cc0ff inset}
.pick-count{position:absolute;top:-7px;right:-7px;background:#2e7dd1;color:#fff;border-radius:11px;min-width:20px;height:20px;font-size:12px;line-height:20px;text-align:center;font-weight:800;padding:0 4px}
.pick-count[data-count="0"]{display:none}
.modal{position:fixed;inset:0;background:#0009;display:flex;align-items:center;justify-content:center;z-index:60;padding:12px}
.modal[hidden]{display:none}
.modal-card{background:#1e262e;border:1px solid #2c3742;border-radius:14px;padding:14px;width:100%;max-width:440px;max-height:92vh;overflow:auto;box-shadow:0 12px 40px #000a}
.picker{display:flex;gap:8px;flex-wrap:wrap;margin:6px 0 12px}
.trade-preview{display:flex;align-items:center;justify-content:center;gap:10px;font-size:20px;background:#161d24;border-radius:8px;padding:8px;margin-bottom:10px;min-height:44px}
.turn-timer{font-size:13px;font-weight:700;color:#f0c040;background:#2a3340;border-radius:6px;padding:2px 8px;white-space:nowrap}
@media(max-width:860px){
 .game{grid-template-columns:1fr}
 .wrap{padding:10px}
 .card{padding:10px;margin-bottom:10px}
 .btn{padding:10px 14px;font-size:15px;min-height:44px}
 .log{max-height:160px}
 #controls{position:sticky;top:0;z-index:20}
}
"#;

fn shell(title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                style { (maud::PreEscaped(CSS)) }
                script src="/static/htmx.min.js" defer {}
                script src="/static/sse.js" defer {}
                script src="/static/timer.js" defer {}
                script src="/static/trade.js" defer {}
            }
            body { (body) }
        }
    }
}

// ---------------------------------------------------------------------------
// Public pages
// ---------------------------------------------------------------------------

pub fn home_page(error: Option<&str>) -> Markup {
    shell("Catan", html! {
        div.wrap {
            h1 { "Settlers of Catan" }
            p.muted { "A tiny self-hosted Catan. Create a room and share the code, or join with a code." }
            @if let Some(e) = error { div.card #toasts { (e) } }
            div.card {
                h3 { "Create a new game" }
                form method="post" action="/create" {
                    div.row {
                        input type="text" name="name" placeholder="Name (max 5)" maxlength="5";
                        label { "Turn limit " select name="turn_seconds" {
                            option value="60" { "1 min" }
                            option value="180" selected { "3 min" }
                            option value="300" { "5 min" }
                            option value="600" { "10 min" }
                        } }
                        button.btn type="submit" { "Create room" }
                    }
                }
            }
            div.card {
                h3 { "Join a game" }
                form method="post" action="/join" {
                    div.row {
                        input type="text" name="code" placeholder="Room code" maxlength="6" style="text-transform:uppercase";
                        input type="text" name="name" placeholder="Name (max 5)" maxlength="5";
                        button.btn type="submit" { "Join" }
                    }
                }
            }
        }
    })
}

pub fn lobby_page(code: &str, data: &RoomData, token: &str) -> Markup {
    shell("Lobby", html! {
        div.wrap {
            h1 { "Lobby" }
            p.muted { "Share this room code with friends:" }
            p { code style="font-size:22px;letter-spacing:3px" { (code) } }
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
            h3 { "Players (" (data.members.len()) "/4)" }
            div.players {
                @for m in &data.members {
                    div.pcard {
                        span.pdot style=(format!("background:{}", color_hex(m.color))) {}
                        div.pname { (m.name) }
                        @if data.is_host(&m.token) || m.is_bot {
                            div.pbadges {
                                @if data.is_host(&m.token) { span { "host" } }
                                @if m.is_bot { span { "bot" } }
                            }
                        }
                    }
                }
            }
        }
        div.row {
            @if is_host {
                form method="post" action=(format!("/room/{code}/add_bot")) {
                    button.btn.sec type="submit" disabled[data.members.len() >= 4] { "Add bot" }
                }
                form method="post" action=(format!("/room/{code}/start")) {
                    button.btn type="submit" disabled[!data.can_start()] {
                        "Start game (" (data.members.len()) "/4)"
                    }
                }
            } @else {
                p.muted { "Waiting for the host to start…" }
            }
        }
        @if data.members.len() < 2 {
            p.muted { "Need at least 2 players to start." }
        }
    }
}

// ---------------------------------------------------------------------------
// Game page + live fragments
// ---------------------------------------------------------------------------

pub fn error_page(msg: &str) -> Markup {
    shell("Catan", html! {
        div.wrap {
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
        div.wrap {
            div.card {
                h1 { "Join game " code { (code) } }
                form method="post" action=(format!("/room/{code}/join")) {
                    div.row {
                        input type="text" name="name" placeholder="Name (max 5)" maxlength="5";
                        button.btn type="submit" { "Join" }
                    }
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
}

pub fn fragments(game: &GameState, data: &RoomData, viewer: Option<PlayerId>) -> Fragments {
    Fragments {
        status: status_frag(game, data),
        board: board_frag(game, data, viewer),
        players: players_frag(game, viewer),
        hand: viewer.map(|v| hand_frag(game, &data.code, v)).unwrap_or_default(),
        controls: controls_frag(game, data, viewer),
        trades: trades_frag(game, &data.code, viewer),
        log: log_frag(game),
    }
}

pub fn game_page(code: &str, data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    let game = data.game.as_ref().expect("started game");
    let f = fragments(game, data, viewer);
    shell("Catan game", html! {
        div.wrap {
            div.turnbar {
                div { h2 style="margin:0" { "Catan" } span.muted { " room " code { (code) } } }
                div.muted { "You are " @if let Some(v)=viewer { b { (game.players[v].name) } } @else { "a spectator" } }
            }
            div id="app" hx-ext="sse"
                sse-connect=(format!("/room/{code}/events"))
                hx-swap="innerHTML" hx-indicator="#busy" {
                div #toasts {}
                div.card #players.section sse-swap="players" { (f.players) }
                div #status.section sse-swap="status" { (f.status) }
                div.game {
                    div {
                        div.card #controls sse-swap="controls" { (f.controls) }
                        div #board sse-swap="board" { (f.board) }
                        div.card #trades sse-swap="trades" { (f.trades) }
                    }
                    div {
                        div.card #hand sse-swap="hand" { (f.hand) }
                        div.card { h3 { "Build guide" } (build_guide()) }
                        div.card { h3 { "Log" } div #log.log sse-swap="log" { (f.log) } }
                    }
                }
                @if let Some(v) = viewer {
                    (trade_modal(game, data, v))
                }
            }
            span #busy.htmx-indicator.muted { "…" }
        }
    })
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

fn status_frag(game: &GameState, data: &RoomData) -> Markup {
    let cur = &game.players[game.current];
    html! {
        div.card {
            div.turnbar {
                div {
                    @match &game.phase {
                        Phase::Lobby => { "In lobby" }
                        Phase::Setup => { "Setup: " b { (cur.name) } " places" @if game.setup_road_from.is_some() { " a road" } @else { " a settlement" } }
                        Phase::Play => { b { (cur.name) } "'s turn" }
                        Phase::Discard => { "Players must discard" }
                        Phase::MoveRobber { .. } => { b { (cur.name) } " must move the robber" }
                        Phase::Steal { .. } => { b { (cur.name) } " must choose who to rob" }
                        Phase::GameOver => { span.win { (game.players[game.winner.unwrap_or(0)].name) " wins!" } }
                    }
                }
                div {
                    @if data.turn_seconds > 0 && data.turn_deadline_ms > 0
                        && matches!(game.phase, Phase::Play) {
                        span.turn-timer data-deadline=(data.turn_deadline_ms) { "⏱ " span data-secs { "" } }
                    }
                    @if let Some((a,b)) = game.dice {
                        span.dice title=(format!("{a} + {b}")) {
                            (dice_face(a)) (dice_face(b))
                            span.dice-total { "= " (a+b) }
                        }
                    }
                    @if (game.phase == Phase::Play || matches!(game.phase, Phase::MoveRobber{..}))
                        && robber_placed(&game.board, game) {
                        span.badge { "Robber: " (game.board.hexes[game.robber_hex].terrain.name()) }
                    }
                }
            }
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

    html! {
        svg viewBox=(format!("{:.2} {:.2} {:.2} {:.2}", minx, miny, w, h)) xmlns="http://www.w3.org/2000/svg" {
            // Hexes
            @for hex in &board.hexes {
                @let pts = hex_points(hex);
                polygon points=(pts) fill=(terrain_fill(hex.terrain)) stroke="#0b0f13" stroke-width="0.06" {}
                text x=(format!("{:.3}", hex.cx)) y=(format!("{:.3}", hex.cy - 0.28))
                    text-anchor="middle" font-size="0.34" opacity="0.75" { (terrain_glyph(hex.terrain)) }
                @if let Some(n) = hex.number {
                    circle cx=(format!("{:.3}", hex.cx)) cy=(format!("{:.3}", hex.cy + 0.05)) r="0.27" fill="#f4efe3" stroke="#b9ab8d" stroke-width="0.02" {}
                    text x=(format!("{:.3}", hex.cx)) y=(format!("{:.3}", hex.cy + 0.09))
                        text-anchor="middle" font-size="0.34" font-weight="700"
                        fill=(if n == 6 || n == 8 { "#c0392b" } else { "#2b2b2b" }) { (n) }
                    // Roll probability shown as pips: 6 - |n - 7|.
                    @let pips = 6 - (n as i32 - 7).abs();
                    @let start = -(pips as f64 - 1.0) / 2.0;
                    @let pip_fill = if n == 6 || n == 8 { "#c0392b" } else { "#6b5a3a" };
                    @for i in 0..pips {
                        circle cx=(format!("{:.3}", hex.cx + (start + i as f64) * 0.058)) cy=(format!("{:.3}", hex.cy + 0.245))
                            r="0.026" fill=(pip_fill) {}
                    }
                }
            }

            // Ports: a small pier drawn outward from the coast, with the
            // 2:1 / 3:1 label sitting on the sea just beyond it.
            @for p in port_marks(board) {
                line x1=(format!("{:.3}", p.ax)) y1=(format!("{:.3}", p.ay))
                    x2=(format!("{:.3}", p.ax + p.ux * 0.5)) y2=(format!("{:.3}", p.ay + p.uy * 0.5))
                    stroke="#e8d9a0" stroke-width="0.05" stroke-linecap="round" {}
                line x1=(format!("{:.3}", p.bx)) y1=(format!("{:.3}", p.by))
                    x2=(format!("{:.3}", p.bx + p.ux * 0.5)) y2=(format!("{:.3}", p.by + p.uy * 0.5))
                    stroke="#e8d9a0" stroke-width="0.05" stroke-linecap="round" {}
                line x1=(format!("{:.3}", p.ax + p.ux * 0.5)) y1=(format!("{:.3}", p.ay + p.uy * 0.5))
                    x2=(format!("{:.3}", p.bx + p.ux * 0.5)) y2=(format!("{:.3}", p.by + p.uy * 0.5))
                    stroke="#e8d9a0" stroke-width="0.05" stroke-linecap="round" {}
                circle cx=(format!("{:.3}", p.label_x)) cy=(format!("{:.3}", p.label_y)) r="0.42"
                    fill="#0d3a5c" opacity="0.85" stroke="#e8d9a0" stroke-width="0.02" {}
                text x=(format!("{:.3}", p.label_x)) y=(format!("{:.3}", p.label_y + 0.09))
                    text-anchor="middle" font-size="0.23" fill="#f4efe3" { (p.label) }
            }

            // Robber — only shown once it has actually been placed (moved off
            // the starting desert). Tucked into the hex corner so it never
            // covers the terrain glyph or the number/probability token.
            @if robber_placed(board, game) {
                @let rh = &board.hexes[game.robber_hex];
                @let rx = rh.cx + 0.5;
                @let ry = rh.cy + 0.46;
                circle cx=(format!("{rx:.3}")) cy=(format!("{ry:.3}")) r="0.2" fill="#101418" stroke="#000" stroke-width="0.035" {}
                text x=(format!("{rx:.3}")) y=(format!("{:.3}", ry + 0.075)) text-anchor="middle" font-size="0.26" { "🦹" }
            }

            // Roads
            @for e in &board.edges {
                @if let Some(owner) = e.owner {
                    @let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
                    @let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
                    line x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                        stroke=(color_hex(game.players[owner].color)) stroke-width="0.11" stroke-linecap="round" {}
                }
            }

            // Buildings
            @for v in &board.vertices {
                @if let (Some(owner), b) = (v.owner, v.building) {
                    @if b == Building::Settlement {
                        polygon points=(house_points(v.x, v.y, 0.17)) fill=(color_hex(game.players[owner].color)) stroke="#0b0f13" stroke-width="0.03" {}
                    } @else if b == Building::City {
                        polygon points=(house_points(v.x, v.y, 0.25)) fill=(color_hex(game.players[owner].color)) stroke="#0b0f13" stroke-width="0.045" {}
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
                            hx-post=(action_url) hx-vals=(format!("{{\"action\":\"place_vertex\",\"vertex\":\"{vi}\"}}")) {}
                        circle cx=(format!("{:.3}", v.x)) cy=(format!("{:.3}", v.y)) r="0.18" pointer-events="none"
                            fill="#ffffff55" stroke="#ffffff" stroke-width="0.03" stroke-dasharray="0.08 0.06" {}
                    }
                    Overlay::Edge(ei) => {
                        @let e = &board.edges[*ei];
                        @let (ax, ay) = (board.vertices[e.a].x, board.vertices[e.a].y);
                        @let (bx, by) = (board.vertices[e.b].x, board.vertices[e.b].y);
                        line.spot x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                            stroke="transparent" stroke-width="0.5" stroke-linecap="round"
                            hx-post=(action_url) hx-vals=(format!("{{\"action\":\"place_edge\",\"edge\":\"{ei}\"}}")) {}
                        line x1=(format!("{ax:.3}")) y1=(format!("{ay:.3}")) x2=(format!("{bx:.3}")) y2=(format!("{by:.3}"))
                            stroke="#ffffffaa" stroke-width="0.17" stroke-linecap="round" pointer-events="none" {}
                    }
                    Overlay::Hex(hi) => {
                        @let hex = &board.hexes[*hi];
                        polygon.spot points=(hex_points(hex)) fill="#ff000033" stroke="#ff6b6b" stroke-width="0.05"
                            hx-post=(action_url) hx-vals=(format!("{{\"action\":\"move_robber\",\"hex\":\"{hi}\"}}")) {}
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
    let m = 1.7;
    (minx - m, miny - m, (maxx - minx) + 2.0 * m, (maxy - miny) + 2.0 * m)
}

fn hex_points(hex: &crate::game::board::Hex) -> String {
    // Corners are the six vertices; but we only have their ids in board, not here.
    // Recompute from center using pointy-top geometry (size 1).
    let mut pts = String::new();
    for i in 0..6 {
        let ang = (60.0 * i as f64 - 30.0f64).to_radians();
        let x = hex.cx + ang.cos();
        let y = hex.cy + ang.sin();
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
    label: String,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    ux: f64,
    uy: f64,
    label_x: f64,
    label_y: f64,
}

fn port_label(kind: PortKind) -> String {
    match kind.resource() {
        Some(r) => format!("2:1 {}", resource_glyph(r)),
        None => "3:1 ⛵".to_string(),
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
        // Outward unit vector points from the board centre toward the coast.
        let len = (mx * mx + my * my).sqrt().max(0.001);
        let (ux, uy) = (mx / len, my / len);
        // Label sits on the sea, safely beyond the coastal vertices.
        let label_r = len + 1.0;
        out.push(PortMark {
            label: port_label(pa),
            ax,
            ay,
            bx,
            by,
            ux,
            uy,
            label_x: ux * label_r,
            label_y: uy * label_r,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

fn players_frag(game: &GameState, viewer: Option<PlayerId>) -> Markup {
    html! {
        h3 { "Players" }
        div.players {
            @for p in &game.players {
                @let is_me = Some(p.id) == viewer;
                @let is_turn = game.current == p.id;
                div.pcard.me[is_me].turn[is_turn] {
                    span.pdot style=(format!("background:{}", color_hex(p.color))) {}
                    div.pname title=(p.name.as_str()) { (p.name) }
                    div.pstats {
                        span title="Victory points" { "🏆" (game.public_victory_points(p.id)) }
                        span title="Resource cards" { "🎴" (p.resources.total()) }
                        span title="Development cards" { "📜" (p.all_dev_cards().count()) }
                        span title="Knights played" { "⚔" (p.played_knights) }
                    }
                    @if game.longest_road == Some(p.id) || game.largest_army == Some(p.id) {
                        div.pbadges {
                            @if game.longest_road == Some(p.id) { span title="Longest Road" { "🛣" } }
                            @if game.largest_army == Some(p.id) { span title="Largest Army" { "⚔" } }
                        }
                    }
                }
            }
        }
    }
}

fn hand_frag(game: &GameState, code: &str, v: PlayerId) -> Markup {
    let _ = code;
    let p = &game.players[v];
    html! {
        h3 { "Your hand" }
        div.hand {
            @for r in ALL_RESOURCES {
                @let n = p.resources.get(r);
                div.rcard.zero[n == 0] {
                    span.rcard-count { (n) }
                    span.rcard-icon { (resource_glyph(r)) }
                    span.rcard-name { (r.name()) }
                }
            }
        }
        @if !p.dev_cards.is_empty() || !p.new_dev_cards.is_empty() {
            h3 style="margin-top:8px" { "Development cards" }
            div {
                @for card in &p.dev_cards {
                    span.chip { (card.name()) }
                }
                @for card in &p.new_dev_cards {
                    span.chip.muted { (card.name()) " (new)" }
                }
            }
        }
    }
}

fn controls_frag(game: &GameState, data: &RoomData, viewer: Option<PlayerId>) -> Markup {
    let Some(v) = viewer else {
        return html! { p.muted { "Spectating." } };
    };
    let my_turn = game.current == v;
    html! {
        @match &game.phase {
            Phase::Setup => {
                @if my_turn {
                    p { "Place your " @if game.setup_road_from.is_some() { "road (click a highlighted edge)." } @else { "settlement (click a highlighted intersection)." } }
                } @else {
                    p.muted { "Waiting for " (game.players[game.current].name) " to place." }
                }
            }
            Phase::Discard => {
                @if game.pending_discards.contains(&v) {
                    (discard_form(game, &data.code, v))
                } @else {
                    p.muted { "Waiting for other players to discard." }
                }
            }
            Phase::MoveRobber { .. } => {
                @if my_turn { p { "Click a highlighted hex to move the robber." } }
                @else { p.muted { "Waiting for the robber to move." } }
            }
            Phase::Steal { hex } => {
                @if my_turn {
                    p { "Choose a player to steal from:" }
                    div.row {
                        @for c in game.steal_candidates(*hex, v) {
                            button.btn hx-post=(format!("/room/{}/action", data.code)) hx-vals=(format!("{{\"action\":\"steal\",\"player\":\"{c}\"}}")) { (game.players[c].name) }
                        }
                    }
                } @else { p.muted { "Waiting…" } }
            }
            Phase::GameOver => {
                p.win { "Game over — " (game.players[game.winner.unwrap_or(0)].name) " wins!" }
            }
            Phase::Lobby => { p.muted { "Starting…" } }
            Phase::Play => {
                @if !my_turn {
                    p.muted { "Waiting for " (game.players[game.current].name) "…" }
                } @else {
                    (play_controls(game, data, v))
                }
            }
        }
    }
}

fn play_controls(game: &GameState, data: &RoomData, v: PlayerId) -> Markup {
    let url = format!("/room/{}/action", data.code);
    let mode_url = format!("/room/{}/mode", data.code);
    let mode = data.members.get(v).map(|m| m.mode).unwrap_or_default();
    let rolled = game.dice.is_some();
    let p = &game.players[v];
    let can_play_dev = !game.played_dev_this_turn;
    html! {
        div.row {
            @if !rolled {
                button.btn.icon title="Roll dice" hx-post=(url) hx-vals="{\"action\":\"roll\"}" { "🎲" }
            }
            button.btn.icon.sec title="Build road" disabled[mode != ViewMode::Normal || !COST_ROAD.can_pay(&p.resources) || p.roads_left == 0]
                hx-post=(mode_url) hx-vals="{\"mode\":\"road\"}" { "🛣" }
            button.btn.icon.sec title="Build settlement" disabled[mode != ViewMode::Normal || !COST_SETTLEMENT.can_pay(&p.resources) || p.settlements_left == 0]
                hx-post=(mode_url) hx-vals="{\"mode\":\"settlement\"}" { "🏠" }
            button.btn.icon.sec title="Build city" disabled[mode != ViewMode::Normal || !COST_CITY.can_pay(&p.resources) || p.cities_left == 0]
                hx-post=(mode_url) hx-vals="{\"mode\":\"city\"}" { "🏙" }
            button.btn.icon.sec title="Buy development card" disabled[game.dev_deck.is_empty() || !COST_DEV.can_pay(&p.resources)] hx-post=(url) hx-vals="{\"action\":\"buy_dev\"}" { "🃏" }
            @if mode != ViewMode::Normal {
                button.btn.icon.warn title="Cancel" hx-post=(mode_url) hx-vals="{\"mode\":\"normal\"}" { "✖" }
            }
            button.btn.icon title="End turn" disabled[!rolled] hx-post=(url) hx-vals="{\"action\":\"end_turn\"}" { "▶" }
        }
        (dev_play_controls(game, data, v, can_play_dev))
        @if game.played_dev_this_turn { p.muted { "You have already played a development card this turn." } }
        @if p.resources.total() > 0 && game.trade.is_none() {
            button.btn.icon.sec type="button" data-open-trade title="Trade" { "⇄" }
        }
    }
}

fn dev_play_controls(game: &GameState, data: &RoomData, v: PlayerId, can_play: bool) -> Markup {
    let p = &game.players[v];
    if !can_play || p.dev_cards.is_empty() {
        return Markup::default();
    }
    let url = format!("/room/{}/action", data.code);
    let has = |c: DevCard| p.dev_cards.contains(&c);
    html! {
        div.row style="margin-top:8px" {
            span.muted { "Play:" }
            @if has(DevCard::Knight) {
                button.btn.icon.sec title="Play Knight" hx-post=(url) hx-vals="{\"action\":\"play_knight\"}" { "⚔" }
            }
            @if has(DevCard::RoadBuilding) {
                button.btn.icon.sec title="Play Road Building" hx-post=(url) hx-vals="{\"action\":\"play_road_building\"}" { "🚧" }
            }
            @if has(DevCard::YearOfPlenty) {
                form.row hx-post=(url) style="display:inline-flex;gap:6px;align-items:center" {
                    input type="hidden" name="action" value="play_yop";
                    select name="first" title="First resource" { @for r in ALL_RESOURCES { option value=(r.slug()) { (resource_glyph(r)) " " (r.name()) } } }
                    select name="second" title="Second resource" { @for r in ALL_RESOURCES { option value=(r.slug()) { (resource_glyph(r)) " " (r.name()) } } }
                    button.btn.icon.sec type="submit" title="Play Year of Plenty" { "🌾" }
                }
            }
            @if has(DevCard::Monopoly) {
                form.row hx-post=(url) style="display:inline-flex;gap:6px;align-items:center" {
                    input type="hidden" name="action" value="play_monopoly";
                    select name="resource" title="Resource" { @for r in ALL_RESOURCES { option value=(r.slug()) { (resource_glyph(r)) " " (r.name()) } } }
                    button.btn.icon.sec type="submit" title="Play Monopoly" { "💰" }
                }
            }
        }
    }
}

fn discard_form(game: &GameState, code: &str, v: PlayerId) -> Markup {
    let p = &game.players[v];
    let need = p.resources.total() / 2;
    html! {
        p { "You have " (p.resources.total()) " cards and must discard " b { (need) } "." }
        form hx-post=(format!("/room/{code}/action")) {
            input type="hidden" name="action" value="discard";
            div.row {
                @for r in ALL_RESOURCES {
                    label title=(r.name()) { (resource_glyph(r)) " " input type="number" name=(r.slug()) min="0" max=(p.resources.get(r)) value="0" style="width:64px"; }
                }
            }
            div.row { button.btn.warn type="submit" title="Discard" { "🗑 " (need) } }
        }
    }
}

fn pick_cards(group: &str) -> Markup {
    html! {
        @for r in ALL_RESOURCES {
            button.rcard.pick type="button" data-res=(r.slug()) data-group=(group) title=(r.name()) {
                span.pick-count data-count="0" { "0" }
                span.rcard-icon { (resource_glyph(r)) }
                span.rcard-name { (r.name()) }
            }
        }
    }
}

fn trade_modal(_game: &GameState, data: &RoomData, _v: PlayerId) -> Markup {
    let url = format!("/room/{}/action", data.code);
    html! {
        div #trade-modal.modal hidden {
            div.modal-card {
                div.trade-head {
                    h3 { "⇄ Trade" }
                    button.btn.icon.sec type="button" data-close-trade title="Close" { "✖" }
                }
                p.muted.small { "Tap cards to build your offer. Shared with everyone — first to accept trades. Ends in 30s or when all decline." }
                strong { "You give" }
                div.picker { (pick_cards("give")) }
                strong { "You want" }
                div.picker { (pick_cards("want")) }
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
                        button #offer-btn.btn type="submit" title="Offer to all players" disabled { "📨 Offer" }
                        button #bank-btn.btn.sec type="submit" title="Exchange with bank / port" disabled { "🔁 Bank" }
                    }
                }
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
        div.card.trade-card data-deadline=(o.deadline_ms) {
            div.trade-head {
                h3 { "⇄ Trade offer" }
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
                button.btn.icon.warn title="Withdraw offer" hx-post=(url) hx-vals="{\"action\":\"cancel_trade\"}" { "✖" }
            } @else if viewer.is_some() {
                div.row {
                    button.btn title="Accept (first to accept trades)" hx-post=(url) hx-vals="{\"action\":\"respond_trade\",\"accept\":\"1\"}" { "✔ Accept" }
                    button.btn.sec.icon title="Decline" hx-post=(url) hx-vals="{\"action\":\"respond_trade\",\"accept\":\"0\"}" { "✖ Decline" }
                }
            } @else {
                p.muted { "Waiting for players to respond." }
            }
        }
    }
}

fn log_frag(game: &GameState) -> Markup {
    html! {
        @for entry in game.log.iter().rev().take(40) {
            div { (entry.text) }
        }
    }
}

fn bundle_text(b: &Bundle) -> String {
    let parts: Vec<String> = b
        .iter_nonzero()
        .map(|(r, n)| format!("{n} {}", r.name()))
        .collect();
    if parts.is_empty() {
        "nothing".to_string()
    } else {
        parts.join(", ")
    }
}

// ---------------------------------------------------------------------------
// Build guide
// ---------------------------------------------------------------------------

fn resource_glyph(r: Resource) -> &'static str {
    match r {
        Resource::Wood => "🌲",
        Resource::Brick => "🧱",
        Resource::Wheat => "🌾",
        Resource::Ore => "🪨",
        Resource::Sheep => "🐑",
    }
}

/// A small SVG die face (1–6), drawn so dice look the same on every platform.
fn dice_face(n: u8) -> Markup {
    let pip = |x: i32, y: i32| -> Markup {
        html! { circle cx=(x) cy=(y) r="2.05" fill="#1c2430" {} }
    };
    html! {
        svg.die viewBox="0 0 20 20" width="30" height="30" aria-hidden="true" {
            rect x="1" y="1" width="18" height="18" rx="4"
                fill="#f4efe3" stroke="#b9ab8d" stroke-width="0.9" {}
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

fn cost_span(cost: Bundle) -> Markup {
    html! {
        span.cost {
            @for (r, n) in cost.iter_nonzero() {
                span.cost-item title=(r.name()) {
                    (resource_glyph(r))
                    @if n > 1 { span.cnt { (n) } }
                }
            }
        }
    }
}

fn guide_row(icon: &str, name: &str, cost: Bundle) -> Markup {
    html! {
        div.guide-row {
            span.guide-icon { (icon) }
            span.guide-name { (name) }
            (cost_span(cost))
        }
    }
}

fn build_guide() -> Markup {
    html! {
        div.guide {
            (guide_row("🛣", "Road", COST_ROAD))
            (guide_row("🏠", "Settlement", COST_SETTLEMENT))
            (guide_row("🏙", "City", COST_CITY))
            (guide_row("🃏", "Dev card", COST_DEV))
        }
    }
}

use crate::game::actions::Action;
use crate::game::resources::{Bundle, Resource};
use crate::game::state::{BotLevel, GameState};
use crate::render;
use crate::state::{AppState, JoinError, Layout, Room, SeatOutcome, ViewMode, now_ms};
use axum::{
    extract::{Form, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response, Sse, sse::{Event, KeepAlive}},
    routing::{get, post},
    Router,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::WatchStream;

pub fn router(app: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/how-to-play", get(how_to_play))
        .route("/privacy", get(privacy))
        .route("/create", get(create_form).post(create))
        .route("/join", get(join_form).post(join))
        .route("/room/{code}", get(room_page))
        .route("/room/{code}/join", post(join_room))
        .route("/room/{code}/events", get(events))
        .route("/room/{code}/action", post(action))
        .route("/room/{code}/mode", post(set_mode))
        .route("/room/{code}/layout", post(set_layout))
        .route("/room/{code}/start", post(start))
        .route("/room/{code}/pause", post(pause))
        .route("/room/{code}/resume", post(resume))
        .route("/room/{code}/replay", get(replay))
        .route("/room/{code}/add_bot", post(add_bot))
        .route("/room/{code}/remove", post(remove_player))
        .route("/room/{code}/leave", post(leave_player))
        .route("/healthz", get(|| async { "ok" }))
        // Served from the root so the worker's scope covers the whole origin.
        .route("/sw.js", get(service_worker))
        // Digital Asset Links for the Android TWA (see docs/PLAYSTORE-ANDROID.md).
        .route("/.well-known/assetlinks.json", get(assetlinks))
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .with_state(app)
}

/// The service worker, embedded and served from the origin root so its scope
/// covers every page (enables PWA installability).
async fn service_worker() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            // Revalidate the worker on every navigation so a new deploy's worker
            // (and its cache version) is picked up immediately instead of being
            // pinned for the browser's default ~24h worker cache.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("../static/sw.js"),
    )
}

/// Package name of the Android Trusted Web Activity (TWA) built with Bubblewrap.
const ANDROID_PACKAGE_NAME: &str = "org.playcatanou.twa";

/// SHA-256 fingerprints accepted for the Android TWA `org.playcatanou.twa`.
///
/// Both are published so the app verifies regardless of how it was installed:
/// - the **upload key** that signs our release `.aab` (used for local/side-loaded
///   install testing), and
/// - the **Play App Signing** certificate that Play re-signs the delivered app
///   with (what devices actually see for a Play install; Play Console ->
///   Test and release -> App integrity -> App signing key certificate).
///
/// Override at runtime with the comma-separated `CATAN_ANDROID_SHA256_FINGERPRINTS`
/// environment variable (it replaces this list), e.g. to rotate the Play
/// certificate without a code change.
const DEFAULT_SHA256_FINGERPRINTS: &[&str] = &[
    "B3:A7:69:04:97:60:66:AB:68:07:26:A3:2D:01:46:07:B0:2C:F0:40:81:9C:B1:13:AD:7B:50:A1:60:50:C3:59",
    "F2:62:F8:12:66:14:0E:4B:97:29:DF:62:7D:41:BD:3D:41:28:BF:D8:9A:ED:FA:2C:C8:5B:71:65:51:43:9C:ED",
];

/// Digital Asset Links statement that verifies the Android TWA against this origin.
async fn assetlinks() -> impl IntoResponse {
    let fingerprints: Vec<String> = std::env::var("CATAN_ANDROID_SHA256_FINGERPRINTS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_SHA256_FINGERPRINTS
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    let statement = serde_json::json!([{
        "relation": ["delegate_permission/common.handle_all_urls"],
        "target": {
            "namespace": "android_app",
            "package_name": ANDROID_PACKAGE_NAME,
            "sha256_cert_fingerprints": fingerprints,
        }
    }]);
    (
        [(header::CONTENT_TYPE, "application/json")],
        statement.to_string(),
    )
}

// ---------------------------------------------------------------------------
// Cookie helpers
// ---------------------------------------------------------------------------

fn cookie_name(code: &str) -> String {
    format!("catan_{code}")
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    let needle = format!("{name}=");
    raw.split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&needle).map(|v| v.to_string()))
}

fn set_cookie_header(code: &str, token: &str) -> (header::HeaderName, String) {
    (
        header::SET_COOKIE,
        format!(
            "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000",
            cookie_name(code)
        ),
    )
}

/// Drop the room's cookie so a tab that is still open stops presenting it.
fn clear_cookie_header(code: &str) -> (header::HeaderName, String) {
    (
        header::SET_COOKIE,
        format!("{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0", cookie_name(code)),
    )
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

async fn home() -> Html<String> {
    Html(render::home_page().into_string())
}

async fn how_to_play(State(app): State<Arc<AppState>>, Query(q): Query<HowToPlayQuery>) -> Html<String> {
    let back = live_room(&app, &q.room);
    Html(render::how_to_play_page(back.as_deref()).into_string())
}

/// The room a reader came from, so the guide can offer a way back to a game in
/// progress. Normalised against the live room table: an empty, stale, or
/// hand-typed code yields `None`, and the guide shows the Home path only
/// rather than a link to a room that no longer exists.
fn live_room(app: &AppState, raw: &str) -> Option<String> {
    let code = raw.trim().to_uppercase();
    (!code.is_empty() && app.room(&code).is_some()).then_some(code)
}

/// Query for `/how-to-play`: the optional `?room=` of the game being left.
#[derive(serde::Deserialize)]
struct HowToPlayQuery {
    #[serde(default)]
    room: String,
}

/// Public privacy policy for the Play Store listing. Required by Google Play
/// and must stay reachable without login; see issue #14 / OMK-55.
async fn privacy() -> Html<String> {
    Html(render::privacy_page().into_string())
}

async fn create_form() -> Html<String> {
    Html(render::create_page(None).into_string())
}

async fn join_form() -> Html<String> {
    Html(render::join_room_page(None).into_string())
}

#[derive(serde::Deserialize)]
struct NameForm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    turn_seconds: u64,
    /// Present (value "on") only when the create form's checkbox is checked;
    /// unset means the room does not allow pausing.
    #[serde(default)]
    allow_pause: Option<String>,
}

#[derive(serde::Deserialize)]
struct JoinForm {
    #[serde(default)]
    code: String,
    #[serde(default)]
    name: String,
}

async fn create(State(app): State<Arc<AppState>>, Form(form): Form<NameForm>) -> Response {
    let turn_seconds = match form.turn_seconds {
        60 | 180 | 300 | 600 => form.turn_seconds,
        _ => 180,
    };
    let allow_pause = form.allow_pause.is_some();
    match app.create_room(form.name, turn_seconds, allow_pause) {
        Ok((room, token)) => {
            let code = crate::state::room_code(&room);
            let (h, v) = set_cookie_header(&code, &token);
            ([(h, v)], Redirect::to(&format!("/room/{code}"))).into_response()
        }
        Err(e) => Html(render::create_page(Some(join_error(&e))).into_string()).into_response(),
    }
}

async fn join(State(app): State<Arc<AppState>>, Form(form): Form<JoinForm>) -> Response {
    let code = form.code.trim().to_uppercase();
    match app.join(&code, form.name) {
        Ok((room, token)) => {
            let code = crate::state::room_code(&room);
            let (h, v) = set_cookie_header(&code, &token);
            ([(h, v)], Redirect::to(&format!("/room/{code}"))).into_response()
        }
        Err(e) => Html(render::join_room_page(Some(join_error(&e))).into_string()).into_response(),
    }
}

async fn join_room(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    Form(form): Form<NameForm>,
) -> Response {
    match app.join(&code, form.name) {
        Ok((room, token)) => {
            let (h, v) = set_cookie_header(&code, &token);
            let _ = room;
            ([(h, v)], Redirect::to(&format!("/room/{code}"))).into_response()
        }
        Err(e) => Html(render::error_page(join_error(&e)).into_string()).into_response(),
    }
}

fn join_error(e: &JoinError) -> &'static str {
    match e {
        JoinError::NotFound => "That room code does not exist.",
        JoinError::Full => "That room is already full.",
        JoinError::AlreadyStarted => "That game has already started.",
        JoinError::NoName => "Please enter a name.",
        JoinError::NameTooLong => "Names can be at most 5 characters.",
        JoinError::NameTaken => "That name is already taken in this room.",
    }
}

async fn room_page(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return Html(render::error_page("That room code does not exist.").into_string())
            .into_response();
    };
    let token = cookie_value(&headers, &cookie_name(&code));
    let mut data = room.data.lock().unwrap();
    let viewer = token.as_deref().and_then(|t| data.member_index(t));
    if let Some(v) = viewer {
        data.members[v].connected = true;
        data.members[v].last_seen_ms = now_ms();
    }
    let html = match viewer {
        Some(v) => {
            if data.started {
                render::game_page(&code, &data, Some(v)).into_string()
            } else {
                render::lobby_page(&code, &data, token.as_deref().unwrap_or("")).into_string()
            }
        }
        None => {
            if data.started {
                render::error_page("That game has already started.").into_string()
            } else {
                render::join_page(&code).into_string()
            }
        }
    };
    Html(html).into_response()
}

// ---------------------------------------------------------------------------
// SSE
// ---------------------------------------------------------------------------

async fn events(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let token = cookie_value(&headers, &cookie_name(&code));
    let Some(token) = token else {
        return StatusCode::FORBIDDEN.into_response();
    };
    {
        let mut data = room.data.lock().unwrap();
        let Some(v) = data.member_index(&token) else {
            return StatusCode::FORBIDDEN.into_response();
        };
        data.members[v].connected = true;
        data.members[v].last_seen_ms = now_ms();
    }

    let rx = room.subscribe();
    let app2 = app.clone();
    let code2 = code.clone();
    let token2 = token.clone();
    let stream = WatchStream::new(rx)
        .map(move |_version| {
            let events = render_events(&app2, &code2, &token2);
            futures::stream::iter(events)
        })
        .flatten();

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

fn render_events(app: &AppState, code: &str, token: &str) -> Vec<Result<Event, Infallible>> {
    let Some(room) = app.room(code) else {
        return Vec::new();
    };
    let mut data = room.data.lock().unwrap();
    let Some(v) = data.member_index(token) else {
        // Our session stopped working — we left, or the host removed us. Tell
        // this browser to go home; the next real page load lands there anyway
        // because the token no longer maps to a seat.
        return vec![Ok(Event::default().event("gone").data("/"))];
    };
    data.members[v].last_seen_ms = now_ms();
    let mk = |name: &str, m: maud::Markup| {
        // An SSE event with an empty data buffer is never dispatched by the
        // browser, so an empty fragment (e.g. no active trade) would leave the
        // target stale. Send a single space instead so the swap still happens.
        let body = m.into_string();
        let body = if body.is_empty() { " ".to_string() } else { body };
        Ok(Event::default().event(name).data(body))
    };
    let Some(game) = data.game.as_ref() else {
        // Still in the lobby: push the lobby fragment so joins/leaves show up.
        return vec![mk("lobby", render::lobby_frag(code, &data, token))];
    };
    let f = render::fragments(game, &data, Some(v));
    vec![
        mk("status", f.status),
        mk("board", f.board),
        mk("players", f.players),
        mk("hand", f.hand),
        mk("controls", f.controls),
        mk("trades", f.trades),
        mk("trade_modal", f.trade_modal),
        mk("log", f.log),
        mk("turn", f.turn),
        // Any lobby viewer still waiting should drop into the game now.
        Ok(Event::default().event("started").data(format!("/room/{code}"))),
    ]
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

async fn action(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Your session expired. Reload the page.");
    };

    let mut pending_expiry: Option<(u64, u64)> = None;
    let outcome: Result<(), String> = {
        let mut data = room.data.lock().unwrap();
        let Some(v) = data.member_index(&token) else {
            return toast("You are not a member of this game.");
        };
        data.members[v].last_seen_ms = now_ms();
        if data.game.is_none() {
            return toast("The game has not started yet.");
        }
        // A paused room rejects every game action, so a client holding a stale
        // (pre-pause) fragment cannot mutate state while blinded.
        if data.paused {
            return toast("The game is paused.");
        }
        let mode = data.members[v].mode;
        let game = data.game.as_mut().unwrap();
        match parse_action(&form, game, v, mode) {
            Ok(act) => match game.apply(v, &act) {
                Ok(()) => {
                    // Reset build mode after a successful placement, but keep
                    // road placement active while free roads from a Road
                    // Building card remain to be placed.
                    let keep = matches!(act, Action::PlaceRoad { .. } | Action::BuildRoad { .. })
                        && game.free_roads_left > 0;
                    let reset_mode = matches!(
                        act,
                        Action::PlaceRoad { .. }
                            | Action::PlaceSettlement { .. }
                            | Action::BuildRoad { .. }
                            | Action::BuildSettlement { .. }
                            | Action::BuildCity { .. }
                    ) && !keep;
                    // Stamp the shared trade offer with its 30s deadline so the
                    // scheduled task below can expire exactly this offer.
                    if matches!(act, Action::ProposeTrade { .. }) {
                        if let Some(t) = game.trade.as_mut() {
                            let deadline = now_ms() + TRADE_SECONDS * 1000;
                            t.deadline_ms = deadline;
                            pending_expiry = Some((t.id, deadline));
                        }
                    }
                    if reset_mode {
                        data.members[v].mode = ViewMode::Normal;
                    }
                    // Playing Road Building drops the player straight into road
                    // placement, so the free roads are placed with the normal
                    // board-highlight flow instead of them having to find the
                    // build button first.
                    if matches!(act, Action::PlayRoadBuilding) {
                        data.members[v].mode = ViewMode::PlaceRoad;
                    }
                    data.last_activity_ms = now_ms();
                    Ok(())
                }
                Err(e) => Err(e.0),
            },
            Err(e) => Err(e),
        }
    };

    match outcome {
        Ok(()) => {
            room.bump();
            app.persist(&room);
            if let Some((id, deadline)) = pending_expiry {
                spawn_trade_expiry(room.clone(), app.clone(), id, deadline);
            }
            // The board/controls themselves arrive over SSE; this response
            // only clears a stale error toast out of band.
            cleared()
        }
        Err(msg) => toast(&msg),
    }
}

/// Seconds a shared trade offer stays open before it expires.
const TRADE_SECONDS: u64 = 30;

/// Expire a shared trade offer after its deadline unless it already ended.
fn spawn_trade_expiry(room: Arc<Room>, app: Arc<AppState>, id: u64, deadline_ms: u64) {
    tokio::spawn(async move {
        let now = now_ms();
        let wait_ms = deadline_ms.saturating_sub(now);
        tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        let changed = {
            let mut data = room.data.lock().unwrap();
            if let Some(game) = data.game.as_mut() {
                game.expire_trade(id)
            } else {
                false
            }
        };
        if changed {
            room.bump();
            app.persist(&room);
        }
    });
}

fn parse_action(
    form: &HashMap<String, String>,
    game: &crate::game::state::GameState,
    _v: usize,
    mode: ViewMode,
) -> Result<Action, String> {
    let get = |k: &str| form.get(k).map(|s| s.as_str()).unwrap_or("");
    let num = |k: &str| get(k).parse::<usize>().map_err(|_| format!("missing {k}"));
    let action = get("action");

    let resource = |k: &str| {
        Resource::from_slug(get(k)).ok_or_else(|| format!("unknown resource in {k}"))
    };

    Ok(match action {
        "roll" => Action::RollDice,
        "end_turn" => Action::EndTurn,
        "buy_dev" => Action::BuyDevCard,
        "play_knight" => Action::PlayKnight,
        "play_road_building" => Action::PlayRoadBuilding,
        "play_yop" => Action::PlayYearOfPlenty {
            first: resource("first")?,
            second: resource("second")?,
        },
        "play_monopoly" => Action::PlayMonopoly {
            resource: resource("resource")?,
        },
        "reveal_vp" => Action::RevealVictoryPoint,
        "move_robber" => Action::MoveRobber { hex: num("hex")? },
        "steal" => {
            let p = get("player");
            Action::StealFrom {
                player: if p.is_empty() || p == "none" {
                    None
                } else {
                    Some(p.parse().map_err(|_| "bad player".to_string())?)
                },
            }
        }
        "discard" => {
            let mut b = Bundle::default();
            for r in [
                Resource::Wood,
                Resource::Brick,
                Resource::Wheat,
                Resource::Ore,
                Resource::Sheep,
            ] {
                let n: u8 = get(r.slug()).parse().unwrap_or(0);
                b = b.with(r, n);
            }
            Action::Discard { resources: b }
        }
        "bank_trade" => Action::BankTrade {
            give: resource("give")?,
            want: resource("want")?,
        },
        "propose_trade" => {
            let mut give = Bundle::default();
            let mut want = Bundle::default();
            for r in [
                Resource::Wood,
                Resource::Brick,
                Resource::Wheat,
                Resource::Ore,
                Resource::Sheep,
            ] {
                let g: u8 = get(&format!("give_{}", r.slug())).parse().unwrap_or(0);
                let w: u8 = get(&format!("want_{}", r.slug())).parse().unwrap_or(0);
                give = give.with(r, g);
                want = want.with(r, w);
            }
            Action::ProposeTrade { give, want }
        }
        "respond_trade" => Action::RespondTrade {
            accept: get("accept") == "1",
        },
        "cancel_trade" => Action::CancelTrade,
        "place_vertex" => {
            let vertex = num("vertex")?;
            if matches!(game.phase, crate::game::state::Phase::Setup) {
                Action::PlaceSettlement { vertex }
            } else {
                match mode {
                    ViewMode::PlaceCity => Action::BuildCity { vertex },
                    _ => Action::BuildSettlement { vertex },
                }
            }
        }
        "place_edge" => {
            let edge = num("edge")?;
            if matches!(game.phase, crate::game::state::Phase::Setup) {
                Action::PlaceRoad { edge }
            } else {
                Action::BuildRoad { edge }
            }
        }
        other => return Err(format!("unknown action '{other}'")),
    })
}

async fn set_mode(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Session expired.");
    };
    let mode = match form.get("mode").map(String::as_str) {
        Some("road") => ViewMode::PlaceRoad,
        Some("settlement") => ViewMode::PlaceSettlement,
        Some("city") => ViewMode::PlaceCity,
        _ => ViewMode::Normal,
    };
    {
        let mut data = room.data.lock().unwrap();
        let Some(v) = data.member_index(&token) else {
            return toast("Not a member.");
        };
        data.members[v].mode = mode;
        data.members[v].last_seen_ms = now_ms();
    }
    room.bump();
    StatusCode::NO_CONTENT.into_response()
}

async fn set_layout(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Session expired.");
    };
    let layout = match form.get("layout").map(String::as_str) {
        Some("classic") => Layout::Classic,
        _ => Layout::Board,
    };
    {
        let mut data = room.data.lock().unwrap();
        let Some(v) = data.member_index(&token) else {
            return toast("Not a member.");
        };
        data.members[v].layout = layout;
        data.members[v].last_seen_ms = now_ms();
    }
    room.bump();
    StatusCode::NO_CONTENT.into_response()
}

async fn start(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return Html(render::error_page("Room not found.").into_string()).into_response();
    };
    let token = cookie_value(&headers, &cookie_name(&code)).unwrap_or_default();
    match app.start_game(&room, &token) {
        Ok(()) => Redirect::to(&format!("/room/{code}")).into_response(),
        Err(e) => Html(render::error_page(&e).into_string()).into_response(),
    }
}

async fn pause(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Your session expired. Reload the page.");
    };
    match app.pause_room(&room, &token) {
        // Room state changes ride the SSE stream; no body needed.
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(msg) => toast(&msg),
    }
}

async fn resume(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Your session expired. Reload the page.");
    };
    match app.resume_room(&room, &token) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(msg) => toast(&msg),
    }
}

/// Query for `/room/{code}/replay`: the move index to show. Omitted or past the
/// end means "show the final position".
#[derive(serde::Deserialize)]
struct ReplayQuery {
    #[serde(default)]
    n: Option<usize>,
}

/// End-of-game move-history viewer (GitHub #13). Reconstructs a read-only,
/// spectator board by replaying the recorded moves up to `n`. Only available
/// once the game is over: at that point a full reveal is already acceptable, so
/// no hidden hand is leaked while a game is still live.
async fn replay(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Query(q): Query<ReplayQuery>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return Html(render::error_page("That room code does not exist.").into_string())
            .into_response();
    };
    let token = cookie_value(&headers, &cookie_name(&code));
    let data = room.data.lock().unwrap();
    let Some(v) = token.as_deref().and_then(|t| data.member_index(t)) else {
        return Html(render::error_page("You are not a member of this game.").into_string())
            .into_response();
    };
    let Some(game) = data.game.as_ref() else {
        return Html(render::error_page("The game has not started yet.").into_string())
            .into_response();
    };
    if !game.is_over() {
        return Html(
            render::error_page("The replay is available once the game is over.").into_string(),
        )
        .into_response();
    }
    let Some(base) = data.initial_game.as_ref() else {
        return Html(render::error_page("This game has no replay data.").into_string())
            .into_response();
    };
    let total = game.moves.len();
    let n = q.n.unwrap_or(total).min(total);
    let view = GameState::replay_to(base, &game.moves, n);
    Html(render::replay_page(&code, &data, &view, n, total, Some(v)).into_string()).into_response()
}

async fn add_bot(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return Html(render::error_page("Room not found.").into_string()).into_response();
    };
    let token = cookie_value(&headers, &cookie_name(&code)).unwrap_or_default();
    let level = form
        .get("level")
        .and_then(|s| BotLevel::from_slug(s))
        .unwrap_or_default();
    {
        let mut data = room.data.lock().unwrap();
        if !data.is_host(&token) {
            return Html(render::error_page("Only the host can add bots.").into_string())
                .into_response();
        }
        data.add_bot(level);
        data.last_activity_ms = now_ms();
    }
    room.bump();
    app.persist(&room);
    Redirect::to(&format!("/room/{code}")).into_response()
}

/// Host-only: drop another seat. In the lobby the slot is freed; once the game
/// is running it ends for everyone in the room.
async fn remove_player(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Your session expired. Reload the page.");
    };
    let Some(target) = form.get("player").and_then(|p| p.parse::<usize>().ok()) else {
        return toast("Which player? Reload the page.");
    };
    let outcome = {
        let mut data = room.data.lock().unwrap();
        data.remove_player(&token, target)
    };
    // An empty 200 clears the toast area; the bumped room pushes the new lobby
    // fragment / game-over state over SSE, so no reload is needed.
    match outcome {
        Ok(_) => {
            room.bump();
            app.persist(&room);
            cleared()
        }
        Err(msg) => toast(&msg),
    }
}

/// A player leaves the room. In the lobby the slot is freed (and the host is
/// handed on, or the room closes if nobody is left); mid-game the game ends for
/// everyone, the host included.
async fn leave_player(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Your session expired. Reload the page.");
    };
    let outcome = {
        let mut data = room.data.lock().unwrap();
        data.leave(&token)
    };
    match outcome {
        Ok(SeatOutcome::Closed { .. }) => {
            // Only bots are left, so drop the room. If a human joined in the
            // meantime the registry re-check keeps it alive with a new host.
            app.close_room_if_empty(&code);
        }
        Ok(_) => {
            room.bump();
            app.persist(&room);
        }
        Err(msg) => return toast(&msg),
    }
    // Send them out of the room, and expire the room's cookie so a stray
    // reload lands on the home page instead of a dead seat. The `gone` SSE
    // event is the second half of that navigation for a tab that is still
    // open.
    let (h, v) = clear_cookie_header(&code);
    (
        StatusCode::OK,
        [
            (h, v),
            (header::HeaderName::from_static("hx-redirect"), "/".to_string()),
        ],
        Html(String::new()),
    )
        .into_response()
}

/// Clear any stale error toast after a successful action.
///
/// This must not depend on the triggering element. The SSE swap can replace
/// the board or lobby fragment (and therefore the clicked button/spot) before
/// the POST response lands, so an out-of-band `#toasts` swap would be resolved
/// against a detached root and silently dropped. htmx dispatches
/// `HX-Trigger-After-Swap` on `document.body` when the trigger is no longer
/// attached, and `ui.js` empties `#toasts` on that event.
fn cleared() -> Response {
    (
        StatusCode::OK,
        [
            (header::HeaderName::from_static("hx-reswap"), "none"),
            (
                header::HeaderName::from_static("hx-trigger-after-swap"),
                "catan:clear-toasts",
            ),
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        ],
        String::new(),
    )
        .into_response()
}

fn toast(msg: &str) -> Response {
    let body = maud::html! { div #toasts.toasts hx-swap-oob="true" { (msg) } }.into_string();
    oob_only(body)
}

/// Return a body that only performs out-of-band swaps.
///
/// `HX-Retarget` is resolved relative to the *triggering* element's root. A
/// button inside a board, controls, or lobby fragment can be detached by the
/// SSE swap before the POST response lands; the retarget then resolves to
/// `null` and htmx throws while adding its swapping class. An out-of-band swap
/// is looked up from the document instead, so it does not care whether the
/// trigger is still attached. `HX-Reswap: none` keeps htmx from also swapping
/// this fragment into the (possibly detached) trigger.
fn oob_only(body: String) -> Response {
    (
        StatusCode::OK,
        [
            (header::HeaderName::from_static("hx-reswap"), "none"),
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        ],
        body,
    )
        .into_response()
}

// Keep the Room import used by the router signature helpers.
#[allow(dead_code)]
fn _assert_room(_: &Arc<Room>) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(tag: &str) -> Arc<AppState> {
        // A per-test directory, removed afterwards, so persisted room files
        // never leak between tests or into the repo.
        let dir = std::env::temp_dir().join(format!("catan-how-to-play-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        AppState::new(dir)
    }

    #[tokio::test]
    async fn toast_swaps_out_of_band_instead_of_retargeting_the_trigger() {
        let res = toast("Boom");
        assert!(
            res.headers().get("hx-retarget").is_none(),
            "a trigger-relative retarget breaks once SSE replaces the trigger"
        );
        assert_eq!(res.headers().get("hx-reswap").unwrap(), "none");
        let body = body_string(res).await;
        assert!(body.contains(r#"id="toasts""#), "the toast must target #toasts: {body}");
        assert!(body.contains(r#"hx-swap-oob="true""#), "the toast must be out of band: {body}");
        assert!(body.contains("Boom"), "the toast must carry the message: {body}");
    }

    #[tokio::test]
    async fn cleared_drives_the_toast_clear_from_an_event_not_a_target() {
        let res = cleared();
        assert!(res.headers().get("hx-retarget").is_none());
        assert_eq!(res.headers().get("hx-reswap").unwrap(), "none");
        assert_eq!(
            res.headers().get("hx-trigger-after-swap").unwrap(),
            "catan:clear-toasts",
            "htmx dispatches this event on <body> when the trigger was detached"
        );
        assert!(
            body_string(res).await.is_empty(),
            "the clear must not rely on resolving a possibly detached target"
        );
    }

    #[test]
    fn lobby_has_a_stable_toast_container() {
        let app = test_app("lobby-toast");
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        let data = room.data.lock().unwrap();
        let page = render::lobby_page(&data.code, &data, &token).into_string();
        drop(data);
        assert!(
            page.contains(r#"id="toasts""#),
            "lobby errors need a persistent #toasts target"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    async fn guide(app: &Arc<AppState>, room: &str) -> String {
        let res = how_to_play(State(app.clone()), Query(HowToPlayQuery { room: room.into() })).await;
        res.0
    }

    #[tokio::test]
    async fn guide_from_a_live_room_links_back_to_that_game() {
        let app = test_app("live");
        let (room, _token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        let _ = std::fs::remove_dir_all(app.data_dir.clone());

        let body = guide(&app, &code).await;
        assert!(body.contains("Back to game"), "a mid-game guide must offer a way back");
        assert!(
            body.contains(&format!("href=\"/room/{code}\"")),
            "the return link must target the live room"
        );
        assert!(body.contains("Back to home"), "Home stays available too");
    }

    #[tokio::test]
    async fn guide_from_home_keeps_the_home_path_only() {
        let app = test_app("home");
        let body = guide(&app, "").await;
        assert!(!body.contains("Back to game"), "no room means there is no game to return to");
        assert!(body.contains("Back to home"), "Home remains the only return path");
    }

    #[tokio::test]
    async fn guide_drops_unknown_or_malformed_room_codes() {
        let app = test_app("stale");
        for raw in ["ZZZZ", "", "   ", "\tabc\n"] {
            let body = guide(&app, raw).await;
            assert!(!body.contains("Back to game"), "{raw:?} must not produce a return path");
            assert!(body.contains("Back to home"), "{raw:?} still needs the Home path");
        }
    }

    #[tokio::test]
    async fn guide_looks_a_live_room_up_case_insensitively() {
        let app = test_app("case");
        let (room, _token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        let _ = std::fs::remove_dir_all(app.data_dir.clone());

        let body = guide(&app, &code.to_lowercase()).await;
        assert!(
            body.contains(&format!("href=\"/room/{code}\"")),
            "the lookup must normalise case before matching a live room"
        );
    }

    // --- replay gate (issue #13) -----------------------------------------

    fn auth_headers(code: &str, token: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        let cookie = format!("{}={token}", cookie_name(code));
        h.insert(header::COOKIE, cookie.parse().unwrap());
        h
    }

    async fn body_string(res: Response) -> String {
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// Create a started room with one bot seat and return (app, room, code, host token).
    fn started_room(tag: &str) -> (Arc<AppState>, Arc<Room>, String, String) {
        let app = test_app(tag);
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        room.data.lock().unwrap().add_bot(BotLevel::default());
        app.start_game(&room, &token).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        (app, room, code, token)
    }

    #[tokio::test]
    async fn replay_is_refused_while_the_game_is_live() {
        let (app, _room, code, token) = started_room("replay-live");
        let res = replay(
            State(app.clone()),
            Path(code.clone()),
            auth_headers(&code, &token),
            Query(ReplayQuery { n: None }),
        )
        .await;
        let body = body_string(res).await;
        assert!(
            body.contains("once the game is over"),
            "a live game must not expose a replay"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[tokio::test]
    async fn replay_page_renders_a_reconstructed_board() {
        let (app, room, code, token) = started_room("replay-done");
        {
            let mut data = room.data.lock().unwrap();
            let g = data.game.as_mut().unwrap();
            let pid = g.current;
            let v = g.legal_settlement_vertices(pid)[0];
            g.apply(pid, &Action::PlaceSettlement { vertex: v }).unwrap();
            // Force the game over so the replay is permitted; the handler does
            // not require the recorded moves to have produced the winner.
            g.phase = crate::game::state::Phase::GameOver;
            g.winner = Some(pid);
        }
        let res = replay(
            State(app.clone()),
            Path(code.clone()),
            auth_headers(&code, &token),
            Query(ReplayQuery { n: Some(1) }),
        )
        .await;
        let body = body_string(res).await;
        assert!(body.contains("Game replay"), "the replay page must render");
        assert!(body.contains("<svg"), "the board must render as SVG");
        assert!(
            body.contains("places a settlement"),
            "the move list must describe the recorded move"
        );
        assert!(
            body.contains("Move 1 / 1"),
            "the position counter must reflect the requested move index"
        );
        assert!(
            body.contains(&format!("href=\"/room/{code}/replay?n=1\"")),
            "the move list must link to the reconstructed position"
        );
        assert!(
            body.contains("replay-item on"),
            "the currently-shown move must be highlighted"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    /// Test helper for manual/browser QA of the replay: writes a finished game
    /// to `CATAN_SEED_DIR` (default a temp dir) and prints its room code. Point
    /// the server at that dir, then join the printed code with the name `Al` to
    /// land in the finished room and open its Replay. Ignored by default.
    #[test]
    #[ignore = "test helper: seeds a finished replay room for browser QA"]
    fn seed_finished_room_for_qa() {
        let dir = std::env::var("CATAN_SEED_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("catan-replay-qa"));
        let _ = std::fs::remove_dir_all(&dir);
        let app = AppState::new(dir.clone());
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        room.data.lock().unwrap().add_bot(BotLevel::default());
        app.start_game(&room, &token).unwrap();
        {
            let mut data = room.data.lock().unwrap();
            let g = data.game.as_mut().unwrap();
            let mut guard = 0;
            while matches!(g.phase, crate::game::state::Phase::Setup) {
                guard += 1;
                assert!(guard < 50, "setup failed to finish");
                let pid = g.current;
                if let Some(v) = g.setup_road_from {
                    let e = g.board.vertices[v]
                        .edges
                        .iter()
                        .copied()
                        .find(|&e| g.board.edges[e].owner.is_none())
                        .unwrap();
                    g.apply(pid, &Action::PlaceRoad { edge: e }).unwrap();
                } else {
                    let v = (0..g.board.vertices.len())
                        .find(|&v| g.legal_settlement_vertices(pid).contains(&v))
                        .unwrap();
                    g.apply(pid, &Action::PlaceSettlement { vertex: v }).unwrap();
                }
            }
            g.phase = crate::game::state::Phase::GameOver;
            g.winner = Some(0);
        }
        app.persist(&room);
        println!(
            "SEEDED room {} in {}",
            room.data.lock().unwrap().code,
            dir.display()
        );
    }
}

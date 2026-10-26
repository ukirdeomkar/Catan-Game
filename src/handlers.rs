use crate::game::actions::Action;
use crate::game::resources::{Bundle, Resource};
use crate::game::state::{BotLevel, GameState, MAX_PLAYERS};
use crate::render;
use crate::state::{AppState, JoinError, Layout, LeaveChoice, Room, SeatOutcome, ViewMode, now_ms};
use axum::{
    extract::{Form, Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
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
        .route("/robots.txt", get(robots))
        // Served from the root so the worker's scope covers the whole origin.
        .route("/sw.js", get(service_worker))
        // Digital Asset Links for the Android TWA (see docs/PLAYSTORE-ANDROID.md).
        .route("/.well-known/assetlinks.json", get(assetlinks))
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .layer(axum::middleware::from_fn(security_headers))
        .with_state(app)
}

/// Content-Security-Policy for the whole origin.
///
/// Every script and style is served from this origin under `/static`, so the
/// policy stays tight: no third-party origins, no inline scripts. Inline
/// `style` attributes are used heavily by the board renderer, hence
/// `style-src 'unsafe-inline'`. `frame-ancestors 'none'` matches
/// `X-Frame-Options: DENY`, and lives in CSP because `X-Frame-Options` cannot
/// express an allowlist.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
     connect-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'self'; \
     form-action 'self'; frame-ancestors 'none'";

/// Add baseline security headers to every response, including static assets.
async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    );
    res
}

/// Crawler policy. Live rooms are private to the players who hold the code, so
/// they are excluded; the public pages stay crawlable.
async fn robots() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        "User-agent: *\nAllow: /\nDisallow: /room/\n",
    )
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

/// The room token is the only identity a player has, so it must never travel
/// over plaintext HTTP: `Secure` keeps the browser from attaching it to an
/// `http://` request. Browsers treat `localhost` as trustworthy, so local
/// development on HTTP still works.
fn set_cookie_header(code: &str, token: &str) -> (header::HeaderName, String) {
    (
        header::SET_COOKIE,
        format!(
            "{}={token}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=31536000",
            cookie_name(code)
        ),
    )
}

/// Drop the room's cookie so a tab that is still open stops presenting it.
fn clear_cookie_header(code: &str) -> (header::HeaderName, String) {
    (
        header::SET_COOKIE,
        format!(
            "{}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0",
            cookie_name(code)
        ),
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
        mk("manage", f.manage),
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
        Some("board") => Layout::Board,
        _ => {
            return form_error(
                &headers,
                StatusCode::BAD_REQUEST,
                "Unknown board layout. Pick classic or board.",
            );
        }
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
    // htmx asks for a frame in place; returning only the stage keeps the
    // document (and its background) mounted instead of doing a full reload.
    let body = if is_hx_request(&headers) {
        render::replay_fragment(&code, &data, &view, n, total, Some(v)).into_string()
    } else {
        render::replay_page(&code, &data, &view, n, total, Some(v)).into_string()
    };
    Html(body).into_response()
}

/// True when the request came from htmx (used for fragment-only swaps).
fn is_hx_request(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

async fn add_bot(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return form_error(&headers, StatusCode::NOT_FOUND, "Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return form_error(
            &headers,
            StatusCode::UNAUTHORIZED,
            "Your session expired. Reload the page.",
        );
    };
    let Some(level) = form.get("level").and_then(|s| BotLevel::from_slug(s)) else {
        return form_error(
            &headers,
            StatusCode::BAD_REQUEST,
            "Unknown bot difficulty. Pick Easy, Medium, or Hard.",
        );
    };
    {
        let mut data = room.data.lock().unwrap();
        if !data.is_host(&token) {
            return form_error(
                &headers,
                StatusCode::FORBIDDEN,
                "Only the host can add bots.",
            );
        }
        if data.started {
            return form_error(
                &headers,
                StatusCode::CONFLICT,
                "The game has already started, so no more bots can join.",
            );
        }
        if data.members.len() >= MAX_PLAYERS {
            return form_error(
                &headers,
                StatusCode::CONFLICT,
                "The room is full (4 players max).",
            );
        }
        data.add_bot(level);
        data.last_activity_ms = now_ms();
    }
    room.bump();
    app.persist(&room);
    if is_htmx(&headers) {
        toast("Bot added.")
    } else {
        Redirect::to(&format!("/room/{code}")).into_response()
    }
}

/// Whether the request came from HTMX. HTMX forms get an inline toast; plain
/// form submissions (no JS) get a full error page so the message is readable.
fn is_htmx(headers: &HeaderMap) -> bool {
    headers.contains_key("hx-request")
}

fn form_error(headers: &HeaderMap, status: StatusCode, msg: &str) -> Response {
    if is_htmx(headers) {
        return toast(msg);
    }
    (status, Html(render::error_page(msg).into_string())).into_response()
}

/// Host-only: drop another seat. In the lobby the slot is freed; once the game
/// is running `mode=end` (the default) ends it for everyone, while `mode=bot`
/// hands the seat to a Medium bot and keeps the game going.
#[derive(Debug, Default, serde::Deserialize)]
struct RemoveForm {
    #[serde(default)]
    player: Option<usize>,
    /// Absent means `end`, so the older lobby forms keep working unchanged.
    #[serde(default)]
    mode: Option<String>,
}

async fn remove_player(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<RemoveForm>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Your session expired. Reload the page.");
    };
    let Some(target) = form.player else {
        return toast("Which player? Reload the page.");
    };
    let choice = match form.mode.as_deref() {
        None | Some("end") => LeaveChoice::End,
        Some("bot") => LeaveChoice::Bot,
        Some(_) => return toast("Choose how to remove this player."),
    };
    let outcome = {
        let mut data = room.data.lock().unwrap();
        data.remove_player_with(&token, target, choice)
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

/// The leave form. Both fields are optional so the older single-button forms
/// (and a plain empty POST) still take the safe default: end the game mid-game,
/// auto-pick the next host in the lobby.
#[derive(Debug, Default, serde::Deserialize)]
struct LeaveForm {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    next_host: Option<usize>,
}

/// A player leaves the room. In the lobby the slot is freed (and the host is
/// handed to the chosen human, or the room closes if nobody is left). Mid-game
/// the leaver picks between ending the game and a bot takeover.
async fn leave_player(
    State(app): State<Arc<AppState>>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Form(form): Form<LeaveForm>,
) -> Response {
    let code = code.to_uppercase();
    let Some(room) = app.room(&code) else {
        return toast("Room not found.");
    };
    let Some(token) = cookie_value(&headers, &cookie_name(&code)) else {
        return toast("Your session expired. Reload the page.");
    };
    let choice = match form.mode.as_deref() {
        None | Some("end") => LeaveChoice::End,
        Some("bot") => LeaveChoice::Bot,
        Some(_) => return toast("Choose how to leave the game."),
    };
    let outcome = {
        let mut data = room.data.lock().unwrap();
        data.leave_with(&token, choice, form.next_host)
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

    fn set_cookie(res: &Response) -> String {
        res.headers()
            .get(header::SET_COOKIE)
            .expect("the response must set the room cookie")
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn create_sets_a_secure_httponly_room_cookie() {
        let app = test_app("cookie-create");
        let res = create(
            State(app.clone()),
            Form(NameForm {
                name: "Al".into(),
                turn_seconds: 180,
                allow_pause: None,
            }),
        )
        .await;

        let cookie = set_cookie(&res);
        assert!(
            cookie.contains("; Secure"),
            "the room token must not go over HTTP: {cookie}"
        );
        assert!(
            cookie.contains("; HttpOnly"),
            "scripts must not read the token: {cookie}"
        );
        assert!(
            cookie.contains("Path=/"),
            "the cookie must stay scoped to the site: {cookie}"
        );
        assert!(
            cookie.contains("SameSite=Lax"),
            "cross-site requests must not carry the token: {cookie}"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[tokio::test]
    async fn clearing_the_room_cookie_keeps_the_secure_flag() {
        let (name, value) = clear_cookie_header("ABCD");
        assert_eq!(name, header::SET_COOKIE);
        assert!(
            value.contains("; Secure"),
            "the clearing cookie must match the set one: {value}"
        );
        assert!(
            value.contains("Max-Age=0"),
            "the clearing cookie must expire: {value}"
        );
    }

    fn cookie_headers(code: &str, token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{}={token}", cookie_name(code))).unwrap(),
        );
        headers
    }

    fn htmx_cookie_headers(code: &str, token: &str) -> HeaderMap {
        let mut headers = cookie_headers(code, token);
        headers.insert("hx-request", HeaderValue::from_static("true"));
        headers
    }

    fn level_form(level: &str) -> Form<HashMap<String, String>> {
        Form(HashMap::from([("level".to_string(), level.to_string())]))
    }

    #[tokio::test]
    async fn add_bot_rejects_an_unknown_difficulty() {
        let app = test_app("bot-level");
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();

        let res = add_bot(
            State(app.clone()),
            Path(code.clone()),
            cookie_headers(&code, &token),
            level_form("godmode"),
        )
        .await;

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            room.data.lock().unwrap().members.len(),
            1,
            "an unknown level must not add a bot"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[tokio::test]
    async fn add_bot_after_the_game_started_is_rejected() {
        let app = test_app("bot-started");
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        room.data.lock().unwrap().add_bot(BotLevel::Easy);
        app.start_game(&room, &token).unwrap();
        let before = room.data.lock().unwrap().members.len();

        let res = add_bot(
            State(app.clone()),
            Path(code.clone()),
            cookie_headers(&code, &token),
            level_form("easy"),
        )
        .await;

        assert_eq!(res.status(), StatusCode::CONFLICT);
        assert_eq!(
            room.data.lock().unwrap().members.len(),
            before,
            "a started game must not accept another bot"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[tokio::test]
    async fn add_bot_htmx_failure_renders_a_toast() {
        let app = test_app("bot-toast");
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        room.data.lock().unwrap().add_bot(BotLevel::Easy);
        app.start_game(&room, &token).unwrap();

        let res = add_bot(
            State(app.clone()),
            Path(code.clone()),
            htmx_cookie_headers(&code, &token),
            level_form("easy"),
        )
        .await;

        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers().get("hx-retarget").is_none(),
            "a trigger-relative retarget breaks once SSE replaces the trigger"
        );
        assert_eq!(res.headers().get("hx-reswap").unwrap(), "none");
        let body = body_string(res).await;
        assert!(body.contains(r#"id="toasts""#), "the toast must target #toasts: {body}");
        assert!(body.contains(r#"hx-swap-oob="true""#), "the toast must be out of band: {body}");
        assert!(
            body.contains("already started"),
            "the toast must explain why the bot was not added: {body}"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[tokio::test]
    async fn set_layout_rejects_an_unknown_layout() {
        let app = test_app("layout-unknown");
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();

        let res = set_layout(
            State(app.clone()),
            Path(code.clone()),
            cookie_headers(&code, &token),
            Form(HashMap::from([("layout".into(), "banana".into())])),
        )
        .await;

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            room.data.lock().unwrap().members[0].layout,
            Layout::Classic,
            "an unknown layout must not change the viewer's layout"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[tokio::test]
    async fn set_layout_accepts_board_and_classic() {
        let app = test_app("layout-valid");
        let (room, token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        for (value, expected) in [("board", Layout::Board), ("classic", Layout::Classic)] {
            let res = set_layout(
                State(app.clone()),
                Path(code.clone()),
                cookie_headers(&code, &token),
                Form(HashMap::from([("layout".into(), value.into())])),
            )
            .await;
            assert_eq!(res.status(), StatusCode::NO_CONTENT);
            assert_eq!(room.data.lock().unwrap().members[0].layout, expected);
        }
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[tokio::test]
    async fn responses_carry_security_headers_and_robots_is_served() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = test_app("headers");
        let res = router(app.clone())
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        for name in [
            "strict-transport-security",
            "content-security-policy",
            "x-content-type-options",
            "x-frame-options",
            "referrer-policy",
            "permissions-policy",
        ] {
            assert!(res.headers().contains_key(name), "missing {name}");
        }

        let res = router(app.clone())
            .oneshot(
                Request::builder()
                    .uri("/robots.txt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap(),
            "text/plain; charset=utf-8"
        );
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("User-agent: *"), "body was {body:?}");
        assert!(body.contains("Disallow: /room/"), "private rooms stay unlisted");
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
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
        assert!(
            body.contains(r##"hx-target="#replay-board""##),
            "prev/next must swap only the board, like the live game"
        );
        assert!(
            body.contains(r##"hx-swap="innerHTML""##),
            "the board wrapper must stay mounted while its contents change"
        );
        assert!(
            body.contains(r#"id="replay-board""#),
            "the replay must expose a stable board swap target"
        );
        assert!(
            body.contains(r#"id="replay-list""#),
            "the move list must stay mounted so its scroll position survives"
        );
        assert!(
            body.contains("/static/replay.js"),
            "arrow-key JS must be an external script so the CSP allows it"
        );
        assert!(
            !body.contains("__catanReplayKeys"),
            "the replay page must not emit inline JS that `script-src 'self'` blocks"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    /// Finishes the started room so the replay route is allowed.
    fn finish_room(room: &Arc<Room>) {
        let mut data = room.data.lock().unwrap();
        let g = data.game.as_mut().unwrap();
        let pid = g.current;
        let v = g.legal_settlement_vertices(pid)[0];
        g.apply(pid, &Action::PlaceSettlement { vertex: v }).unwrap();
        g.phase = crate::game::state::Phase::GameOver;
        g.winner = Some(pid);
    }

    #[tokio::test]
    async fn replay_fragment_swaps_the_board_and_out_of_band_chrome() {
        let (app, room, code, token) = started_room("replay-hx");
        finish_room(&room);
        let mut headers = auth_headers(&code, &token);
        headers.insert("HX-Request", "true".parse().unwrap());
        let res = replay(
            State(app.clone()),
            Path(code.clone()),
            headers,
            Query(ReplayQuery { n: Some(1) }),
        )
        .await;
        let body = body_string(res).await;
        assert!(
            body.contains("<svg"),
            "the fragment must still render the reconstructed board"
        );
        assert!(
            body.contains(r#"data-n="1""#) && body.contains("Move 1 / 1"),
            "the position must update out of band and carry the current move"
        );
        assert!(
            body.contains(r#"hx-swap-oob="outerHTML""#) && body.contains(r#"hx-swap-oob="innerHTML""#),
            "the chrome around the board must update out of band"
        );
        assert!(
            !body.contains(r#"id="replay-root""#) && !body.contains(r#"id="replay-list""#),
            "the fragment must leave the page layout and move list mounted"
        );
        assert!(
            !body.contains("<html") && !body.contains("htmx.min.js"),
            "the fragment must not ship a second full document"
        );
        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    // --- leave UX v2 (issue #309) ----------------------------------------

    #[test]
    fn lobby_host_leave_picker_lists_humans_and_moves_the_controls() {
        let app = test_app("leave-picker");
        let (room, alice) = app.create_room("Alice".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        let (_, bob) = app.join(&code, "Bob".into()).unwrap();
        room.data.lock().unwrap().add_bot(BotLevel::default());
        let (_, carol) = app.join(&code, "Carol".into()).unwrap();

        let (alice_html, carol_html_before, carol_idx) = {
            let data = room.data.lock().unwrap();
            (
                render::lobby_frag(&code, &data, &alice).into_string(),
                render::lobby_frag(&code, &data, &carol).into_string(),
                data.member_index(&carol).unwrap(),
            )
        };
        assert!(
            alice_html.contains("name=\"next_host\""),
            "the host must pick the successor explicitly"
        );
        let picker = alice_html
            .split("name=\"next_host\"")
            .nth(1)
            .expect("next-host select")
            .split("</select>")
            .next()
            .unwrap();
        assert!(picker.contains("Bob") && picker.contains("Carol"));
        assert!(!picker.contains("Bot 1"), "bots can never inherit the badge");
        assert!(
            !carol_html_before.contains("Host controls"),
            "only the host sees host controls before the transfer"
        );

        room.data
            .lock()
            .unwrap()
            .leave_with(&alice, LeaveChoice::End, Some(carol_idx))
            .unwrap();
        let (carol_html, bob_html) = {
            let data = room.data.lock().unwrap();
            (
                render::lobby_frag(&code, &data, &carol).into_string(),
                render::lobby_frag(&code, &data, &bob).into_string(),
            )
        };
        assert!(
            carol_html.contains("Host controls") && carol_html.contains("Start game"),
            "the chosen host gets the controls on the next SSE fragment"
        );
        assert!(
            !bob_html.contains("Host controls"),
            "the skipped human does not get host controls"
        );
        let _ = app;
        std::fs::remove_dir_all(app.data_dir.clone()).ok();
    }

    #[test]
    fn midgame_cards_are_information_only_and_management_is_step_by_step() {
        let app = test_app("manage-choices");
        let (room, alice) = app.create_room("Alice".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        let (_, bob) = app.join(&code, "Bob".into()).unwrap();
        let (_, _carol) = app.join(&code, "Carol".into()).unwrap();
        app.start_game(&room, &alice).unwrap();
        let (alice_seat, bob_seat) = {
            let data = room.data.lock().unwrap();
            (
                data.member_index(&alice).unwrap(),
                data.member_index(&bob).unwrap(),
            )
        };

        let (host, guest) = {
            let data = room.data.lock().unwrap();
            let game = data.game.as_ref().unwrap();
            (
                render::fragments(game, &data, Some(alice_seat)),
                render::fragments(game, &data, Some(bob_seat)),
            )
        };
        let host_players = host.players.into_string();
        let guest_players = guest.players.into_string();
        let host_status = host.status.into_string();
        let guest_status = guest.status.into_string();
        let host_manage = host.manage.into_string();
        let guest_manage = guest.manage.into_string();

        // Player cards carry no management actions at all.
        for html in [&host_players, &guest_players] {
            for gone in ["Remove", "End game", "Continue with bot", "Next host", "data-sheet-open"] {
                assert!(
                    !html.contains(gone),
                    "player cards must be information-only, found {gone:?}: {html}"
                );
            }
        }

        // The top bar is the only management affordance.
        assert!(host_status.contains("Manage game"));
        assert!(host_status.contains(r#"data-sheet-open="manage""#));
        assert!(guest_status.contains("Leave game"));
        assert!(guest_status.contains(r#"data-sheet-open="leave""#));

        // Host: two-step list, then leave / players, then kick.
        assert!(host_manage.contains(r#"data-sheet="manage""#));
        assert!(host_manage.contains(r#"data-sheet-open="manage-leave""#));
        assert!(host_manage.contains(r#"data-sheet-open="manage-players""#));
        assert!(host_manage.contains(r#"data-sheet="manage-leave""#));
        assert!(host_manage.contains("Pass my seat to a bot"));
        assert!(host_manage.contains("End game for everyone"));
        assert!(
            host_manage.contains(r#"name="next_host""#),
            "a leaving host picks the next host on the continue path"
        );
        assert!(host_manage.contains(r#"data-sheet="manage-kick-"#));
        assert!(host_manage.contains("Pass seat to a bot"));
        assert!(host_manage.contains("Manage players"));

        // Non-host: one sheet, no host picker, no player management.
        assert!(guest_manage.contains(r#"data-sheet="leave""#));
        assert!(guest_manage.contains("Pass my seat to a bot"));
        assert!(guest_manage.contains("End game for everyone"));
        assert!(!guest_manage.contains(r#"name="next_host""#));
        assert!(!guest_manage.contains("Manage players"));
        assert!(!guest_manage.contains(r#"data-sheet="manage""#));

        // Only bots remain after the last human: end-game is the only choice.
        let (room2, alice2) = app.create_room("Solo".into(), 0, false).unwrap();
        room2.data.lock().unwrap().add_bot(BotLevel::Medium);
        app.start_game(&room2, &alice2).unwrap();
        let solo = {
            let data = room2.data.lock().unwrap();
            let game = data.game.as_ref().unwrap();
            (
                render::fragments(game, &data, Some(0)).status.into_string(),
                render::fragments(game, &data, Some(0)).manage.into_string(),
            )
        };
        assert!(solo.0.contains("Manage game"), "the solo host still manages");
        assert!(
            !solo.1.contains("Pass my seat to a bot"),
            "with no other humans, a bot takeover is not offered"
        );
        assert!(solo.1.contains("End game for everyone"));
        let _ = app;
        std::fs::remove_dir_all(app.data_dir.clone()).ok();
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

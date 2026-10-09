use crate::game::actions::Action;
use crate::game::resources::{Bundle, Resource};
use crate::game::state::BotLevel;
use crate::render;
use crate::state::{AppState, JoinError, Layout, Room, ViewMode, now_ms};
use axum::{
    extract::{Form, Path, State},
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
        .route("/create", post(create))
        .route("/join", post(join))
        .route("/room/{code}", get(room_page))
        .route("/room/{code}/join", post(join_room))
        .route("/room/{code}/events", get(events))
        .route("/room/{code}/action", post(action))
        .route("/room/{code}/mode", post(set_mode))
        .route("/room/{code}/layout", post(set_layout))
        .route("/room/{code}/start", post(start))
        .route("/room/{code}/pause", post(pause))
        .route("/room/{code}/resume", post(resume))
        .route("/room/{code}/add_bot", post(add_bot))
        .route("/healthz", get(|| async { "ok" }))
        // Served from the root so the worker's scope covers the whole origin.
        .route("/sw.js", get(service_worker))
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .with_state(app)
}

/// The service worker, embedded and served from the origin root so its scope
/// covers every page (enables PWA installability).
async fn service_worker() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../static/sw.js"),
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

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

async fn home() -> Html<String> {
    Html(render::home_page(None).into_string())
}

async fn how_to_play() -> Html<String> {
    Html(render::how_to_play_page().into_string())
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
        Err(e) => Html(render::home_page(Some(join_error(&e))).into_string()).into_response(),
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
        Err(e) => Html(render::home_page(Some(join_error(&e))).into_string()).into_response(),
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
        return Vec::new();
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
            // Empty body so htmx clears any stale error toast. The retarget
            // header keeps the swap pointed at #toasts even though the action
            // was triggered from a board/controls element inside the SSE-swapped
            // fragments (which no longer inherit a target from #app).
            (
                StatusCode::OK,
                [(header::HeaderName::from_static("hx-retarget"), "#toasts")],
                Html(String::new()),
            )
                .into_response()
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
                if game.trade.as_ref().map(|t| t.id) == Some(id) {
                    game.trade = None;
                    game.push_log(None, "The trade offer expired.".to_string());
                    true
                } else {
                    false
                }
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

fn toast(msg: &str) -> Response {
    let body = maud::html! { (msg) }.into_string();
    (
        StatusCode::OK,
        [(header::HeaderName::from_static("hx-retarget"), "#toasts")],
        Html(body),
    )
        .into_response()
}

// Keep the Room import used by the router signature helpers.
#[allow(dead_code)]
fn _assert_room(_: &Arc<Room>) {}

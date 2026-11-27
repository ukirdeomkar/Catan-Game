//! Hex Ports — a small self-hosted multiplayer board game server.
//!
//! The rules engine in `game/` exposes a broad API (score helpers, legal-move
//! enumeration, port lookups) that the server does not need *yet* but future
//! AI bots and the UI will, so dead-code analysis is disabled crate-wide.
#![allow(dead_code)]

mod bot;
mod game;
mod handlers;
mod ratelimit;
mod render;
mod seo;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let data_dir = state::data_dir_default();
    state::ensure_dir(&data_dir);
    let app: Arc<state::AppState> = state::AppState::new(data_dir.clone());
    tracing::info!("data directory: {}", data_dir.display());

    // Periodically drop rooms that have been idle for a long time.
    let sweeper = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            state::sweep(&sweeper, 24 * 60 * 60 * 1000);
        }
    });

    // Drive the per-turn countdown timer for every room.
    let timers = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            // These loops have no supervisor, so a panic must not end them: a
            // dead timer task would silently stop every phase deadline (#81).
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state::tick_turn_timers(&timers)
            }));
            if run.is_err() {
                tracing::error!("timer tick panicked; the clock keeps running");
            }
        }
    });

    // Let AI bots take one action per tick so humans can watch the game unfold.
    let bots = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(700));
        loop {
            tick.tick().await;
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state::tick_bots(&bots)
            }));
            if run.is_err() {
                tracing::error!("bot tick panicked; the loop keeps running");
            }
        }
    });

    // Append an aggregate activity line every few minutes so the owner can see
    // how many humans are actually playing and against whom. It writes nothing
    // when nobody is connected.
    let activity = app.clone();
    let activity_path = state::activity_log_path(&app);
    let activity_secs = std::env::var("CATAN_ACTIVITY_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(300);
    tracing::info!(
        "activity log: {} every {activity_secs}s",
        activity_path.display()
    );
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(activity_secs));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            // This loop has no supervisor, so a panic must not end it either.
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some(snapshot) = state::snapshot_activity(&activity) {
                    state::append_activity_log(&activity_path, &snapshot);
                }
            }));
            if run.is_err() {
                tracing::error!("activity snapshot panicked; the logger keeps running");
            }
        }
    });

    let addr = std::env::var("CATAN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind address");
    tracing::info!("listening on http://{addr}");
    // Carry the peer address into requests so the room-creation limiter can
    // tell clients apart (see `handlers::client_ip`).
    axum::serve(
        listener,
        handlers::router(app).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}

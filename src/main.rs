//! Catan — a small self-hosted multiplayer board game server.
//!
//! The rules engine in `game/` exposes a broad API (score helpers, legal-move
//! enumeration, port lookups) that the server does not need *yet* but future
//! AI bots and the UI will, so dead-code analysis is disabled crate-wide.
#![allow(dead_code)]

mod bot;
mod game;
mod handlers;
mod render;
mod state;

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
            state::tick_turn_timers(&timers);
        }
    });

    // Let AI bots take one action per tick so humans can watch the game unfold.
    let bots = app.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(700));
        loop {
            tick.tick().await;
            state::tick_bots(&bots);
        }
    });

    let addr = std::env::var("CATAN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind address");
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, handlers::router(app))
        .await
        .expect("server error");
}

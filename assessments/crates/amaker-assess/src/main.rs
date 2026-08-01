//! amaker-assess — respondent binary.
//!
//! A respondent fills out a published assessment here. No chat sidebar,
//! no surgical tools, no LLM access. Shares filesystem storage with the
//! author binary via `ASSESS_DATA_DIR`.

mod config;
mod handlers;
mod routes;
mod state;

use std::net::SocketAddr;

use amaker_core::net::{detect_lan_ip, shutdown_signal};
use amaker_core::observability::init_tracing;

use tower_http::{services::ServeDir, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::from_env()?;

    init_tracing(&config.log_level, &[]);

    let state = state::AppState::new(config.clone()).await?;

    let app = routes::create_router(state)
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = config.bind_address().parse()?;
    tracing::info!("Starting amaker-assess at http://{}", addr);
    if addr.ip().is_unspecified() {
        tracing::info!("  local:    http://127.0.0.1:{}", addr.port());
        if let Some(lan_ip) = detect_lan_ip() {
            tracing::info!("  LAN:      http://{}:{}", lan_ip, addr.port());
        }
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

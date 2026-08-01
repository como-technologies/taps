//! Amaker - AI-Assisted Assessment Authoring Tool
//!
//! A Rust web application that helps domain Subject Matter Experts (SMEs)
//! create structured assessments through AI-assisted conversation.

mod config;
mod handlers;
mod routes;
mod services;
mod state;

use std::net::SocketAddr;

use amaker_core::net::{detect_lan_ip, shutdown_signal};
use amaker_core::observability::init_tracing;

use axum::http::{HeaderValue, Method};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = config::Config::from_env()?;

    // `rig::agent::prompt_request` emits the `execute_tool` INFO span and
    // an event per tool call that prints the full tool args + result
    // inline; at assessment-authoring size those payloads are the entire
    // prompt, which drowns out everything useful. Pin that target to WARN.
    init_tracing(&config.log_level, &["rig::agent::prompt_request"]);

    // Create application state
    let state = state::AppState::new(config.clone()).await?;

    // CORS for the cross-binary regenerate-narrative POST that originates
    // in the browser on the analyze binary's origin. We only need to allow
    // the analyze base URL — neither assess nor anything else needs to
    // call author cross-origin in Stage 2. If/when more cross-origin calls
    // land, widen this list rather than going permissive.
    let analyze_origin: HeaderValue = config
        .analyze_base_url
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid AUTHOR_ANALYZE_BASE_URL: {e}"))?;
    let cors = CorsLayer::new()
        .allow_origin(analyze_origin)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(tower_http::cors::Any);

    let app = routes::create_router(state)
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Start server
    let addr: SocketAddr = config.bind_address().parse()?;
    tracing::info!("Starting Amaker at http://{}", addr);
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

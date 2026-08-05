//! amaker-analyze — analyst binary.
//!
//! Read-only scorecard / gaps / roadmap / narrative tabs over a
//! published assessment + its responses. No LLM access; the
//! "Regenerate narrative" button POSTs cross-origin to the author
//! binary, which holds the LLM transport.

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
    // Answer --version before any env/config side effects (suite
    // convention; taps issue 50).
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let config = config::Config::from_env()?;

    init_tracing(&config.log_level, &[]);

    let state = state::AppState::new(config.clone()).await?;

    let app = routes::create_router(state)
        .nest_service("/assets", ServeDir::new("assets"))
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = config.bind_address().parse()?;
    tracing::info!("Starting amaker-analyze at http://{}", addr);
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

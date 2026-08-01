//! Analyst-binary routes — all read-only.

use std::sync::Arc;

use axum::{Router, routing::get};

use crate::handlers;
use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Liveness probe — process-up check, touches no storage.
        .route("/healthz", get(|| async { "ok" }))
        .route("/analyze/{id}", get(handlers::pages::analyze))
        .route(
            "/api/projects/{id}/analysis",
            get(handlers::analysis::get_tabs),
        )
        .route(
            "/api/projects/{id}/analysis/scorecard",
            get(handlers::analysis::get_scorecard),
        )
        .route(
            "/api/projects/{id}/analysis/gaps",
            get(handlers::analysis::get_gaps),
        )
        .route(
            "/api/projects/{id}/analysis/roadmap",
            get(handlers::analysis::get_roadmap),
        )
        .route(
            "/api/projects/{id}/analysis/narrative",
            get(handlers::analysis::get_narrative),
        )
        .with_state(state)
}

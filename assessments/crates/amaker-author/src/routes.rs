//! Application routes.

use std::sync::Arc;

use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::handlers;
use crate::state::AppState;

/// Create the application router.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Liveness probe — process-up check, touches no storage or LLM.
        .route("/healthz", get(|| async { "ok" }))
        // ===== Page Routes =====
        // Respond + analyze pages live in their own binaries
        // (amaker-assess, amaker-analyze). The author's "Respond" and
        // "Analyze" header links navigate to those processes directly.
        .route("/", get(handlers::pages::home))
        .route("/projects/{id}", get(handlers::pages::workspace))
        // ===== Project API =====
        .route("/api/projects", get(handlers::projects::list))
        .route("/api/projects", post(handlers::projects::create))
        .route("/api/projects/{id}", get(handlers::projects::get))
        .route("/api/projects/{id}", delete(handlers::projects::delete))
        .route(
            "/api/projects/{id}/phase",
            get(handlers::projects::get_phase),
        )
        .route(
            "/api/projects/{id}/phase",
            post(handlers::projects::update_phase),
        )
        // ===== Chat API =====
        .route("/api/projects/{id}/chat", get(handlers::chat::get_messages))
        .route(
            "/api/projects/{id}/chat",
            post(handlers::chat::send_message),
        )
        .route(
            "/api/projects/{id}/chat/respond",
            post(handlers::chat::respond_to_question),
        )
        .route(
            "/api/projects/{id}/chat/skip-question",
            post(handlers::chat::skip_question),
        )
        // ===== Assist API =====
        .route(
            "/api/projects/{id}/assist",
            get(handlers::assist::generate_assist),
        )
        // ===== Preview API =====
        .route(
            "/api/projects/{id}/preview",
            get(handlers::preview::get_assessment),
        )
        // ===== Analysis API =====
        // Read-only tabs / scorecard / gaps / roadmap / narrative live in
        // amaker-analyze. Only narrative regeneration stays here — it's
        // the only LLM-using endpoint in the analysis cluster, and rig
        // lives in the author binary.
        .route(
            "/api/projects/{id}/analysis/narrative/regenerate",
            post(handlers::analysis::regenerate_narrative),
        )
        // ===== Upload API =====
        .route(
            "/api/projects/{id}/upload",
            post(handlers::upload::upload_document),
        )
        .route(
            "/api/projects/{id}/documents/{doc_id}",
            delete(handlers::upload::delete_document),
        )
        // ===== Export API =====
        .route("/api/schema", get(handlers::export::get_schema))
        .route(
            "/api/projects/{id}/export",
            get(handlers::export::download_export),
        )
        .with_state(state)
}

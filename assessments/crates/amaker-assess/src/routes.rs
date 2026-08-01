//! Respondent-binary routes.

use std::sync::Arc;

use axum::{
    Router,
    routing::{delete, get, patch, post},
};

use crate::handlers;
use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Liveness probe — process-up check, touches no storage.
        .route("/healthz", get(|| async { "ok" }))
        .route("/assess/{id}", get(handlers::pages::assess))
        .route(
            "/api/projects/{id}/response",
            get(handlers::collection::get_response_form),
        )
        .route(
            "/api/projects/{id}/response/submit",
            post(handlers::collection::submit_response),
        )
        .route(
            "/api/projects/{id}/response/answers/{question_id}",
            patch(handlers::collection::upsert_answer),
        )
        .route(
            "/api/projects/{id}/response/answers/{question_id}",
            delete(handlers::collection::clear_answer),
        )
        .with_state(state)
}

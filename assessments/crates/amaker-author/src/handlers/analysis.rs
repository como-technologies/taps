//! Narrative-regeneration handler.
//!
//! After the audience split (#39 Stage 2), the read-only analysis routes
//! (tabs / scorecard / gaps / roadmap / narrative GET) live in
//! `amaker-analyze`. Only narrative *regeneration* stays here because it
//! invokes the LLM, and the LLM transport (`rig`) is author-only.
//!
//! The analyze binary's narrative tab POSTs cross-origin to this handler
//! when the SME (or an analyst) hits "Regenerate"; the rendered partial
//! is swapped back into the analyze page.

use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::Html,
};

use amaker_core::AppError;
use amaker_core::models::ProjectId;
use amaker_core::services::analysis::{compute_gaps, compute_roadmap, compute_scorecard};
use amaker_core::services::markdown_to_html;

use crate::services::narrative;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "partials/analysis/narrative.html")]
pub struct NarrativeTemplate {
    pub project_id: ProjectId,
    pub markdown: Option<String>,
    pub html: String,
    pub cached: bool,
    /// Where the regenerate button POSTs back to. Author renders this
    /// template in response to a regenerate POST; the swapped-in HTML
    /// lands inside analyze's DOM, so the button URL must remain
    /// absolute to author for the next regenerate cycle.
    pub author_base_url: String,
}

/// POST to (re)generate the narrative. Always hits the LLM and overwrites
/// the cache. Returns the same `narrative.html` partial body that analyze's
/// tab swaps in.
pub async fn regenerate_narrative(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let (assessment, response) = state.responses.load_primary_context(project_id).await?;
    let scorecard = compute_scorecard(&assessment, &response);
    let inventory = compute_gaps(&assessment, &response);
    let roadmap = compute_roadmap(&inventory);

    let report = narrative::regenerate(
        &state.storage,
        &state.ai,
        project_id,
        &assessment,
        &response,
        &scorecard,
        &inventory,
        &roadmap,
        None,
    )
    .await?;

    let html = markdown_to_html(&report.markdown);
    let tpl = NarrativeTemplate {
        project_id,
        markdown: Some(report.markdown),
        html,
        cached: report.cached,
        author_base_url: state.author_public_url.clone(),
    };
    Ok(Html(
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

//! Read-only analysis handlers.
//!
//! Each route renders one tab of the analysis pane. The narrative tab
//! reads the cached `report.md`; regeneration POSTs cross-origin to the
//! author binary, which holds the LLM transport (#39 Stage 2).

use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::Html,
};

use amaker_core::AppError;
use amaker_core::models::{Assessment, ProjectId};
use amaker_core::services::analysis::{
    GapInventory, Roadmap, Scorecard, compute_gaps, compute_roadmap, compute_scorecard,
};
use amaker_core::services::markdown_to_html;

use crate::state::AppState;

#[derive(Template)]
#[template(path = "partials/analysis/tabs.html")]
pub struct AnalysisTabsTemplate {
    pub project_id: ProjectId,
    pub assessment: Assessment,
}

#[derive(Template)]
#[template(path = "partials/analysis/scorecard.html")]
pub struct ScorecardTemplate {
    pub scorecard: Scorecard,
}

#[derive(Template)]
#[template(path = "partials/analysis/gap_inventory.html")]
pub struct GapInventoryTemplate {
    pub inventory: GapInventory,
}

#[derive(Template)]
#[template(path = "partials/analysis/roadmap.html")]
pub struct RoadmapTemplate {
    pub roadmap: Roadmap,
}

#[derive(Template)]
#[template(path = "partials/analysis/narrative.html")]
pub struct NarrativeTemplate {
    pub project_id: ProjectId,
    pub markdown: Option<String>,
    pub html: String,
    pub cached: bool,
    /// URL of the author binary — the regenerate button POSTs here.
    pub author_base_url: String,
}

pub async fn get_tabs(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let (assessment, _response) = state.responses.load_primary_context(project_id).await?;
    let tpl = AnalysisTabsTemplate {
        project_id,
        assessment,
    };
    Ok(Html(render(tpl)?))
}

pub async fn get_scorecard(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let (assessment, response) = state.responses.load_primary_context(project_id).await?;
    let scorecard = compute_scorecard(&assessment, &response);
    Ok(Html(render(ScorecardTemplate { scorecard })?))
}

pub async fn get_gaps(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let (assessment, response) = state.responses.load_primary_context(project_id).await?;
    let inventory = compute_gaps(&assessment, &response);
    Ok(Html(render(GapInventoryTemplate { inventory })?))
}

pub async fn get_roadmap(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let (assessment, response) = state.responses.load_primary_context(project_id).await?;
    let inventory = compute_gaps(&assessment, &response);
    let roadmap = compute_roadmap(&inventory);
    Ok(Html(render(RoadmapTemplate { roadmap })?))
}

pub async fn get_narrative(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let markdown = state
        .storage
        .load_analysis_cache(project_id, "report.md")
        .await?;
    let cached = markdown.is_some();
    let html = markdown
        .as_deref()
        .map(markdown_to_html)
        .unwrap_or_default();
    Ok(Html(render(NarrativeTemplate {
        project_id,
        markdown,
        html,
        cached,
        author_base_url: state.author_base_url.clone(),
    })?))
}

fn render<T: Template>(tpl: T) -> Result<String, AppError> {
    tpl.render().map_err(|e| AppError::Internal(e.to_string()))
}

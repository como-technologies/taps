//! Page shells for the respondent binary. Just the one route, since
//! everything inside the page loads via HTMX from the API routes.

use std::sync::Arc;

use amaker_core::AppError;
use amaker_core::models::{Project, ProjectId};
use askama::Template;
use axum::{
    extract::{Path, State},
    response::Html,
};

use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/assess.html")]
pub struct AssessTemplate {
    pub project: Project,
    /// Absolute URL of the author binary — "Back to authoring" link.
    pub author_base_url: String,
    /// Absolute URL of the analyze binary — "View results" link.
    pub analyze_base_url: String,
}

pub async fn assess(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let project = state.storage.load_project(project_id).await?;
    let template = AssessTemplate {
        project,
        author_base_url: state.author_base_url.clone(),
        analyze_base_url: state.analyze_base_url.clone(),
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

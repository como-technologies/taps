//! Page shells for the analyst binary.

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
#[template(path = "pages/analyze.html")]
pub struct AnalyzeTemplate {
    pub project: Project,
    pub author_base_url: String,
    pub assess_base_url: String,
}

pub async fn analyze(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let project = state.storage.load_project(project_id).await?;
    let template = AnalyzeTemplate {
        project,
        author_base_url: state.author_base_url.clone(),
        assess_base_url: state.assess_base_url.clone(),
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

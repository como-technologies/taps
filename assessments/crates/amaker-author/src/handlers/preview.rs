//! Authoring preview pane — renders the live draft assessment.
//!
//! Stage 1b: this route only serves the Authoring view. The respondent
//! form and analyst tabs are reached via their own top-level routes
//! (`/respond/{id}` and `/analyze/{id}`).

use std::sync::Arc;

use amaker_core::AppError;
use amaker_core::models::{Assessment, ProjectId};
use amaker_core::services::YamlService;
use askama::Template;
use axum::{
    extract::{Path, State},
    response::Html,
};

use crate::state::AppState;

/// Assessment preview template — the live draft tree.
#[derive(Template)]
#[template(path = "partials/preview/assessment.html")]
pub struct AssessmentPreviewTemplate {
    pub assessment: Option<Assessment>,
}

pub async fn get_assessment(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let yaml = state.storage.load_assessment_yaml(project_id).await?;
    let assessment = yaml
        .as_deref()
        .and_then(|y| YamlService::parse_assessment(y).ok());
    let template = AssessmentPreviewTemplate { assessment };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

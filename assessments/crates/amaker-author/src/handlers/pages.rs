//! Full page handlers.

use std::sync::Arc;

use crate::handlers::projects::ProjectCardView;
use crate::state::AppState;
use amaker_core::AppError;
use amaker_core::models::{ModelOption, Project, ProjectId};
use askama::Template;
use axum::{
    extract::{Path, State},
    response::Html,
};

/// Home page template.
#[derive(Template)]
#[template(path = "pages/home.html")]
pub struct HomeTemplate {
    pub cards: Vec<ProjectCardView>,
}

/// Workspace page template.
#[derive(Template)]
#[template(path = "pages/workspace.html")]
pub struct WorkspaceTemplate {
    pub project: Project,
    pub models: Vec<ModelOption>,
    pub default_model: String,
    /// Absolute URL of the assess binary, used to build the "Respond" header link.
    pub assess_base_url: String,
    /// Absolute URL of the analyze binary, used to build the "Analyze" header link.
    pub analyze_base_url: String,
}

/// Render the home page with project list.
pub async fn home(State(state): State<Arc<AppState>>) -> Result<Html<String>, AppError> {
    let projects = state.storage.list_projects().await?;
    let mut cards = Vec::with_capacity(projects.len());
    for project in projects {
        cards.push(ProjectCardView::for_project(&state, project).await);
    }
    let template = HomeTemplate { cards };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// Render the workspace page for a project.
pub async fn workspace(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let project = state.storage.load_project(project_id).await?;
    let template = WorkspaceTemplate {
        project,
        models: state.models.clone(),
        default_model: state.default_model.clone(),
        assess_base_url: state.assess_base_url.clone(),
        analyze_base_url: state.analyze_base_url.clone(),
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

// Respond + analyze page shells live in their own binaries
// (amaker-assess, amaker-analyze). The author's "Respond" / "Analyze"
// header links navigate to those processes cross-origin — see
// `workspace.html`.

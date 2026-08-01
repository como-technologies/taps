//! Project CRUD handlers.

use std::sync::Arc;

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;

use amaker_core::AppError;
use amaker_core::models::{AuthoringSubstate, Project, ProjectId};
use amaker_core::services::AnswerProgress;

use crate::state::AppState;

/// A project plus its computed answer progress (if any) — the shape the
/// card template consumes.
pub struct ProjectCardView {
    pub project: Project,
    pub progress: Option<AnswerProgress>,
}

impl ProjectCardView {
    pub async fn for_project(state: &Arc<AppState>, project: Project) -> Self {
        let progress = state.responses.compute_progress(project.id).await;
        Self { project, progress }
    }
}

/// Project list partial template.
#[derive(Template)]
#[template(path = "partials/projects/list.html")]
pub struct ProjectListTemplate {
    pub cards: Vec<ProjectCardView>,
}

/// Project card partial template.
#[derive(Template)]
#[template(path = "partials/projects/card.html")]
pub struct ProjectCardTemplate {
    pub card: ProjectCardView,
}

/// Form data for creating a project.
#[derive(Deserialize)]
pub struct CreateProjectForm {
    pub name: String,
    pub description: Option<String>,
}

/// List all projects (HTMX partial).
pub async fn list(State(state): State<Arc<AppState>>) -> Result<Html<String>, AppError> {
    let projects = state.storage.list_projects().await?;
    let mut cards = Vec::with_capacity(projects.len());
    for project in projects {
        cards.push(ProjectCardView::for_project(&state, project).await);
    }
    let template = ProjectListTemplate { cards };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// Create a new project. Writes `project.json`, then initializes the
/// per-project git repo with a `.gitignore` + initial commit — the
/// single entry point that establishes the draft/publish invariant
/// that every project is a git repo.
pub async fn create(
    State(state): State<Arc<AppState>>,
    Form(form): Form<CreateProjectForm>,
) -> Result<Html<String>, AppError> {
    let project = Project::new(form.name, form.description);
    state.storage.save_project(&project).await?;

    let card = ProjectCardView::for_project(&state, project).await;
    let template = ProjectCardTemplate { card };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// Get a single project.
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let project = state.storage.load_project(project_id).await?;
    let card = ProjectCardView::for_project(&state, project).await;
    let template = ProjectCardTemplate { card };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// Delete a project.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    state.storage.delete_project(project_id).await?;
    Ok(Html(String::new()))
}

/// A single step in the authoring-substate stepper.
pub struct FocusStep {
    pub name: String,
    pub description: String,
    /// Wire value: snake_case substate (`"scoping"`, `"structuring"`, …).
    pub value: String,
    pub is_current: bool,
    pub is_completed: bool,
}

/// Authoring-substate indicator. Renders the four substates as a stepper.
/// Respond/Analyze are not states; they're separate audience routes
/// (`/respond/{id}`, `/analyze/{id}`) and don't appear here.
#[derive(Template)]
#[template(path = "partials/projects/phase_indicator.html")]
pub struct FocusIndicatorTemplate {
    pub project_id: ProjectId,
    pub phases: Vec<FocusStep>,
}

impl FocusIndicatorTemplate {
    pub fn for_project(project_id: ProjectId, project: &Project) -> Self {
        let current_index = AuthoringSubstate::all()
            .iter()
            .position(|s| *s == project.focus_substate)
            .unwrap_or(0);
        let phases: Vec<FocusStep> = AuthoringSubstate::all()
            .iter()
            .enumerate()
            .map(|(i, sub)| FocusStep {
                name: sub.display_name().to_string(),
                description: sub.description().to_string(),
                value: sub.wire_value().to_string(),
                is_current: project.focus_substate == *sub,
                is_completed: i < current_index,
            })
            .collect();
        Self { project_id, phases }
    }
}

/// Form data for updating focus. Wire value: snake_case substate name
/// (`"scoping"`, `"structuring"`, `"questions"`, `"refining"`).
#[derive(Deserialize)]
pub struct UpdateFocusForm {
    pub target_focus: String,
}

/// Get the authoring-substate indicator partial.
pub async fn get_phase(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let project = state.storage.load_project(project_id).await?;
    let template = FocusIndicatorTemplate::for_project(project_id, &project);
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// Update the project's authoring substate from the stepper UI.
pub async fn update_phase(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
    Form(form): Form<UpdateFocusForm>,
) -> Result<Response, AppError> {
    let mut project = state.storage.load_project(project_id).await?;

    let mut changed = false;
    if let Some(target_sub) = AuthoringSubstate::from_wire(&form.target_focus.to_lowercase())
        && target_sub != project.focus_substate
    {
        project.set_substate(target_sub);
        project.touch();
        state.storage.save_project(&project).await?;
        changed = true;
    }

    let template = FocusIndicatorTemplate::for_project(project_id, &project);
    let html = template
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Fire focusChanged so the preview panel (and anything else listening)
    // refreshes. Without this, switching via the stepper would update the
    // indicator but the preview pane would keep showing the old focus's view.
    let mut headers = HeaderMap::new();
    if changed {
        headers.insert("HX-Trigger", HeaderValue::from_static("focusChanged"));
    }

    Ok((headers, Html(html)).into_response())
}

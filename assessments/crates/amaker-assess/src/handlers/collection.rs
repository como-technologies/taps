//! Handlers for the Collection phase.
//!
//! Answers flow per-question via HTMX PATCH (upsert) and DELETE (clear).
//! A POST to /submit marks the response as submitted and advances the
//! project to Analysis.

use std::collections::HashMap;
use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Response},
};
use chrono::Utc;
use serde::Deserialize;

use crate::state::AppState;
use amaker_core::AppError;
use amaker_core::models::ids::{BlockerTypeId, EvidenceTypeId, QuestionId};
use amaker_core::models::{
    Answer, AnswerValue, Assessment, AssessmentResponse, BlockerType, EvidenceType, ProjectId,
};
use amaker_core::services::YamlService;

/// Full response form — the body of the `/respond/{id}` page.
#[derive(Template)]
#[template(path = "partials/collection/response_form.html")]
pub struct ResponseFormTemplate {
    pub project_id: ProjectId,
    pub assessment: Option<Assessment>,
    /// Keyed by `QuestionId`'s underlying `Uuid` so askama's `.get()` works
    /// with a plain `QuestionId` key at the call site.
    pub answers: HashMap<QuestionId, Answer>,
    pub answered: usize,
    pub total: usize,
    pub submitted: bool,
}

/// Per-question answer row (returned by PATCH / DELETE for swap-in-place).
#[derive(Template)]
#[template(path = "partials/collection/answer_row.html")]
pub struct AnswerRowTemplate {
    pub project_id: ProjectId,
    pub question: amaker_core::models::assessment::Question,
    pub answer: Option<Answer>,
    pub evidence_types: Vec<EvidenceType>,
    pub blocker_types: Vec<BlockerType>,
}

/// Tiny OOB chip showing answered / total.
#[derive(Template)]
#[template(path = "partials/collection/progress_badge.html")]
pub struct ProgressBadgeTemplate {
    pub answered: usize,
    pub total: usize,
}

/// Submit button + hint text. Rendered inside the form and OOB-swapped on
/// every answer change so the button's disabled state tracks the live
/// answer count, not the count at page load.
#[derive(Template)]
#[template(path = "partials/collection/response_footer.html")]
pub struct ResponseFooterTemplate {
    pub project_id: ProjectId,
    pub answered: usize,
    pub submitted: bool,
}

/// Form payload from the per-question HTMX PATCH. Uses the html-form parser
/// (axum-extra) so repeated checkbox keys hydrate into a Vec.
#[derive(Debug, Deserialize)]
pub struct AnswerForm {
    pub value: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub blocker_ids: Vec<String>,
    pub planned: Option<String>,
    pub notes: Option<String>,
}

/// Upsert (or clear) one answer. Returns the re-rendered answer row plus
/// an OOB-swap progress chip.
pub async fn upsert_answer(
    State(state): State<Arc<AppState>>,
    Path((project_id, question_id)): Path<(ProjectId, QuestionId)>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<AnswerForm>,
) -> Result<Response, AppError> {
    let (_response, assessment) = load_response_and_assessment(&state, project_id).await?;

    match form.value.as_deref() {
        None | Some("") => {
            state
                .responses
                .clear_answer(project_id, question_id)
                .await?;
        }
        Some(value) => {
            let value = parse_answer_value(value)?;
            let polarity = find_question(&assessment, question_id)
                .ok_or_else(|| AppError::NotFound(format!("Question {} not found", question_id)))?
                .polarity;
            let planned = form
                .planned
                .as_deref()
                .map(|p| matches!(p, "true" | "on" | "1"));
            let notes = form
                .notes
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            // Build a scratch answer to test the polarity-adjusted outcome,
            // then only persist the supporting fields that match that
            // outcome. Pass → evidence; Fail → blockers + planned;
            // Unknown → neither (notes only).
            let scratch = Answer {
                value,
                evidence_ids: Vec::new(),
                blocker_ids: Vec::new(),
                planned: None,
                notes: notes.clone(),
                answered_at: Utc::now(),
            };
            let (evidence_ids, blocker_ids, planned_out) = if scratch.resolves_to_pass(&polarity) {
                (
                    form.evidence_ids
                        .into_iter()
                        .map(EvidenceTypeId::new)
                        .collect(),
                    Vec::new(),
                    None,
                )
            } else if scratch.resolves_to_fail(&polarity) {
                (
                    Vec::new(),
                    form.blocker_ids
                        .into_iter()
                        .map(BlockerTypeId::new)
                        .collect(),
                    planned.or(Some(false)),
                )
            } else {
                (Vec::new(), Vec::new(), None)
            };
            let answer = Answer {
                value,
                evidence_ids,
                blocker_ids,
                planned: planned_out,
                notes,
                answered_at: Utc::now(),
            };
            state
                .responses
                .upsert_answer(project_id, question_id, answer)
                .await?;
        }
    }

    render_row_and_progress(&state, project_id, question_id, assessment).await
}

/// Clear one answer.
pub async fn clear_answer(
    State(state): State<Arc<AppState>>,
    Path((project_id, question_id)): Path<(ProjectId, QuestionId)>,
) -> Result<Response, AppError> {
    // ensure_primary creates a response if missing (binding to latest publish);
    // load_response_and_assessment then loads the bound assessment.
    let (_response, assessment) = load_response_and_assessment(&state, project_id).await?;
    state
        .responses
        .clear_answer(project_id, question_id)
        .await
        .ok(); // ignore "not found" — idempotent from the UI's perspective
    render_row_and_progress(&state, project_id, question_id, assessment).await
}

/// GET the full response form (used both by the preview route on Collection
/// entry and for explicit refresh).
pub async fn get_response_form(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let html = render_response_form(&state, project_id).await?;
    Ok(Html(html))
}

/// Submit the current response — marks submitted_at and switches focus to
/// Analyzing. Fires HX triggers so the sidebar focus indicator + preview
/// pane both refresh.
pub async fn submit_response(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Response, AppError> {
    // Load + mark submitted (create if missing). Project state is left
    // alone — "the respondent is done" is now derived from the response
    // file's `submitted_at`, not from a focus mutation. The respondent UI
    // redirects to `/analyze/{id}` after success via the HX trigger below.
    let (mut response, _assessment) = load_response_and_assessment(&state, project_id).await?;
    if response.submitted_at.is_none() {
        response.submitted_at = Some(Utc::now());
        state.responses.save_primary(project_id, &response).await?;
        tracing::info!(
            %project_id,
            answered = response.answers.len(),
            "response submitted"
        );
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        "HX-Trigger",
        HeaderValue::from_static("responseSubmitted, assessmentUpdated"),
    );
    let html = render_response_form(&state, project_id).await?;
    Ok((headers, Html(html)).into_response())
}

// ============================================================================
// Helpers
// ============================================================================

fn find_question(
    assessment: &Assessment,
    question_id: QuestionId,
) -> Option<&amaker_core::models::assessment::Question> {
    assessment
        .domains
        .iter()
        .flat_map(|d| &d.practices)
        .flat_map(|p| &p.questions)
        .find(|q| q.id == question_id)
}

/// Ensure the primary response exists (creating it bound to the latest
/// published version when missing) and load the assessment shape at that
/// response's bound git tag. Validates that every answered question UUID
/// still exists in the bound assessment — mismatch is a hard error per
/// `docs/src/architecture/draft-publish.md`.
async fn load_response_and_assessment(
    state: &Arc<AppState>,
    project_id: ProjectId,
) -> Result<(AssessmentResponse, Assessment), AppError> {
    let response = state.responses.ensure_primary(project_id).await?;
    let yaml = state
        .storage
        .load_assessment_yaml_at(project_id, &response.version)
        .await?;
    let assessment = YamlService::parse_assessment(&yaml).map_err(|e| {
        AppError::Internal(format!(
            "parse assessment at version '{}': {}",
            response.version, e
        ))
    })?;
    response.validate_against(&assessment)?;
    Ok((response, assessment))
}

fn parse_answer_value(s: &str) -> Result<AnswerValue, AppError> {
    match s {
        "yes" => Ok(AnswerValue::Yes),
        "no" => Ok(AnswerValue::No),
        "unknown" => Ok(AnswerValue::Unknown),
        other => Err(AppError::ParseError(format!(
            "Invalid answer value: {}",
            other
        ))),
    }
}

// `AnswerProgress` + `compute_progress` live in
// `amaker_core::services::responses` so any audience binary can show a
// progress chip. The author binary uses it on the home-card list.

/// Produce the full response-form HTML for the preview pane. When no
/// version has been published yet `ensure_primary` returns a friendly
/// `BadRequest`; render the empty form so the chrome can show the
/// "publish before collecting" guidance instead of a 4xx page.
pub async fn render_response_form(
    state: &Arc<AppState>,
    project_id: ProjectId,
) -> Result<String, AppError> {
    let loaded = load_response_and_assessment(state, project_id).await;
    let (assessment, response) = match loaded {
        Ok((response, assessment)) => (Some(assessment), Some(response)),
        Err(AppError::BadRequest(msg)) => {
            tracing::info!("render_response_form: pre-publish state — {}", msg);
            (None, None)
        }
        Err(e) => return Err(e),
    };

    let total = assessment.as_ref().map(|a| a.question_count()).unwrap_or(0);
    let answers = response
        .as_ref()
        .map(|r| r.answers.clone())
        .unwrap_or_default();
    let answered = answers.len();
    let submitted = response.as_ref().and_then(|r| r.submitted_at).is_some();

    let tpl = ResponseFormTemplate {
        project_id,
        assessment,
        answers,
        answered,
        total,
        submitted,
    };
    tpl.render().map_err(|e| AppError::Internal(e.to_string()))
}

/// After a PATCH/DELETE, emit the updated answer row + an OOB progress chip.
async fn render_row_and_progress(
    state: &Arc<AppState>,
    project_id: ProjectId,
    question_id: QuestionId,
    assessment: Assessment,
) -> Result<Response, AppError> {
    let question = find_question(&assessment, question_id)
        .ok_or_else(|| AppError::NotFound(format!("Question {} not found", question_id)))?
        .clone();

    let response = state.responses.load_primary(project_id).await?;
    let answer = response
        .as_ref()
        .and_then(|r| r.answers.get(&question_id).cloned());

    let row = AnswerRowTemplate {
        project_id,
        question,
        answer,
        evidence_types: assessment.evidence_types.clone(),
        blocker_types: assessment.blocker_types.clone(),
    }
    .render()
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let total = assessment.question_count();
    let answered = response.as_ref().map(|r| r.answers.len()).unwrap_or(0);
    let progress = ProgressBadgeTemplate { answered, total }
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // OOB swap: the progress chip's outer element has `id="response-progress"`.
    // Emitting it with `hx-swap-oob="true"` replaces whatever chip is currently
    // in the DOM without affecting the main target.
    let oob_progress = progress.replace(
        "<span id=\"response-progress\"",
        "<span id=\"response-progress\" hx-swap-oob=\"true\"",
    );

    let submitted = response.as_ref().and_then(|r| r.submitted_at).is_some();
    let footer = ResponseFooterTemplate {
        project_id,
        answered,
        submitted,
    }
    .render()
    .map_err(|e| AppError::Internal(e.to_string()))?;
    let oob_footer = footer.replace(
        "<div id=\"response-footer\"",
        "<div id=\"response-footer\" hx-swap-oob=\"true\"",
    );

    Ok(Html(format!("{}\n{}\n{}", row, oob_progress, oob_footer)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use amaker_core::models::AssessmentResponse;
    use amaker_core::models::assessment::{Domain, Practice, Question};
    use amaker_core::models::ids::RespondentId;

    fn sample_assessment() -> Assessment {
        let mut a = Assessment::new("Test".into(), "Desc".into(), "Goal".into());
        let mut d = Domain::new("D".into(), "c".into(), "v".into(), "r".into());
        let mut p = Practice::new("P".into(), "c".into(), "v".into(), "r".into());
        p.questions.push(Question::new("Q1?".into()));
        p.questions.push(Question::new("Q2?".into()));
        d.practices.push(p);
        a.domains.push(d);
        a
    }

    #[test]
    fn response_form_renders_with_empty_response() {
        let a = sample_assessment();
        let response = AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string());
        let tpl = ResponseFormTemplate {
            project_id: ProjectId::new(),
            total: a.question_count(),
            answered: response.answers.len(),
            submitted: false,
            assessment: Some(a),
            answers: response.answers,
        };
        let html = tpl.render().expect("response_form renders");
        assert!(html.contains("Test"));
        assert!(html.contains("Submit Response"));
        assert!(html.contains("0 / 2 answered"));
    }

    #[test]
    fn response_form_renders_with_one_yes_answer() {
        let a = sample_assessment();
        let q_id = a.domains[0].practices[0].questions[0].id;
        let mut response = AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string());
        let mut answer = Answer::new(AnswerValue::Yes);
        answer.evidence_ids.push(a.evidence_types[1].id.clone());
        response.upsert_answer(q_id, answer);

        let tpl = ResponseFormTemplate {
            project_id: ProjectId::new(),
            total: a.question_count(),
            answered: response.answers.len(),
            submitted: false,
            assessment: Some(a),
            answers: response.answers,
        };
        let html = tpl.render().expect("response_form renders");
        assert!(html.contains("1 / 2 answered"));
        // Evidence checkbox for the selected type should be rendered checked.
        assert!(html.contains("checked"));
    }
}

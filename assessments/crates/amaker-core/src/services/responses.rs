//! High-level operations on `AssessmentResponse`s.
//!
//! Wraps [`StorageService`] with semantic helpers — "load the primary
//! response", "upsert an answer", "ensure a primary response exists" —
//! so handlers and tools don't have to think about filenames or the
//! single-vs-multi-respondent split.
//!
//! For v1 we use a single default respondent per project, identified by
//! [`primary_respondent_id`]. The storage layer treats responses as a
//! keyed set, so multi-respondent is purely additive later.
//!
//! Responses bind to a *published version* at creation. The
//! loader recovers the assessment shape via
//! [`StorageService::load_assessment_yaml_at`] — see
//! `docs/src/architecture/draft-publish.md` "Responses & answers".

#![allow(dead_code)]

use std::sync::OnceLock;

use uuid::Uuid;

use crate::error::AppError;
use crate::models::ids::{QuestionId, RespondentId};
use crate::models::{Answer, Assessment, AssessmentResponse, ProjectId};
use crate::services::StorageService;

/// A stable well-known respondent ID used as the default single-user
/// respondent in v1.
///
/// `00000000-0000-0000-0000-000000000001` — chosen to be obviously
/// synthetic if someone stumbles across it on disk.
pub fn primary_respondent_id() -> RespondentId {
    static PRIMARY: OnceLock<RespondentId> = OnceLock::new();
    *PRIMARY.get_or_init(|| {
        let uuid = Uuid::parse_str("00000000-0000-0000-0000-000000000001")
            .expect("hard-coded primary respondent UUID is valid");
        RespondentId::from(uuid)
    })
}

/// Semantic wrapper around response storage.
#[derive(Clone)]
pub struct ResponseService {
    storage: StorageService,
}

impl ResponseService {
    pub fn new(storage: StorageService) -> Self {
        Self { storage }
    }

    /// Load the primary (single-user v1) response for a project.
    pub async fn load_primary(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<AssessmentResponse>, AppError> {
        self.storage
            .load_response(project_id, &primary_respondent_id())
            .await
    }

    /// Save the primary response for a project (creates or overwrites).
    pub async fn save_primary(
        &self,
        project_id: ProjectId,
        response: &AssessmentResponse,
    ) -> Result<(), AppError> {
        self.storage.save_response(project_id, response).await
    }

    /// Initialize (or reuse) a primary response.
    ///
    /// If no response exists, binds a fresh one to the *latest* published
    /// version and saves it. The assessment id is read from the assessment
    /// YAML at that version, so the response always self-describes which
    /// assessment it answers.
    ///
    /// Errors with `BadRequest` if no version has been published yet —
    /// per `docs/src/architecture/draft-publish.md`, responses must
    /// follow a publish.
    pub async fn ensure_primary(
        &self,
        project_id: ProjectId,
    ) -> Result<AssessmentResponse, AppError> {
        if let Some(existing) = self.load_primary(project_id).await? {
            return Ok(existing);
        }
        let version = self
            .storage
            .latest_version(project_id)
            .await?
            .ok_or_else(|| {
                AppError::BadRequest(
                    "No published version exists yet. Publish the assessment before \
                     respondents start answering."
                        .to_string(),
                )
            })?
            .name;
        let yaml = self
            .storage
            .load_assessment_yaml_at(project_id, &version)
            .await?;
        let assessment: Assessment = serde_yaml::from_str(&yaml).map_err(|e| {
            AppError::Internal(format!("parse assessment at version '{version}': {e}"))
        })?;
        let response = AssessmentResponse::new(assessment.id, primary_respondent_id(), version);
        self.save_primary(project_id, &response).await?;
        Ok(response)
    }

    /// Load the published assessment-at-version + its primary response,
    /// parsing the YAML and validating that every answer key still exists
    /// in the assessment's question tree. Used by analysis handlers that
    /// need both halves of the pair in one shot.
    pub async fn load_primary_context(
        &self,
        project_id: ProjectId,
    ) -> Result<(Assessment, AssessmentResponse), AppError> {
        let response = self.ensure_primary(project_id).await?;
        let yaml = self
            .storage
            .load_assessment_yaml_at(project_id, &response.version)
            .await?;
        let assessment: Assessment = serde_yaml::from_str(&yaml).map_err(|e| {
            AppError::Internal(format!(
                "parse assessment at version '{}': {}",
                response.version, e
            ))
        })?;
        response.validate_against(&assessment)?;
        Ok((assessment, response))
    }

    /// Upsert one answer and persist the whole response.
    pub async fn upsert_answer(
        &self,
        project_id: ProjectId,
        question_id: QuestionId,
        answer: Answer,
    ) -> Result<AssessmentResponse, AppError> {
        let mut response = self.load_primary(project_id).await?.ok_or_else(|| {
            AppError::NotFound(format!(
                "No response initialized for project {}",
                project_id
            ))
        })?;
        response.upsert_answer(question_id, answer);
        self.save_primary(project_id, &response).await?;
        Ok(response)
    }

    /// Clear one answer and persist. No-op if the answer wasn't set.
    pub async fn clear_answer(
        &self,
        project_id: ProjectId,
        question_id: QuestionId,
    ) -> Result<AssessmentResponse, AppError> {
        let mut response = self.load_primary(project_id).await?.ok_or_else(|| {
            AppError::NotFound(format!(
                "No response initialized for project {}",
                project_id
            ))
        })?;
        response.clear_answer(&question_id);
        self.save_primary(project_id, &response).await?;
        Ok(response)
    }

    /// Summary of an in-progress response — `None` once submitted or
    /// when no response exists. Used by the home-page card chip. Lives
    /// in core so any audience binary (today: author for the chip;
    /// later: a respondent-portal dashboard) can call it.
    pub async fn compute_progress(&self, project_id: ProjectId) -> Option<AnswerProgress> {
        let response = self.load_primary(project_id).await.ok().flatten()?;
        if response.submitted_at.is_some() {
            return None;
        }
        let yaml = self
            .storage
            .load_assessment_yaml_at(project_id, &response.version)
            .await
            .ok()?;
        let assessment: Assessment = serde_yaml::from_str(&yaml).ok()?;
        let total = assessment.question_count();
        if total == 0 {
            return None;
        }
        Some(AnswerProgress {
            answered: response.answers.len(),
            total,
        })
    }
}

/// Answer-count snapshot for a project. `None` from `compute_progress`
/// means there's no in-progress response (either never started, or
/// already submitted).
#[derive(Debug, Clone)]
pub struct AnswerProgress {
    pub answered: usize,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::assessment::{Domain, Practice};
    use crate::models::{AnswerValue, Project};
    use crate::storage_backend::{StorageBackend, build_store};

    /// Build an in-memory service with a project seeded and a published
    /// `v1` so `ensure_primary` has something to bind to.
    async fn mk_service_with_published_v1() -> (ResponseService, ProjectId) {
        let store = build_store(&StorageBackend::InMemory).unwrap();
        let storage = StorageService::new(store);
        storage.init().await.unwrap();
        let project = Project::new("test".into(), None);
        let project_id = project.id;
        storage.save_project(&project).await.unwrap();

        let mut a = Assessment::new("A".into(), "d".into(), "g".into());
        let mut d = Domain::new("D".into(), "c".into(), "v".into(), "r".into());
        d.practices.push(Practice::new(
            "P".into(),
            "c".into(),
            "v".into(),
            "r".into(),
        ));
        a.domains.push(d);
        let yaml = a.to_yaml().unwrap();
        storage
            .save_assessment_yaml(project_id, &yaml)
            .await
            .unwrap();
        storage
            .publish_version(project_id, "v1", Some("first publish"))
            .await
            .unwrap();

        (ResponseService::new(storage), project_id)
    }

    #[tokio::test]
    async fn ensure_primary_creates_bound_to_latest_version() {
        let (svc, project_id) = mk_service_with_published_v1().await;
        let response = svc.ensure_primary(project_id).await.unwrap();
        assert_eq!(response.version, "v1");
        assert!(response.is_empty());

        let again = svc.ensure_primary(project_id).await.unwrap();
        assert_eq!(again.version, "v1");
    }

    #[tokio::test]
    async fn ensure_primary_errors_when_no_publish_exists() {
        let store = build_store(&StorageBackend::InMemory).unwrap();
        let storage = StorageService::new(store);
        storage.init().await.unwrap();
        let project = Project::new("test".into(), None);
        let project_id = project.id;
        storage.save_project(&project).await.unwrap();

        let svc = ResponseService::new(storage);
        let err = svc.ensure_primary(project_id).await.unwrap_err();
        match err {
            AppError::BadRequest(msg) => assert!(
                msg.contains("Publish") || msg.contains("publish"),
                "expected 'publish first' guidance, got: {msg}"
            ),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn round_trip_answers() {
        let (svc, project_id) = mk_service_with_published_v1().await;
        svc.ensure_primary(project_id).await.unwrap();

        let q1 = QuestionId::new();
        let q2 = QuestionId::new();

        svc.upsert_answer(project_id, q1, Answer::new(AnswerValue::Yes))
            .await
            .unwrap();
        svc.upsert_answer(project_id, q2, Answer::new(AnswerValue::No))
            .await
            .unwrap();

        let reloaded = svc.load_primary(project_id).await.unwrap().unwrap();
        assert_eq!(reloaded.answers.len(), 2);
        assert_eq!(reloaded.answers[&q1].value, AnswerValue::Yes);
        assert_eq!(reloaded.answers[&q2].value, AnswerValue::No);

        svc.clear_answer(project_id, q1).await.unwrap();
        let reloaded = svc.load_primary(project_id).await.unwrap().unwrap();
        assert_eq!(reloaded.answers.len(), 1);
        assert!(!reloaded.answers.contains_key(&q1));
    }
}

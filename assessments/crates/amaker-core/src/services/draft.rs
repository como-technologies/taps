//! Surgical-edit helper for the live draft (`draft.yaml`).
//!
//! Every CRUD tool follows the same shape: load the draft (with its
//! version handle), parse, mutate the in-memory `Assessment`, serialize,
//! save conditional on that version. On a concurrent writer winning the
//! race the conditional save fails with `AppError::Conflict` and the error
//! propagates — the caller (typically a tool returning to the agent loop)
//! decides whether to surface or re-issue. We deliberately don't do
//! internal retry here so tool closures stay simple (`FnOnce`, free to
//! move captured state).

use crate::error::AppError;
use crate::models::{Assessment, ProjectId};
use crate::services::{StorageService, YamlService};

/// Load the draft, apply `mutate`, write back conditional on the loaded
/// version. Returns the post-edit `Assessment` plus whatever owned data
/// the mutator yields (entity name, count, etc.).
///
/// Errors:
/// - `BadRequest` if no draft exists yet (caller should run
///   `generate_structure` first).
/// - `Conflict` if a concurrent writer landed between load and save. The
///   tool that surfaces this can be invoked again with the same args; the
///   second attempt will see the updated state.
/// - Whatever `mutate` returns, propagated unchanged.
pub async fn edit_yaml<F, T>(
    storage: &StorageService,
    project_id: ProjectId,
    mutate: F,
) -> Result<(Assessment, T), AppError>
where
    F: FnOnce(&mut Assessment) -> Result<T, AppError>,
{
    let (yaml, version) = storage
        .load_assessment_yaml_versioned(project_id)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest("No draft assessment yet. Call generate_structure first.".into())
        })?;
    let mut assessment = YamlService::parse_assessment(&yaml)?;
    let extracted = mutate(&mut assessment)?;
    assessment.updated_at = chrono::Utc::now();
    let new_yaml = assessment
        .to_yaml()
        .map_err(|e| AppError::Internal(format!("serialize assessment: {}", e)))?;
    storage
        .save_assessment_yaml_if_match(project_id, &new_yaml, Some(version))
        .await?;
    Ok((assessment, extracted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Project;
    use crate::models::assessment::{Domain, Practice, Question};
    use crate::storage_backend::{StorageBackend, build_store};

    async fn mk_storage() -> (StorageService, ProjectId) {
        let store = build_store(&StorageBackend::InMemory).unwrap();
        let storage = StorageService::new(store);
        storage.init().await.unwrap();
        let project = Project::new("Test".to_string(), None);
        storage.save_project(&project).await.unwrap();
        (storage, project.id)
    }

    fn mk_assessment() -> Assessment {
        let mut assessment = Assessment::new("A".into(), "d".into(), "g".into());
        let mut domain = Domain::new("D".into(), "c".into(), "v".into(), "r".into());
        let mut practice = Practice::new("P".into(), "c".into(), "v".into(), "r".into());
        practice.questions.push(Question::new("Q1?".into()));
        domain.practices.push(practice);
        assessment.domains.push(domain);
        assessment
    }

    #[tokio::test]
    async fn edit_yaml_mutates_and_persists() {
        let (storage, project_id) = mk_storage().await;
        let yaml = mk_assessment().to_yaml().unwrap();
        storage
            .save_assessment_yaml(project_id, &yaml)
            .await
            .unwrap();

        let (after, prev_name) = edit_yaml(&storage, project_id, |a| {
            let prev = a.name.clone();
            a.name = "Renamed".into();
            Ok(prev)
        })
        .await
        .unwrap();

        assert_eq!(after.name, "Renamed");
        assert_eq!(prev_name, "A");
        let reloaded = storage
            .load_assessment_yaml(project_id)
            .await
            .unwrap()
            .unwrap();
        assert!(reloaded.contains("Renamed"));
    }

    #[tokio::test]
    async fn edit_yaml_propagates_mutator_error_without_writing() {
        let (storage, project_id) = mk_storage().await;
        let yaml = mk_assessment().to_yaml().unwrap();
        storage
            .save_assessment_yaml(project_id, &yaml)
            .await
            .unwrap();
        let before = storage
            .load_assessment_yaml(project_id)
            .await
            .unwrap()
            .unwrap();

        let err = edit_yaml(&storage, project_id, |_| {
            Err::<(), _>(AppError::NotFound("nope".into()))
        })
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));

        let after = storage
            .load_assessment_yaml(project_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn edit_yaml_refuses_when_no_draft_exists() {
        let (storage, project_id) = mk_storage().await;
        let err = edit_yaml(&storage, project_id, |_| Ok::<(), AppError>(()))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}

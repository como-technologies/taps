//! Storage service — a thin facade over an `object_store::ObjectStore`.
//!
//! Every project lives at the `projects/{project_id}/` prefix. Within
//! that prefix:
//!
//! ```text
//! project.json                  # mutable; the project envelope
//! draft.yaml                    # mutable; the live working copy
//! versions/{name}.yaml          # immutable; published snapshot of draft.yaml
//! versions/{name}.meta.json     # immutable; { published_at, notes? }
//! responses/{respondent}.yaml   # mutable; per-respondent answers
//! chat.json                     # mutable; conversation transcript
//! analysis/report.md            # cached; regenerable
//! analysis/scorecard.json       # cached; regenerable
//! analysis/gaps.json            # cached; regenerable
//! uploads/{doc_id}_{filename}   # opaque blob
//! ```
//!
//! Concurrency: most save methods are unconditional (last-writer-wins),
//! which matches the prior per-process-mutex behavior. The draft path
//! exposes ETag-aware load + conditional save so `draft::edit_yaml` can
//! retry on conflict — that's the only place high-frequency concurrent
//! writes from the agent loop are realistic. Publishing a version uses
//! `PutMode::Create` so a duplicate name fails atomically.

use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use object_store::path::Path as ObjectPath;
use object_store::{
    GetOptions, ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload, UpdateVersion,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::ids::RespondentId;
use crate::models::{AssessmentResponse, Conversation, DocumentId, Project, ProjectId};

/// Storage service backed by an `object_store::ObjectStore`. Cheap to clone.
#[derive(Clone)]
pub struct StorageService {
    store: Arc<dyn ObjectStore>,
}

/// Metadata recorded alongside each published version blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionMeta {
    pub published_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Returned by `list_versions` / `latest_version`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub name: String,
    pub published_at: DateTime<Utc>,
    #[serde(default)]
    pub notes: Option<String>,
}

impl StorageService {
    /// Wrap an `ObjectStore`. The backend is whatever
    /// `amaker_core::build_store(&config)` returned.
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    /// No-op kept for API stability with callers (binaries call this at
    /// startup). Object stores need no per-startup initialization.
    pub async fn init(&self) -> Result<(), AppError> {
        Ok(())
    }

    // ===== Project Operations =====

    /// Save a project envelope. Unconditional (last-writer-wins). The
    /// `project_lock` mutex pattern that previously protected concurrent
    /// `save_project` is gone; in practice this object is rarely
    /// contended.
    pub async fn save_project(&self, project: &Project) -> Result<(), AppError> {
        let key = project_key(project.id);
        let bytes = serde_json::to_vec_pretty(project)?;
        self.store
            .put(&key, PutPayload::from(Bytes::from(bytes)))
            .await
            .map_err(map_put_err)?;
        Ok(())
    }

    pub async fn load_project(&self, project_id: ProjectId) -> Result<Project, AppError> {
        let key = project_key(project_id);
        let bytes = self
            .fetch_bytes(&key)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Project {} not found", project_id)))?;
        let project: Project = serde_json::from_slice(&bytes)?;
        Ok(project)
    }

    /// Enumerate every project envelope, newest `updated_at` first.
    pub async fn list_projects(&self) -> Result<Vec<Project>, AppError> {
        let prefix = ObjectPath::from("projects");
        let list = self
            .store
            .list_with_delimiter(Some(&prefix))
            .await
            .map_err(|e| AppError::Internal(format!("list projects: {e}")))?;
        let mut projects = Vec::new();
        for project_prefix in list.common_prefixes {
            let project_json = ObjectPath::from(format!("{}/project.json", project_prefix));
            if let Some(bytes) = self.fetch_bytes(&project_json).await?
                && let Ok(project) = serde_json::from_slice::<Project>(&bytes)
            {
                projects.push(project);
            }
        }
        projects.sort_by_key(|p| std::cmp::Reverse(p.updated_at));
        Ok(projects)
    }

    /// Delete every object under `projects/{id}/`. Best-effort: keeps
    /// going on per-object errors and returns the first failure (if any).
    pub async fn delete_project(&self, project_id: ProjectId) -> Result<(), AppError> {
        let prefix = project_prefix(project_id);
        let mut stream = self.store.list(Some(&prefix));
        let mut keys = Vec::new();
        while let Some(meta) = stream.next().await {
            let meta =
                meta.map_err(|e| AppError::Internal(format!("list project objects: {e}")))?;
            keys.push(meta.location);
        }
        for key in keys {
            self.store
                .delete(&key)
                .await
                .map_err(|e| AppError::Internal(format!("delete {}: {e}", key)))?;
        }
        Ok(())
    }

    // ===== Conversation Operations =====

    pub async fn save_conversation(&self, conv: &Conversation) -> Result<(), AppError> {
        let key = chat_key(conv.project_id);
        let bytes = serde_json::to_vec_pretty(conv)?;
        self.store
            .put(&key, PutPayload::from(Bytes::from(bytes)))
            .await
            .map_err(map_put_err)?;
        Ok(())
    }

    pub async fn load_conversation(&self, project_id: ProjectId) -> Result<Conversation, AppError> {
        let key = chat_key(project_id);
        match self.fetch_bytes(&key).await? {
            Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
            None => Ok(Conversation::new(project_id)),
        }
    }

    // ===== Assessment YAML (the live draft) =====

    /// Save the live draft assessment. Unconditional. Use
    /// `save_assessment_yaml_if_match` for compare-and-swap.
    pub async fn save_assessment_yaml(
        &self,
        project_id: ProjectId,
        yaml: &str,
    ) -> Result<(), AppError> {
        let key = draft_key(project_id);
        self.store
            .put(&key, PutPayload::from(Bytes::from(yaml.to_owned())))
            .await
            .map_err(map_put_err)?;
        Ok(())
    }

    pub async fn load_assessment_yaml(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<String>, AppError> {
        let key = draft_key(project_id);
        match self.fetch_bytes(&key).await? {
            Some(bytes) => {
                Ok(Some(String::from_utf8(bytes.to_vec()).map_err(|e| {
                    AppError::Internal(format!("draft.yaml not utf-8: {e}"))
                })?))
            }
            None => Ok(None),
        }
    }

    /// Load the live draft alongside the version handle a subsequent
    /// `save_assessment_yaml_if_match` needs for a conditional write.
    /// Returns `None` when no draft exists yet (pre-`generate_structure`).
    ///
    /// The handle carries *both* the ETag and the store-native version
    /// (the GCS generation, the S3 version-id, …). Different backends
    /// condition on different fields — S3 and the local filesystem use
    /// the ETag, GCS requires the generation — so we capture the whole
    /// `UpdateVersion` and hand it back unchanged.
    pub async fn load_assessment_yaml_versioned(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<(String, UpdateVersion)>, AppError> {
        let key = draft_key(project_id);
        match self.store.get(&key).await {
            Ok(result) => {
                let version = UpdateVersion {
                    e_tag: result.meta.e_tag.clone(),
                    version: result.meta.version.clone(),
                };
                let bytes = result
                    .bytes()
                    .await
                    .map_err(|e| AppError::Internal(format!("read draft body: {e}")))?;
                let yaml = String::from_utf8(bytes.to_vec())
                    .map_err(|e| AppError::Internal(format!("draft.yaml not utf-8: {e}")))?;
                Ok(Some((yaml, version)))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(AppError::Internal(format!("load draft: {e}"))),
        }
    }

    /// Conditional write of the draft. When `expected` is `Some(v)`, the
    /// put only succeeds if the object still matches that version handle
    /// (from `load_assessment_yaml_versioned`); on mismatch returns
    /// `AppError::Conflict`. When `None`, behaves like
    /// `save_assessment_yaml` (unconditional).
    pub async fn save_assessment_yaml_if_match(
        &self,
        project_id: ProjectId,
        yaml: &str,
        expected: Option<UpdateVersion>,
    ) -> Result<(), AppError> {
        let key = draft_key(project_id);
        let mode = match expected {
            Some(version) => PutMode::Update(version),
            None => PutMode::Overwrite,
        };
        let payload = PutPayload::from(Bytes::from(yaml.to_owned()));
        let opts = PutOptions {
            mode,
            ..Default::default()
        };
        match self.store.put_opts(&key, payload, opts).await {
            Ok(_) => Ok(()),
            Err(object_store::Error::Precondition { .. }) => Err(AppError::Conflict(format!(
                "draft.yaml for project {} was modified by another writer; retry",
                project_id
            ))),
            Err(e) => Err(AppError::Internal(format!("save draft: {e}"))),
        }
    }

    /// Read the assessment YAML at a published version. Hard error if
    /// the version doesn't exist.
    pub async fn load_assessment_yaml_at(
        &self,
        project_id: ProjectId,
        version: &str,
    ) -> Result<String, AppError> {
        let key = version_yaml_key(project_id, version);
        let bytes = self.fetch_bytes(&key).await?.ok_or_else(|| {
            AppError::NotFound(format!(
                "Version '{}' not found for project {}",
                version, project_id
            ))
        })?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| AppError::Internal(format!("version {} yaml not utf-8: {e}", version)))
    }

    // ===== Versions =====

    /// All published versions for a project, newest first by
    /// `published_at`. Versions without a `.meta.json` are skipped
    /// silently (orphaned content from a crashed publish; safe to ignore).
    pub async fn list_versions(&self, project_id: ProjectId) -> Result<Vec<VersionInfo>, AppError> {
        let prefix = versions_prefix(project_id);
        let mut stream = self.store.list(Some(&prefix));
        let mut versions = Vec::new();
        while let Some(meta) = stream.next().await {
            let meta = meta.map_err(|e| AppError::Internal(format!("list versions: {e}")))?;
            let key_str = meta.location.to_string();
            if let Some(name) = key_str
                .strip_suffix(".meta.json")
                .and_then(|p| p.rsplit('/').next().filter(|n| !n.is_empty()))
                && let Some(bytes) = self.fetch_bytes(&meta.location).await?
                && let Ok(m) = serde_json::from_slice::<VersionMeta>(&bytes)
            {
                versions.push(VersionInfo {
                    name: name.to_string(),
                    published_at: m.published_at,
                    notes: m.notes,
                });
            }
        }
        versions.sort_by_key(|v| std::cmp::Reverse(v.published_at));
        Ok(versions)
    }

    /// Most recently published version (by `published_at`), or `None`.
    pub async fn latest_version(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<VersionInfo>, AppError> {
        Ok(self.list_versions(project_id).await?.into_iter().next())
    }

    /// Snapshot the current `draft.yaml` as a new published version. Uses
    /// `PutMode::Create` on both the content and meta blobs so a duplicate
    /// name fails atomically without overwriting anything.
    pub async fn publish_version(
        &self,
        project_id: ProjectId,
        name: &str,
        notes: Option<&str>,
    ) -> Result<(), AppError> {
        validate_version_name(name)?;
        let draft = self
            .load_assessment_yaml(project_id)
            .await?
            .ok_or_else(|| {
                AppError::BadRequest("No draft to publish; call generate_structure first.".into())
            })?;
        let yaml_key = version_yaml_key(project_id, name);
        let meta_key = version_meta_key(project_id, name);

        // Content blob first (Create). If this fails because the name is
        // taken, we abort cleanly.
        self.store
            .put_opts(
                &yaml_key,
                PutPayload::from(Bytes::from(draft)),
                PutOptions {
                    mode: PutMode::Create,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| match e {
                object_store::Error::AlreadyExists { .. } => AppError::BadRequest(format!(
                    "Version '{}' already exists for project {}",
                    name, project_id
                )),
                other => AppError::Internal(format!("write version content: {other}")),
            })?;

        // Meta blob (Create). A crash between the two writes leaves an
        // orphan content blob discoverable by `list_versions` (which
        // ignores entries lacking meta); harmless and reclaimable.
        let meta = VersionMeta {
            published_at: Utc::now(),
            notes: notes.map(|s| s.to_string()),
        };
        let meta_bytes = serde_json::to_vec_pretty(&meta)?;
        self.store
            .put_opts(
                &meta_key,
                PutPayload::from(Bytes::from(meta_bytes)),
                PutOptions {
                    mode: PutMode::Create,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| AppError::Internal(format!("write version meta: {e}")))?;
        Ok(())
    }

    /// Overwrite `draft.yaml` with the contents of `versions/{version}.yaml`.
    /// Last-writer-wins on the draft side — concurrent surgical edits in
    /// flight will fail on their next conditional save and retry against
    /// the reset content.
    pub async fn reset_draft_from_version(
        &self,
        project_id: ProjectId,
        version: &str,
    ) -> Result<(), AppError> {
        let content = self.load_assessment_yaml_at(project_id, version).await?;
        self.save_assessment_yaml(project_id, &content).await
    }

    // ===== Documents =====

    pub async fn save_document(
        &self,
        project_id: ProjectId,
        doc_id: DocumentId,
        filename: &str,
        content: &[u8],
    ) -> Result<PathBuf, AppError> {
        let safe = format!("{}_{}", doc_id, sanitize_filename(filename));
        let key = upload_key(project_id, &safe);
        self.store
            .put(&key, PutPayload::from(Bytes::copy_from_slice(content)))
            .await
            .map_err(map_put_err)?;
        Ok(PathBuf::from(key.to_string()))
    }

    /// Uploaded-document metadata for a project.
    ///
    /// `save_document` persists raw bytes only — extracted text (used for AI
    /// context) is a web-upload concern that isn't wired into the headless
    /// core path. So this returns no documents; `services::author::project_context`
    /// correctly reads that as "no uploaded-doc context" for the CLI flow.
    pub async fn load_documents(
        &self,
        _project_id: ProjectId,
    ) -> Result<Vec<crate::models::UploadedDocument>, AppError> {
        Ok(Vec::new())
    }

    pub async fn delete_document(
        &self,
        project_id: ProjectId,
        doc_id: DocumentId,
    ) -> Result<(), AppError> {
        let prefix = uploads_prefix(project_id);
        let mut stream = self.store.list(Some(&prefix));
        let needle = format!("{}_", doc_id);
        let mut to_delete = Vec::new();
        while let Some(meta) = stream.next().await {
            let meta = meta.map_err(|e| AppError::Internal(format!("list uploads: {e}")))?;
            let leaf = meta
                .location
                .to_string()
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string();
            if leaf.starts_with(&needle) {
                to_delete.push(meta.location);
            }
        }
        for key in to_delete {
            self.store
                .delete(&key)
                .await
                .map_err(|e| AppError::Internal(format!("delete upload: {e}")))?;
        }
        Ok(())
    }

    // ===== Responses =====

    pub async fn save_response(
        &self,
        project_id: ProjectId,
        response: &AssessmentResponse,
    ) -> Result<(), AppError> {
        let key = response_key(project_id, &response.respondent_id);
        let yaml = serde_yaml::to_string(response)
            .map_err(|e| AppError::Internal(format!("serialize response: {e}")))?;
        self.store
            .put(&key, PutPayload::from(Bytes::from(yaml)))
            .await
            .map_err(map_put_err)?;
        Ok(())
    }

    pub async fn load_response(
        &self,
        project_id: ProjectId,
        respondent_id: &RespondentId,
    ) -> Result<Option<AssessmentResponse>, AppError> {
        let key = response_key(project_id, respondent_id);
        match self.fetch_bytes(&key).await? {
            Some(bytes) => {
                let response: AssessmentResponse = serde_yaml::from_slice(&bytes)
                    .map_err(|e| AppError::Internal(format!("parse response: {e}")))?;
                Ok(Some(response))
            }
            None => Ok(None),
        }
    }

    // ===== Analysis caches =====

    pub async fn save_scorecard_cache(
        &self,
        project_id: ProjectId,
        json: &str,
    ) -> Result<(), AppError> {
        self.put_string(analysis_key(project_id, "scorecard.json"), json)
            .await
    }

    pub async fn save_gaps_cache(&self, project_id: ProjectId, json: &str) -> Result<(), AppError> {
        self.put_string(analysis_key(project_id, "gaps.json"), json)
            .await
    }

    pub async fn save_report_cache(
        &self,
        project_id: ProjectId,
        markdown: &str,
    ) -> Result<(), AppError> {
        self.put_string(analysis_key(project_id, "report.md"), markdown)
            .await
    }

    pub async fn load_analysis_cache(
        &self,
        project_id: ProjectId,
        filename: &str,
    ) -> Result<Option<String>, AppError> {
        let key = analysis_key(project_id, filename);
        match self.fetch_bytes(&key).await? {
            Some(bytes) => Ok(Some(String::from_utf8(bytes.to_vec()).map_err(|e| {
                AppError::Internal(format!("{} not utf-8: {e}", filename))
            })?)),
            None => Ok(None),
        }
    }

    // ===== Internal helpers =====

    /// Fetch bytes for a key, or `None` if the object doesn't exist. Any
    /// other error becomes `AppError::Internal`.
    async fn fetch_bytes(&self, key: &ObjectPath) -> Result<Option<Bytes>, AppError> {
        match self.store.get_opts(key, GetOptions::default()).await {
            Ok(result) => {
                let bytes = result
                    .bytes()
                    .await
                    .map_err(|e| AppError::Internal(format!("read {}: {e}", key)))?;
                Ok(Some(bytes))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(AppError::Internal(format!("get {}: {e}", key))),
        }
    }

    async fn put_string(&self, key: ObjectPath, body: &str) -> Result<(), AppError> {
        self.store
            .put(&key, PutPayload::from(Bytes::from(body.to_owned())))
            .await
            .map_err(map_put_err)?;
        Ok(())
    }
}

// ===== Key construction =====

fn project_prefix(project_id: ProjectId) -> ObjectPath {
    ObjectPath::from(format!("projects/{}", project_id))
}

fn project_key(project_id: ProjectId) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/project.json", project_id))
}

fn draft_key(project_id: ProjectId) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/draft.yaml", project_id))
}

fn chat_key(project_id: ProjectId) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/chat.json", project_id))
}

fn versions_prefix(project_id: ProjectId) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/versions", project_id))
}

fn version_yaml_key(project_id: ProjectId, name: &str) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/versions/{}.yaml", project_id, name))
}

fn version_meta_key(project_id: ProjectId, name: &str) -> ObjectPath {
    ObjectPath::from(format!(
        "projects/{}/versions/{}.meta.json",
        project_id, name
    ))
}

fn uploads_prefix(project_id: ProjectId) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/uploads", project_id))
}

fn upload_key(project_id: ProjectId, safe_name: &str) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/uploads/{}", project_id, safe_name))
}

fn response_key(project_id: ProjectId, respondent: &RespondentId) -> ObjectPath {
    ObjectPath::from(format!(
        "projects/{}/responses/{}.yaml",
        project_id, respondent
    ))
}

fn analysis_key(project_id: ProjectId, filename: &str) -> ObjectPath {
    ObjectPath::from(format!("projects/{}/analysis/{}", project_id, filename))
}

/// Permit only basic characters in publish names; reject path separators
/// and anything that would let a name escape the `versions/` prefix.
fn validate_version_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::BadRequest("version name cannot be empty".into()));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError::BadRequest(format!(
            "invalid version name '{}'",
            name
        )));
    }
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Translate `object_store::Error` from a put call into an `AppError`.
fn map_put_err(e: object_store::Error) -> AppError {
    AppError::Internal(format!("object_store put: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Project;
    use crate::storage_backend::{StorageBackend, build_store, filesystem};
    use tempfile::TempDir;

    async fn mk_storage_in_memory() -> StorageService {
        let store = build_store(&StorageBackend::InMemory).unwrap();
        StorageService::new(store)
    }

    async fn mk_storage_on_disk() -> (TempDir, StorageService) {
        let tmp = TempDir::new().unwrap();
        let store = build_store(&filesystem(tmp.path())).unwrap();
        (tmp, StorageService::new(store))
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello.txt"), "hello.txt");
        assert_eq!(sanitize_filename("hello world.txt"), "hello_world.txt");
        assert_eq!(
            sanitize_filename("../../../etc/passwd"),
            ".._.._.._etc_passwd"
        );
    }

    #[test]
    fn validate_version_name_rejects_path_traversal() {
        assert!(validate_version_name("v1").is_ok());
        assert!(validate_version_name("v1.0").is_ok());
        assert!(validate_version_name("initial-draft").is_ok());
        assert!(validate_version_name("").is_err());
        assert!(validate_version_name("../escape").is_err());
        assert!(validate_version_name("a/b").is_err());
        assert!(validate_version_name("ouch\\bad").is_err());
    }

    #[tokio::test]
    async fn save_and_load_project_round_trip() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("Test".to_string(), None);
        storage.save_project(&project).await.unwrap();
        let loaded = storage.load_project(project.id).await.unwrap();
        assert_eq!(loaded.id, project.id);
        assert_eq!(loaded.name, "Test");
    }

    #[tokio::test]
    async fn load_missing_project_is_not_found() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("x".to_string(), None);
        let err = storage.load_project(project.id).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_projects_returns_all_sorted_by_updated_at() {
        let storage = mk_storage_in_memory().await;
        let mut a = Project::new("a".to_string(), None);
        let mut b = Project::new("b".to_string(), None);
        a.updated_at = chrono::Utc::now() - chrono::Duration::seconds(60);
        b.updated_at = chrono::Utc::now();
        storage.save_project(&a).await.unwrap();
        storage.save_project(&b).await.unwrap();
        let list = storage.list_projects().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id, "newest first");
    }

    #[tokio::test]
    async fn delete_project_clears_all_keys_under_prefix() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("d".to_string(), None);
        storage.save_project(&project).await.unwrap();
        storage
            .save_assessment_yaml(project.id, "name: stub\n")
            .await
            .unwrap();
        storage.delete_project(project.id).await.unwrap();
        assert!(matches!(
            storage.load_project(project.id).await,
            Err(AppError::NotFound(_))
        ));
        assert!(
            storage
                .load_assessment_yaml(project.id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn conversation_round_trip_and_default_when_missing() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("c".to_string(), None);
        // Missing → returns an empty Conversation::new()
        let conv = storage.load_conversation(project.id).await.unwrap();
        assert_eq!(conv.project_id, project.id);
        assert!(conv.messages.is_empty());
        // Save + reload
        let mut conv = Conversation::new(project.id);
        conv.add_message(crate::models::ChatMessage::user("hi".to_string()));
        storage.save_conversation(&conv).await.unwrap();
        let loaded = storage.load_conversation(project.id).await.unwrap();
        assert_eq!(loaded.messages.len(), 1);
    }

    #[tokio::test]
    async fn assessment_yaml_round_trip() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("a".to_string(), None);
        assert!(
            storage
                .load_assessment_yaml(project.id)
                .await
                .unwrap()
                .is_none()
        );
        storage
            .save_assessment_yaml(project.id, "name: foo\n")
            .await
            .unwrap();
        let loaded = storage
            .load_assessment_yaml(project.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded, "name: foo\n");
    }

    #[tokio::test]
    async fn publish_version_and_load_at() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("p".to_string(), None);
        storage
            .save_assessment_yaml(project.id, "name: v1-content\n")
            .await
            .unwrap();
        storage
            .publish_version(project.id, "v1", Some("first cut"))
            .await
            .unwrap();
        let restored = storage
            .load_assessment_yaml_at(project.id, "v1")
            .await
            .unwrap();
        assert_eq!(restored, "name: v1-content\n");
        let versions = storage.list_versions(project.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].name, "v1");
        assert_eq!(versions[0].notes.as_deref(), Some("first cut"));
    }

    #[tokio::test]
    async fn publish_duplicate_name_fails() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("p".to_string(), None);
        storage
            .save_assessment_yaml(project.id, "name: x\n")
            .await
            .unwrap();
        storage
            .publish_version(project.id, "v1", None)
            .await
            .unwrap();
        let err = storage
            .publish_version(project.id, "v1", None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn latest_version_picks_newest_published_at() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("p".to_string(), None);
        storage
            .save_assessment_yaml(project.id, "name: a\n")
            .await
            .unwrap();
        storage
            .publish_version(project.id, "v1", None)
            .await
            .unwrap();
        // Force a millisecond gap so the timestamps differ deterministically.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        storage
            .save_assessment_yaml(project.id, "name: b\n")
            .await
            .unwrap();
        storage
            .publish_version(project.id, "v2", None)
            .await
            .unwrap();
        let latest = storage.latest_version(project.id).await.unwrap().unwrap();
        assert_eq!(latest.name, "v2");
    }

    #[tokio::test]
    async fn reset_draft_overwrites_with_published_content() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("p".to_string(), None);
        storage
            .save_assessment_yaml(project.id, "name: original\n")
            .await
            .unwrap();
        storage
            .publish_version(project.id, "v1", None)
            .await
            .unwrap();
        storage
            .save_assessment_yaml(project.id, "name: drifted\n")
            .await
            .unwrap();
        storage
            .reset_draft_from_version(project.id, "v1")
            .await
            .unwrap();
        assert_eq!(
            storage
                .load_assessment_yaml(project.id)
                .await
                .unwrap()
                .unwrap(),
            "name: original\n"
        );
    }

    #[tokio::test]
    async fn etag_conditional_write_detects_conflict() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("p".to_string(), None);
        storage
            .save_assessment_yaml(project.id, "name: start\n")
            .await
            .unwrap();
        let (_yaml, version) = storage
            .load_assessment_yaml_versioned(project.id)
            .await
            .unwrap()
            .unwrap();

        // A second writer slips in.
        storage
            .save_assessment_yaml(project.id, "name: interloper\n")
            .await
            .unwrap();

        // First writer tries to commit with the stale version — should conflict.
        let result = storage
            .save_assessment_yaml_if_match(project.id, "name: ours\n", Some(version))
            .await;
        assert!(matches!(result, Err(AppError::Conflict(_))));
        assert_eq!(
            storage
                .load_assessment_yaml(project.id)
                .await
                .unwrap()
                .unwrap(),
            "name: interloper\n",
            "the interloper's write should still be the latest"
        );
    }

    #[tokio::test]
    async fn response_round_trip() {
        use crate::models::AssessmentResponse;
        use crate::models::ids::AssessmentId;
        let storage = mk_storage_in_memory().await;
        let project = Project::new("r".to_string(), None);
        let respondent = crate::services::responses::primary_respondent_id();
        let response = AssessmentResponse::new(AssessmentId::new(), respondent, "v1".to_string());
        storage.save_response(project.id, &response).await.unwrap();
        let loaded = storage
            .load_response(project.id, &respondent)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.version, "v1");
        assert_eq!(loaded.respondent_id, respondent);
    }

    #[tokio::test]
    async fn analysis_cache_round_trip() {
        let storage = mk_storage_in_memory().await;
        let project = Project::new("ac".to_string(), None);
        storage
            .save_scorecard_cache(project.id, r#"{"answered":1}"#)
            .await
            .unwrap();
        let got = storage
            .load_analysis_cache(project.id, "scorecard.json")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, r#"{"answered":1}"#);
        assert!(
            storage
                .load_analysis_cache(project.id, "missing.json")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn filesystem_backend_persists_real_files() {
        let (tmp, storage) = mk_storage_on_disk().await;
        let project = Project::new("fs".to_string(), None);
        storage.save_project(&project).await.unwrap();
        storage
            .save_assessment_yaml(project.id, "name: fs\n")
            .await
            .unwrap();
        let on_disk = tmp
            .path()
            .join(format!("projects/{}/draft.yaml", project.id));
        assert!(on_disk.exists());
    }
}

//! The one command surface, defined once.
//!
//! Each command's parameters are a struct deriving both `clap::Args` (the
//! terminal surface) and `schemars::JsonSchema` + `serde::Deserialize` (the
//! MCP tool surface) — the `///` doc comments are simultaneously `--help`
//! text and tool-parameter descriptions. Core functions take these structs
//! and return structured reports; the clap dispatch prints them, the MCP
//! wrappers ([`crate::mcp`]) return them verbatim. One definition, three
//! callers: human, automation, agent.

use std::path::PathBuf;

use anyhow::Context;
use serde_json::{Value, json};

use amaker_core::models::ProjectId;
use amaker_core::services::{DataFormat, ExportService, ResponseService};

// ── Parameters ────────────────────────────────────────────────────────────────

/// Parameters for `validate`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ValidateParams {
    /// Assessment file; format inferred from extension (.yaml/.yml/.json/.toml)
    pub file: PathBuf,
}

/// Parameters for `import`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ImportParams {
    /// Assessment file; format inferred from extension (.yaml/.yml/.json/.toml)
    pub file: PathBuf,
    /// Project name (default: the assessment's name)
    #[arg(long)]
    pub name: Option<String>,
    /// Published version name
    #[arg(long, default_value = "v1")]
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_version() -> String {
    "v1".to_string()
}

/// Parameters for `export`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportParams {
    /// Project ID (UUID) under DATA_DIR/projects/
    pub project_id: ProjectId,
    /// Output format: yaml, json, or toml
    #[arg(long, default_value = "yaml", value_parser = crate::parse_data_format)]
    #[serde(default = "default_format")]
    pub format: DataFormat,
    /// Output file (content returned/printed when omitted)
    #[arg(long)]
    pub out: Option<PathBuf>,
}

fn default_format() -> DataFormat {
    DataFormat::Yaml
}

/// Parameters for `publish`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PublishParams {
    /// Project ID (UUID) under DATA_DIR/projects/
    pub project_id: ProjectId,
    /// Target space name (overrides KB_WIKI)
    #[arg(long)]
    pub wiki: Option<String>,
}

/// Parameters for `schema`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SchemaParams {
    /// Output file (schema returned/printed when omitted)
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// Parameters for `status`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StatusParams {
    /// Project ID (UUID) under DATA_DIR/projects/
    pub project_id: ProjectId,
}

// ── Core functions ────────────────────────────────────────────────────────────
//
// Every function returns a `serde_json::Value` report. Where a shape is a
// documented contract (`validate`'s summary joins against `adroit import
// -o json`; `import`'s report is what scripts read the project_id from),
// the keys here are that contract — change them in lockstep with the
// consumers or not at all.

/// `validate`: schema + degeneracy gates, returning the machine summary.
pub fn validate_core(p: &ValidateParams) -> anyhow::Result<Value> {
    let assessment = crate::validate_cmd(&p.file)?;
    Ok(serde_json::from_str(&crate::validate_summary_json(
        &assessment,
    )?)?)
}

/// `import`: validate, create a project, store the assessment, publish a
/// version — the headless authoring door.
pub async fn import_core(data_dir: &std::path::Path, p: &ImportParams) -> anyhow::Result<Value> {
    let assessment = crate::validate_cmd(&p.file)?;
    let storage = crate::fs_storage(data_dir);
    storage.init().await?;
    let project = amaker_core::models::Project::new(
        p.name.clone().unwrap_or_else(|| assessment.name.clone()),
        None,
    );
    storage.save_project(&project).await?;
    // Store as YAML regardless of input format — the storage layer's
    // one representation (same normalization the web app performs).
    let yaml = ExportService::to_data(&assessment, DataFormat::Yaml)?;
    storage.save_assessment_yaml(project.id, &yaml).await?;
    storage
        .publish_version(project.id, &p.version, None)
        .await?;
    Ok(json!({
        "project_id": project.id,
        "name": project.name,
        "version": p.version,
        "domains": assessment.domain_count(),
        "practices": assessment.practice_count(),
        "questions": assessment.question_count(),
    }))
}

/// `export`: serialize a stored project's assessment via the shared export
/// path; writes `out` when given, else carries the content in the report.
pub async fn export_core(data_dir: &std::path::Path, p: &ExportParams) -> anyhow::Result<Value> {
    let storage = crate::fs_storage(data_dir);
    let (project, content) = crate::export_project(&storage, p.project_id, p.format).await?;
    if let Some(path) = &p.out {
        std::fs::write(path, &content)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(json!({
        "project_id": p.project_id,
        "name": project.name,
        "format": p.format,
        "written": p.out,
        "content": if p.out.is_none() { Value::String(content) } else { Value::Null },
    }))
}

/// `publish`: assessment + analysis to the KB as amaker-owned pages.
pub async fn publish_core(data_dir: &std::path::Path, p: &PublishParams) -> anyhow::Result<Value> {
    let storage = crate::fs_storage(data_dir);
    storage.init().await?;
    let report = crate::publish::publish_cmd(storage, p.project_id, p.wiki.clone()).await?;
    Ok(serde_json::to_value(report)?)
}

/// `schema`: the assessment JSON Schema; writes `out` when given.
pub fn schema_core(p: &SchemaParams) -> anyhow::Result<Value> {
    let schema = ExportService::json_schema();
    if let Some(path) = &p.out {
        let mut s = serde_json::to_string_pretty(&schema)?;
        s.push('\n');
        std::fs::write(path, s).with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(schema)
}

/// `list`: every project with its latest published version — how a caller
/// that only knows the data dir finds its work.
pub async fn list_core(data_dir: &std::path::Path) -> anyhow::Result<Value> {
    let storage = crate::fs_storage(data_dir);
    storage.init().await?;
    let mut entries = Vec::new();
    for project in storage.list_projects().await? {
        let latest = storage.latest_version(project.id).await?;
        entries.push(json!({
            "project_id": project.id,
            "name": project.name,
            "description": project.description,
            "updated_at": project.updated_at,
            "latest_version": latest,
        }));
    }
    Ok(json!({ "projects": entries }))
}

/// `status`: one project's published versions and the primary respondent's
/// progress — how a caller knows the human has answered before publishing.
pub async fn status_core(data_dir: &std::path::Path, p: &StatusParams) -> anyhow::Result<Value> {
    let storage = crate::fs_storage(data_dir);
    storage.init().await?;
    let project = storage.load_project(p.project_id).await?;
    let versions = storage.list_versions(p.project_id).await?;
    let response = match ResponseService::new(storage.clone())
        .load_primary(p.project_id)
        .await?
    {
        None => Value::Null,
        Some(r) => {
            let yaml = storage
                .load_assessment_yaml_at(p.project_id, &r.version)
                .await?;
            let assessment = ExportService::validate_and_parse(&yaml, DataFormat::Yaml)?;
            let answered = r.answers.len();
            let total = assessment.question_count();
            json!({
                "version": r.version,
                "answered": answered,
                "total": total,
                "submitted_at": r.submitted_at,
                "complete": r.submitted_at.is_some() || answered >= total,
            })
        }
    };
    Ok(json!({
        "project_id": p.project_id,
        "name": project.name,
        "versions": versions,
        "response": response,
    }))
}

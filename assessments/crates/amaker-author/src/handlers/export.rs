//! Assessment export handlers.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;

use crate::state::AppState;
use amaker_core::AppError;
use amaker_core::models::ProjectId;
use amaker_core::services::{DataFormat, ExportService, YamlService};

/// Query parameters for export.
#[derive(Deserialize)]
pub struct ExportQuery {
    /// Export format: json, yaml, or toml (default: yaml)
    format: Option<String>,
}

/// Download assessment in the specified format.
pub async fn download_export(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, AppError> {
    let project = state.storage.load_project(project_id).await?;

    // Parse format from query param (default to YAML for backward compatibility)
    let format = query
        .format
        .as_deref()
        .and_then(DataFormat::from_str)
        .unwrap_or(DataFormat::Yaml);

    // Load and parse assessment
    let yaml = state
        .storage
        .load_assessment_yaml(project_id)
        .await?
        .ok_or_else(|| AppError::NotFound("No assessment generated yet".to_string()))?;

    let assessment = YamlService::parse_assessment(&yaml)?;
    let content = ExportService::to_data(&assessment, format)?;

    // Create filename from project name
    let base_name: String = project
        .name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect();
    let filename = format!("{}.{}", base_name, format.extension());

    let response = (
        [
            (header::CONTENT_TYPE, format.content_type()),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        content,
    );

    Ok(response.into_response())
}

/// Get the JSON Schema for Assessment.
pub async fn get_schema() -> impl IntoResponse {
    Json(ExportService::json_schema())
}

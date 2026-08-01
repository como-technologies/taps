//! File upload handlers.

use std::sync::Arc;

use crate::state::AppState;
use amaker_core::AppError;
use amaker_core::models::{DocumentId, ProjectId, UploadedDocument};
use askama::Template;
use axum::{
    extract::{Multipart, Path, State},
    response::Html,
};

/// Document badge template.
#[derive(Template)]
#[template(path = "partials/upload/document.html")]
pub struct DocumentTemplate {
    pub document: UploadedDocument,
    pub project_id: ProjectId,
}

/// Upload a document.
pub async fn upload_document(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
    mut multipart: Multipart,
) -> Result<Html<String>, AppError> {
    // Get the first field from the multipart upload
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest("No file uploaded".to_string()))?;

    let filename = field
        .file_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let content_type = field
        .content_type()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let data = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let document = UploadedDocument::new(filename.clone(), content_type, data.len());

    // Save file
    state
        .storage
        .save_document(project_id, document.id, &filename, &data)
        .await?;

    // Update project with document ID
    let mut project = state.storage.load_project(project_id).await?;
    project.document_ids.push(document.id);
    project.touch();
    state.storage.save_project(&project).await?;

    let template = DocumentTemplate {
        document,
        project_id,
    };

    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// Delete a document.
pub async fn delete_document(
    State(state): State<Arc<AppState>>,
    Path((project_id, doc_id)): Path<(ProjectId, DocumentId)>,
) -> Result<Html<String>, AppError> {
    state.storage.delete_document(project_id, doc_id).await?;

    // Update project
    let mut project = state.storage.load_project(project_id).await?;
    project.document_ids.retain(|id| *id != doc_id);
    project.touch();
    state.storage.save_project(&project).await?;

    Ok(Html(String::new()))
}

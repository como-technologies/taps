//! Assist handler for generating simulated SME responses.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::services::assessment_structure_summary;
use crate::state::AppState;
use amaker_core::AppError;
use amaker_core::models::{AuthoringSubstate, ChatRole, ProjectId};
use amaker_core::services::YamlService;

/// System prompt for user simulation.
const USER_SIMULATION_PROMPT: &str = include_str!("../prompts/user_simulation.md");

/// Query parameters for assist endpoint.
#[derive(Debug, Deserialize)]
pub struct AssistQuery {
    /// Optional model override (e.g., "claude-sonnet-4-5")
    pub model: Option<String>,
}

/// Generate an assist message simulating an SME response.
pub async fn generate_assist(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
    Query(query): Query<AssistQuery>,
) -> Result<Response, AppError> {
    // Load project and conversation
    let project = state.storage.load_project(project_id).await?;
    let conversation = state.storage.load_conversation(project_id).await?;

    // Build context for Claude
    let mut context = format!(
        "## Current Focus\n{}\n\n",
        focus_description(project.focus_substate)
    );

    // Add conversation history summary
    context.push_str("## Conversation History\n");
    if conversation.messages.is_empty() {
        context.push_str("(No messages yet - this is the start of the conversation)\n");
    } else {
        for msg in &conversation.messages {
            let role = match msg.role {
                ChatRole::User => "SME",
                ChatRole::Assistant => "AI",
                ChatRole::System => continue, // Skip system messages
            };
            // Truncate long messages (find valid UTF-8 boundary)
            let content = if msg.content.len() > 500 {
                let mut end = 500;
                while !msg.content.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &msg.content[..end])
            } else {
                msg.content.clone()
            };
            context.push_str(&format!("\n**{}**: {}\n", role, content));
        }
    }

    // Add assessment summary once the draft is far enough along to talk
    // about questions. Scoping/Structuring stays at the conversation level.
    let show_assessment = matches!(
        project.focus_substate,
        AuthoringSubstate::Questions | AuthoringSubstate::Refining
    );
    if show_assessment
        && let Ok(Some(yaml)) = state.storage.load_assessment_yaml(project_id).await
        && let Ok(assessment) = YamlService::parse_assessment(&yaml)
    {
        let summary = assessment_structure_summary(&assessment, &project);
        context.push_str(&format!("\n## Current Assessment\n{}\n", summary));
    }

    // Build the user message for Claude
    let user_message = format!(
        "{}\n\n---\n\nBased on this context, generate a realistic SME message:\n\n{}",
        USER_SIMULATION_PROMPT, context
    );

    // Call the AI service to generate the simulated response.
    let output = state
        .ai
        .completion_only(
            "You generate realistic SME messages for testing. Output ONLY the message text.",
            user_message,
            500,
            query.model.as_deref().filter(|s| !s.is_empty()),
        )
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    Ok((headers, output).into_response())
}

/// Describe the current authoring substate for the simulator's context.
fn focus_description(substate: AuthoringSubstate) -> &'static str {
    match substate {
        AuthoringSubstate::Scoping => {
            "Authoring / Scoping - The SME is describing what they want to assess, the target audience, and goals."
        }
        AuthoringSubstate::Structuring => {
            "Authoring / Structuring - The AI is proposing assessment structure (domains and practices). SME reviews and approves."
        }
        AuthoringSubstate::Questions => {
            "Authoring / Questions - The AI presents each practice one at a time for question generation. SME provides guidance or approves."
        }
        AuthoringSubstate::Refining => {
            "Authoring / Refining - The assessment is in surgical-edit mode. SME can request rewording, additions, or removals."
        }
    }
}

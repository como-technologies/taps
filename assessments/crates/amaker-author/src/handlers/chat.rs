//! Chat handlers for AI conversation.

use std::sync::Arc;

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use rig::completion::Message;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::services::{
    AddDomainTool, AddPracticeTool, AddQuestionTool, AskClarifyingQuestionTool, DeleteDomainTool,
    DeletePracticeTool, DeleteQuestionTool, EditDomainTool, EditPracticeTool, EditQuestionTool,
    GenerateQuestionsTool, GenerateStructureTool, MovePracticeTool, OfferSuggestedRepliesTool,
    PublishAssessmentTool, RegenerateQuestionTool, ReorderDomainsTool, ReorderPracticesTool,
    RequestContext, ResetDraftFromVersionTool, SetQuestionBudgetTool, SharedContext,
    SwitchFocusTool, TailorVocabularyTool, assessment_structure_summary, linkify_entities,
};
use crate::state::AppState;
use amaker_core::AppError;
use amaker_core::models::{
    AuthoringSubstate, ChatMessage, ChatRole, ClarifyingQuestion, ClarifyingQuestionId, ProjectId,
};
use amaker_core::services::YamlService;

/// Chat message list template.
#[derive(Template)]
#[template(path = "partials/chat/message_list.html")]
pub struct MessageListTemplate {
    pub project_id: ProjectId,
    pub messages: Vec<ChatMessage>,
}

/// Single chat message template.
#[derive(Template)]
#[template(path = "partials/chat/message.html")]
pub struct MessageTemplate {
    pub project_id: ProjectId,
    pub message: ChatMessage,
}

/// Clarifying question template.
#[derive(Template)]
#[template(path = "partials/chat/clarifying_question.html")]
pub struct ClarifyingQuestionTemplate {
    pub question: ClarifyingQuestion,
    pub project_id: ProjectId,
    pub timestamp: DateTime<Utc>,
    /// Any preamble text from the model (rendered above the question).
    pub preamble_html: String,
}

/// Form data for sending a message.
#[derive(Deserialize)]
pub struct SendMessageForm {
    pub content: String,
    /// Optional model override (e.g., "claude-sonnet-4-5")
    pub model: Option<String>,
}

/// Strip YAML code blocks from text and replace with a placeholder.
/// Prevents YAML duplication across assessment.yaml and chat.json.
fn strip_yaml_blocks(text: &str) -> String {
    let yaml_pattern = regex::Regex::new(r"```ya?ml\s*\n[\s\S]*?\n```").unwrap();
    yaml_pattern
        .replace_all(text, "[Assessment YAML - see preview panel]")
        .to_string()
}

/// True when `text` contains a phrase like "N questions generated" or
/// "generated N questions" — the canonical hallucinated-success patterns
/// the orchestrator emits when it forgets to actually call
/// `generate_questions`. Combined with `ctx.questions_generated.is_none()`
/// this is the diagnostic signal for a tool-claim hallucination.
fn claims_questions_generated(text: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?i)(\b\d+\s+questions?\s+(have\s+been\s+)?generated\b|\bgenerated\s+\d+\s+questions?\b)",
        )
        .expect("static regex compiles")
    })
    .is_match(text)
}

/// Get chat history for a project.
pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
) -> Result<Html<String>, AppError> {
    let conversation = state.storage.load_conversation(project_id).await?;
    let template = MessageListTemplate {
        project_id,
        messages: conversation.messages,
    };
    Ok(Html(
        template
            .render()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    ))
}

/// Convert saved conversation messages into rig `Message`s for history.
fn to_rig_history(conversation: &amaker_core::models::Conversation) -> Vec<Message> {
    conversation
        .messages
        .iter()
        .filter(|m| !m.content.is_empty())
        .filter_map(|m| match m.role {
            ChatRole::User => Some(Message::user(m.content.clone())),
            ChatRole::Assistant => Some(Message::assistant(m.content.clone())),
            ChatRole::System => None,
        })
        .collect()
}

/// Build the substate-specific tool set bound to the shared context.
///
/// `switch_focus` and `offer_suggested_replies` are always available;
/// surgical CRUD lights up from Structuring onward; generation tools and
/// the question-budget tool are substate-gated to match the workflow.
fn tools_for_substate(
    substate: AuthoringSubstate,
    ctx: &SharedContext,
) -> Vec<Box<dyn rig::tool::ToolDyn>> {
    let mut tools: Vec<Box<dyn rig::tool::ToolDyn>> = Vec::new();
    tools.push(Box::new(SwitchFocusTool { ctx: ctx.clone() }));
    match substate {
        AuthoringSubstate::Scoping => {
            tools.push(Box::new(AskClarifyingQuestionTool { ctx: ctx.clone() }));
            tools.push(Box::new(SetQuestionBudgetTool { ctx: ctx.clone() }));
        }
        AuthoringSubstate::Structuring => {
            tools.push(Box::new(GenerateStructureTool { ctx: ctx.clone() }));
            tools.push(Box::new(SetQuestionBudgetTool { ctx: ctx.clone() }));
            tools.push(Box::new(TailorVocabularyTool { ctx: ctx.clone() }));
            push_draft_tools(&mut tools, ctx);
        }
        AuthoringSubstate::Questions => {
            tools.push(Box::new(GenerateQuestionsTool { ctx: ctx.clone() }));
            tools.push(Box::new(TailorVocabularyTool { ctx: ctx.clone() }));
            push_draft_tools(&mut tools, ctx);
        }
        AuthoringSubstate::Refining => {
            tools.push(Box::new(TailorVocabularyTool { ctx: ctx.clone() }));
            push_draft_tools(&mut tools, ctx);
        }
    }
    tools.push(Box::new(OfferSuggestedRepliesTool { ctx: ctx.clone() }));
    tools
}

/// Append publish + reset + 13 surgical CRUD tools — the set every Authoring
/// substate from Structuring onward shares.
fn push_draft_tools(tools: &mut Vec<Box<dyn rig::tool::ToolDyn>>, ctx: &SharedContext) {
    tools.push(Box::new(PublishAssessmentTool { ctx: ctx.clone() }));
    tools.push(Box::new(ResetDraftFromVersionTool { ctx: ctx.clone() }));
    tools.push(Box::new(AddDomainTool { ctx: ctx.clone() }));
    tools.push(Box::new(EditDomainTool { ctx: ctx.clone() }));
    tools.push(Box::new(DeleteDomainTool { ctx: ctx.clone() }));
    tools.push(Box::new(ReorderDomainsTool { ctx: ctx.clone() }));
    tools.push(Box::new(AddPracticeTool { ctx: ctx.clone() }));
    tools.push(Box::new(EditPracticeTool { ctx: ctx.clone() }));
    tools.push(Box::new(DeletePracticeTool { ctx: ctx.clone() }));
    tools.push(Box::new(MovePracticeTool { ctx: ctx.clone() }));
    tools.push(Box::new(ReorderPracticesTool { ctx: ctx.clone() }));
    tools.push(Box::new(AddQuestionTool { ctx: ctx.clone() }));
    tools.push(Box::new(EditQuestionTool { ctx: ctx.clone() }));
    tools.push(Box::new(DeleteQuestionTool { ctx: ctx.clone() }));
    tools.push(Box::new(RegenerateQuestionTool { ctx: ctx.clone() }));
}

/// Send a message and get AI response.
pub async fn send_message(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
    Form(form): Form<SendMessageForm>,
) -> Result<Response, AppError> {
    tracing::info!("=== CHAT REQUEST START ===");
    tracing::info!("Project ID: {}", project_id);

    // Load conversation and project
    let mut conversation = state.storage.load_conversation(project_id).await?;
    let mut project = state.storage.load_project(project_id).await?;

    // Free-text flush: any pending clarifying questions queued by a prior
    // turn's model are stale once the user types into the free-text area
    // instead of answering/skipping a card. The `respond_to_question` and
    // `skip_question` paths only hand off to `send_message` when the queue is
    // already empty, so a non-empty queue here always means "user ignored the
    // card." Clear it before the new turn runs.
    let flushed = project.pending_clarifying_questions.len();
    if flushed > 0 {
        project.pending_clarifying_questions.clear();
        tracing::info!(
            "Flushed {} pending clarifying question(s) on free-text send",
            flushed
        );
    }

    tracing::info!("Project substate: {:?}", project.focus_substate);
    tracing::debug!(
        "Conversation history: {} messages",
        conversation.messages.len()
    );

    // Append user message to conversation (saved at end).
    let user_msg = ChatMessage::user(form.content.clone());
    conversation.add_message(user_msg.clone());

    // Build history for the agent (everything BEFORE the new user message).
    let history = {
        let mut snapshot = conversation.clone();
        snapshot.messages.pop();
        to_rig_history(&snapshot)
    };

    // Pull in the assessment summary once the SME is past Structuring — the
    // model needs the question tree once it's drafting/refining it.
    let want_summary = matches!(
        project.focus_substate,
        AuthoringSubstate::Questions | AuthoringSubstate::Refining
    );
    let assessment_summary = if want_summary {
        state
            .storage
            .load_assessment_yaml(project_id)
            .await
            .ok()
            .flatten()
            .and_then(|yaml| YamlService::parse_assessment(&yaml).ok())
            .map(|a| assessment_structure_summary(&a, &project))
    } else {
        None
    };

    let substate = project.focus_substate;
    let system_prompt = state
        .ai
        .system_prompt_for_substate(substate, assessment_summary.as_deref());

    // Wire the per-request context shared across tool calls.
    let ctx_inner = RequestContext::new(
        project,
        conversation.clone(),
        state.storage.clone(),
        state.ai.clone(),
    );
    let ctx: SharedContext = Arc::new(Mutex::new(ctx_inner));

    let model_override = form.model.as_deref().filter(|s| !s.is_empty());
    tracing::info!(
        "Running agent (substate={:?}, model={:?})",
        substate,
        model_override
    );

    let tools = tools_for_substate(substate, &ctx);
    let output = state
        .ai
        .run_agent(
            &system_prompt,
            history,
            form.content.clone(),
            model_override,
            tools,
        )
        .await?;

    // Drain side effects from the shared context. Pops the first queued
    // clarifying question (if any) into the active slot for this turn; the
    // remainder stays in `project.pending_clarifying_questions` and drains via
    // `respond_to_question` / `skip_question` without further LLM turns.
    let (
        project,
        focus_changed,
        assessment_updated,
        clarifying_question,
        suggestions,
        questions_generated,
    ) = {
        let mut ctx = ctx.lock().await;
        let active = if ctx.project.pending_clarifying_questions.is_empty() {
            None
        } else {
            Some(ctx.project.pending_clarifying_questions.remove(0))
        };
        (
            ctx.project.clone(),
            ctx.focus_changed,
            ctx.assessment_generated,
            active,
            ctx.suggestions.clone(),
            ctx.questions_generated.clone(),
        )
    };

    tracing::info!(
        "Agent done: output={} chars, focus_changed={}, assessment_updated={}, has_question={}",
        output.len(),
        focus_changed,
        assessment_updated,
        clarifying_question.is_some()
    );

    // Hallucination canary: the model sometimes writes a "N questions
    // generated" success summary in the same turn as the tool call (or
    // without calling the tool at all). When that happens, ctx.questions_generated
    // stays None but the output text still claims success — we ship a lie
    // to the user. The system_base + phase_questions prompts forbid this,
    // but model behavior drifts; this WARN surfaces the regression in the
    // app log instead of leaving it to chat.json archeology.
    if questions_generated.is_none() && claims_questions_generated(&output) {
        tracing::warn!(
            project_id = %project_id,
            output = %output,
            "Suspected tool-claim hallucination: output text claims questions were generated, but generate_questions was not called this turn"
        );
    }

    // Persist any project changes the tools made (tools already save, but be defensive).
    state.storage.save_project(&project).await?;

    // Strip YAML before saving to the conversation log.
    let stripped_raw = strip_yaml_blocks(&output);

    // Linkify domain/practice mentions so clicking them scrolls the preview
    // pane to the right entity. The assessment may have been updated by a
    // tool this turn, so re-read it (not just the pre-turn summary).
    let stripped = match state.storage.load_assessment_yaml(project_id).await {
        Ok(Some(yaml)) => match YamlService::parse_assessment(&yaml) {
            Ok(a) => linkify_entities(&stripped_raw, &a),
            Err(_) => stripped_raw,
        },
        _ => stripped_raw,
    };

    // Build the assistant message to append to conversation history.
    let ai_msg = if let Some(ref question) = clarifying_question {
        let question_text = format!(
            "**Question:** {}\n\n**Options:**\n{}",
            question.question,
            question
                .options
                .iter()
                .map(|o| match &o.description {
                    Some(desc) => format!("- {} - {}", o.label, desc),
                    None => format!("- {}", o.label),
                })
                .collect::<Vec<_>>()
                .join("\n")
        );
        let content = if stripped.is_empty() {
            question_text
        } else {
            format!("{}\n\n{}", stripped, question_text)
        };
        // Clarifying question UI already includes answer cards; suggestions are redundant.
        ChatMessage::with_question(content, question.clone())
    } else {
        let mut msg = ChatMessage::assistant(stripped.clone()).with_suggestions(suggestions);
        if let Some(info) = questions_generated {
            msg = msg.with_questions_generated(info);
        }
        msg
    };
    conversation.add_message(ai_msg.clone());
    state.storage.save_conversation(&conversation).await?;

    // HTMX triggers
    let mut headers = HeaderMap::new();
    let mut triggers = Vec::new();
    if focus_changed {
        triggers.push("focusChanged");
    }
    if assessment_updated {
        triggers.push("assessmentUpdated");
    }
    if !triggers.is_empty() {
        headers.insert(
            "HX-Trigger",
            HeaderValue::from_str(&triggers.join(", ")).unwrap_or(HeaderValue::from_static("")),
        );
    }

    // Render
    let ai_html = if let Some(question) = clarifying_question {
        // Render the stripped text as markdown for the card preamble.
        let preamble_html = if stripped.is_empty() {
            String::new()
        } else {
            amaker_core::services::markdown_to_html(&stripped)
        };
        let tpl = ClarifyingQuestionTemplate {
            question,
            project_id,
            timestamp: ai_msg.timestamp,
            preamble_html,
        };
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        let tpl = MessageTemplate {
            project_id,
            message: ai_msg,
        };
        tpl.render()
            .map_err(|e| AppError::Internal(e.to_string()))?
    };

    tracing::info!("=== CHAT REQUEST END ===");
    Ok((headers, Html(ai_html)).into_response())
}

/// Form data for responding to a clarifying question.
///
/// Parsed with `axum_extra::extract::Form` (serde_html_form) so repeated
/// `selections=A&selections=B` keys from multi-select checkboxes hydrate
/// into the Vec. Standard `axum::Form` (serde_urlencoded) would silently
/// keep only the last value.
#[derive(Debug, Deserialize)]
pub struct QuestionResponseForm {
    pub question_id: ClarifyingQuestionId,
    #[serde(default)]
    pub selections: Vec<String>,
    #[serde(default)]
    pub custom_text: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// Handle user response to a clarifying question.
pub async fn respond_to_question(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<QuestionResponseForm>,
) -> Result<Response, AppError> {
    tracing::info!("=== QUESTION RESPONSE START ===");
    tracing::info!("Project ID: {}", project_id);
    tracing::info!("Question ID: {}", form.question_id);
    tracing::debug!(
        "Selections ({} items): {:?}",
        form.selections.len(),
        form.selections
    );
    tracing::debug!("Custom text: {:?}", form.custom_text);

    simplify_answered_message(&state, project_id, form.question_id).await?;
    let response_content = format_question_response(&form);
    advance_or_forward(state, project_id, response_content, form.model).await
}

/// Form data for skipping a clarifying question.
#[derive(Debug, Deserialize)]
pub struct SkipQuestionForm {
    pub question_id: ClarifyingQuestionId,
    #[serde(default)]
    pub model: Option<String>,
}

/// Handle user skipping a clarifying question. Dismisses the current card and
/// advances to the next queued question (if any), or forwards a `(skipped)`
/// user message to the LLM path when the queue is empty.
pub async fn skip_question(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<ProjectId>,
    Form(form): Form<SkipQuestionForm>,
) -> Result<Response, AppError> {
    tracing::info!("=== QUESTION SKIP START ===");
    tracing::info!("Project ID: {}", project_id);
    tracing::info!("Question ID: {}", form.question_id);

    simplify_answered_message(&state, project_id, form.question_id).await?;
    advance_or_forward(state, project_id, "(skipped)".to_string(), form.model).await
}

/// Strip the rendered `**Options:**` block from the prior assistant message
/// and clear its `clarifying_question` so history renders it readonly.
async fn simplify_answered_message(
    state: &Arc<AppState>,
    project_id: ProjectId,
    question_id: ClarifyingQuestionId,
) -> Result<(), AppError> {
    let mut conversation = state.storage.load_conversation(project_id).await?;
    for msg in conversation.messages.iter_mut() {
        if let Some(ref q) = msg.clarifying_question
            && q.id == question_id
        {
            if let Some(idx) = msg.content.find("**Options:**") {
                msg.content.truncate(idx);
                msg.content = msg.content.trim_end().to_string();
            }
            msg.clarifying_question = None;
            break;
        }
    }
    state.storage.save_conversation(&conversation).await?;
    Ok(())
}

/// If the project's pending-clarifying-question queue has entries, pop the
/// next one and synthesize an assistant turn without consulting the LLM.
/// Otherwise forward `content` to `send_message` as a normal free-text turn
/// so the LLM produces the next response.
async fn advance_or_forward(
    state: Arc<AppState>,
    project_id: ProjectId,
    content: String,
    model: Option<String>,
) -> Result<Response, AppError> {
    let mut project = state.storage.load_project(project_id).await?;

    if project.pending_clarifying_questions.is_empty() {
        // Queue empty — let the LLM path handle it.
        let message_form = SendMessageForm { content, model };
        return send_message(State(state), Path(project_id), Form(message_form)).await;
    }

    // Queue non-empty — pop and synthesize the next card locally.
    let next = project.pending_clarifying_questions.remove(0);
    project.touch();

    let mut conversation = state.storage.load_conversation(project_id).await?;
    conversation.add_message(ChatMessage::user(content));
    let assistant_msg = ChatMessage::with_question(String::new(), next.clone());
    conversation.add_message(assistant_msg.clone());
    state.storage.save_conversation(&conversation).await?;
    state.storage.save_project(&project).await?;

    let tpl = ClarifyingQuestionTemplate {
        question: next,
        project_id,
        timestamp: assistant_msg.timestamp,
        preamble_html: String::new(),
    };
    let html = tpl
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(html).into_response())
}

/// Combine every selected option with the custom text (if present) into a
/// single natural-language answer, joined by ", ".
fn format_question_response(form: &QuestionResponseForm) -> String {
    let mut parts: Vec<String> = form
        .selections
        .iter()
        .filter(|s| !s.is_empty() && s.as_str() != "__custom__")
        .cloned()
        .collect();

    if form.selections.iter().any(|s| s == "__custom__")
        && let Some(custom) = &form.custom_text
    {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }

    if parts.is_empty() {
        "No selection made".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::claims_questions_generated;

    #[test]
    fn detects_canonical_hallucination_phrases() {
        // Real-world hallucination samples (chat.json archeology).
        assert!(claims_questions_generated(
            "3 questions generated for **Pricing Strategy** — take a look in the preview pane!"
        ));
        assert!(claims_questions_generated(
            "2 questions have been generated for Equipment & Supplies Readiness."
        ));
        assert!(claims_questions_generated(
            "I generated 5 questions for that practice."
        ));
    }

    #[test]
    fn ignores_unrelated_text() {
        assert!(!claims_questions_generated(
            "Generating questions for Pricing Strategy now…"
        ));
        assert!(!claims_questions_generated(
            "Let me know if you'd like more or fewer questions."
        ));
        assert!(!claims_questions_generated(""));
        assert!(!claims_questions_generated(
            "5 practices remaining across 2 domains."
        ));
    }
}

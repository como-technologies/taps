//! Tool definitions and executors for the agent loop.
//!
//! Each tool is a rig `Tool` that holds a reference to a per-request
//! `RequestContext` (behind `Arc<Mutex<_>>`). Tool calls mutate the context in
//! place (updating the project phase, marking flags, attaching clarifying
//! questions) and return a text summary that is fed back to the model.
//!
//! The handler (`handlers::chat`) reads the final `RequestContext` after the
//! agent loop returns to decide which HTMX triggers to fire.

use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;

use amaker_core::AppError;
use amaker_core::models::{
    AuthoringSubstate, BlockerType, ClarifyingQuestion, ClarifyingQuestionId, Conversation,
    EffortRange, EvidenceType, GeneratedQuestionsInfo, Polarity, PracticeId, Project,
    QuestionOption,
    assessment::{Domain, Practice, Question},
    ids::{BlockerTypeId, DomainId, EvidenceTypeId, QuestionId},
};
use amaker_core::services::{StorageService, YamlService, draft};

use crate::services::AiService;

// ============================================================================
// Per-request context shared across tool calls
// ============================================================================

/// Mutable state shared across all tool calls for a single chat turn.
///
/// Tools lock this, mutate the project/flags/clarifying question, persist, and
/// release. The handler reads the final state after the agent loop finishes.
pub struct RequestContext {
    pub project: Project,
    pub conversation: Conversation,
    pub storage: StorageService,
    pub ai: AiService,
    /// Set by `switch_focus` (the only mutator of `focus_state`/`focus_substate`).
    /// The chat handler reads this to fire the `focusChanged` HX trigger so the
    /// UI re-renders the focus indicator and dispatches the preview pane.
    pub focus_changed: bool,
    pub assessment_generated: bool,
    /// Quick-reply labels the model offered for this turn (rendered as pill buttons).
    pub suggestions: Vec<String>,
    /// Set when the model called `generate_questions` this turn.
    pub questions_generated: Option<GeneratedQuestionsInfo>,
}

impl RequestContext {
    pub fn new(
        project: Project,
        conversation: Conversation,
        storage: StorageService,
        ai: AiService,
    ) -> Self {
        Self {
            project,
            conversation,
            storage,
            ai,
            focus_changed: false,
            assessment_generated: false,
            suggestions: Vec::new(),
            questions_generated: None,
        }
    }
}

pub type SharedContext = Arc<Mutex<RequestContext>>;

// ============================================================================
// Tool input types (Deserialize from the model's JSON arguments)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SwitchFocusInput {
    /// Authoring substate to switch to. Wire values:
    /// `"scoping"`, `"structuring"`, `"questions"`, `"refining"`.
    pub substate: String,
    /// Free-text reason logged for observability.
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateStructureInput {
    pub assessment_name: String,
    pub context_summary: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateQuestionsInput {
    pub practice_id: PracticeId,
    #[serde(default)]
    pub context: Option<String>,
    /// Exact number of questions to generate. Enforced server-side: values
    /// outside the configured per-practice min/max are rejected so the outer
    /// LLM is forced to retry with a corrected value.
    #[serde(default)]
    pub target_count: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct SetQuestionBudgetInput {
    pub min: u16,
    pub max: u16,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AskClarifyingQuestionOptionInput {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OfferSuggestedRepliesInput {
    pub labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct VocabEntryInput {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TailorVocabularyInput {
    /// Full replacement list of evidence types. Omit to leave unchanged.
    #[serde(default)]
    pub evidence_types: Option<Vec<VocabEntryInput>>,
    /// Full replacement list of blocker types. Omit to leave unchanged.
    #[serde(default)]
    pub blocker_types: Option<Vec<VocabEntryInput>>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct AskClarifyingQuestionInput {
    pub question: String,
    pub options: Vec<AskClarifyingQuestionOptionInput>,
    #[serde(default = "default_true")]
    pub allow_custom: bool,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Debug, Deserialize)]
pub struct PublishAssessmentInput {
    /// User-supplied publish name. If absent, defaults to `v<n>` where
    /// `n` is the count of existing tags + 1.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional release-style note; becomes the annotated tag's message.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResetDraftFromVersionInput {
    /// Name of the published version to restore the draft from.
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct AddQuestionInput {
    pub practice_id: PracticeId,
    pub text: String,
    pub polarity: Polarity,
    #[serde(default)]
    pub guidance: Option<String>,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub remediation: Option<String>,
    #[serde(default)]
    pub roles: Option<Vec<String>>,
    #[serde(default)]
    pub effort: Option<EffortRange>,
}

#[derive(Debug, Deserialize)]
pub struct EditQuestionInput {
    pub question_id: QuestionId,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub polarity: Option<Polarity>,
    #[serde(default)]
    pub guidance: Option<String>,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub remediation: Option<String>,
    #[serde(default)]
    pub roles: Option<Vec<String>>,
    #[serde(default)]
    pub effort: Option<EffortRange>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuestionInput {
    pub question_id: QuestionId,
}

#[derive(Debug, Deserialize)]
pub struct RegenerateQuestionInput {
    pub question_id: QuestionId,
    /// What should change in the rewrite (the SME's correction or steer).
    pub feedback: String,
}

#[derive(Debug, Deserialize)]
pub struct AddPracticeInput {
    pub domain_id: DomainId,
    pub name: String,
    pub context: String,
    pub value: String,
    pub risk: String,
    #[serde(default)]
    pub guidance: Option<String>,
    #[serde(default)]
    pub terminology: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EditPracticeInput {
    pub practice_id: PracticeId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub risk: Option<String>,
    #[serde(default)]
    pub guidance: Option<String>,
    #[serde(default)]
    pub terminology: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeletePracticeInput {
    pub practice_id: PracticeId,
    /// How to handle the practice's questions. Required when the practice has
    /// dependents; the tool refuses with a dependents list when omitted in
    /// that case.
    #[serde(default)]
    pub disposition: Option<DeleteDisposition>,
}

#[derive(Debug, Deserialize)]
pub struct MovePracticeInput {
    pub practice_id: PracticeId,
    pub target_domain_id: DomainId,
    /// Insertion index in the target domain. Omit to append.
    #[serde(default)]
    pub position: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderPracticesInput {
    pub domain_id: DomainId,
    /// New ordering. Must be a permutation of the domain's current practice ids.
    pub order: Vec<PracticeId>,
}

#[derive(Debug, Deserialize)]
pub struct AddDomainInput {
    pub name: String,
    pub context: String,
    pub value: String,
    pub risk: String,
    #[serde(default)]
    pub terminology: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EditDomainInput {
    pub domain_id: DomainId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub risk: Option<String>,
    #[serde(default)]
    pub terminology: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteDomainInput {
    pub domain_id: DomainId,
    #[serde(default)]
    pub disposition: Option<DeleteDisposition>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderDomainsInput {
    /// Must be a permutation of the assessment's current domain ids.
    pub order: Vec<DomainId>,
}

/// How a delete tool should handle entities that depend on the target.
///
/// Used by `delete_domain` (practices + questions are dependents) and
/// `delete_practice` (questions are dependents). The orchestrator must pick
/// one; tools refuse to guess. The target UUID in `ReparentTo` is interpreted
/// as a sibling of whatever is being deleted (e.g. a sibling domain when
/// deleting a domain, a sibling practice when deleting a practice).
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeleteDisposition {
    /// Delete the target and every entity beneath it.
    Cascade,
    /// Move dependents under `target` (a sibling of the deleted entity).
    ReparentTo { target: String },
    /// Refuse the delete unless the target has no dependents.
    AbortIfOrphan,
}

fn default_true() -> bool {
    true
}

/// Validate a proposed `target_count` against per-practice env bounds, waiving
/// the env range when a project-level aggregate budget is set. With a budget,
/// the aggregate total is the real ceiling (checked separately).
fn check_target_count_bounds(
    target_count: Option<u8>,
    env_min: u8,
    env_max: u8,
    budget_max: Option<u16>,
) -> Result<(), AppError> {
    let Some(n) = target_count else {
        return Ok(());
    };
    if n == 0 {
        return Err(AppError::BadRequest("target_count must be >= 1.".into()));
    }
    if budget_max.is_some() {
        // Aggregate budget active — per-practice env range is waived.
        return Ok(());
    }
    if n < env_min || n > env_max {
        return Err(AppError::BadRequest(format!(
            "target_count {} is outside the allowed range {}..={}. Re-call with a value in that range.",
            n, env_min, env_max
        )));
    }
    Ok(())
}

// ============================================================================
// Tool: switch_focus
// ============================================================================
//
// Phase 3 retires `advance_phase` and `go_back_phase` in favor of a single
// pure substate indicator. The orchestrator infers focus most of the time
// from conversation; `switch_focus` exists for the cases where a deliberate
// reset is helpful (e.g. SME explicitly says "let's go back to the domains").
// The tool mutates only `project.focus_substate`; no data gate, no prerequisite
// check. Respondent and analyst views live at separate routes, not in
// `Project`, so they are not focus targets.

#[derive(Clone)]
pub struct SwitchFocusTool {
    pub ctx: SharedContext,
}

impl Tool for SwitchFocusTool {
    const NAME: &'static str = "switch_focus";
    type Error = AppError;
    type Args = SwitchFocusInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Move the authoring focus indicator to a different substate. Use \
                sparingly — only when a deliberate reset helps (e.g. SME says \
                'let's go back to the domains'). This is a hint, not a gate: \
                surgical edits work from any substate."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "substate": {
                        "type": "string",
                        "enum": ["scoping", "structuring", "questions", "refining"]
                    },
                    "reason": {
                        "type": "string",
                        "description": "Brief explanation of why the focus is shifting"
                    }
                },
                "required": ["substate", "reason"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let substate = AuthoringSubstate::from_wire(&args.substate.to_lowercase())
            .ok_or_else(|| {
                AppError::BadRequest(format!(
                    "switch_focus: unknown substate '{}'. Use scoping, structuring, questions, or refining.",
                    args.substate
                ))
            })?;

        let mut ctx = self.ctx.lock().await;
        let from = ctx.project.focus_substate;
        ctx.project.set_substate(substate);
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        ctx.focus_changed = true;
        tracing::info!(
            "Switched focus {:?} -> {:?}: {}",
            from,
            substate,
            args.reason
        );
        Ok(format!("Focus switched to {substate}. {}", args.reason))
    }
}

// ============================================================================
// Tool: generate_structure
// ============================================================================

#[derive(Clone)]
pub struct GenerateStructureTool {
    pub ctx: SharedContext,
}

impl Tool for GenerateStructureTool {
    const NAME: &'static str = "generate_structure";
    type Error = AppError;
    type Args = GenerateStructureInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Generate the assessment structure including domains and practices (without questions). \
                Call this during the Structuring phase when you have enough context to create the structure. \
                The structure will appear in the preview panel, then you'll add questions in the next phase."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "assessment_name": {
                        "type": "string",
                        "description": "Name for the assessment"
                    },
                    "context_summary": {
                        "type": "string",
                        "description": "Summary of gathered context to inform structure generation"
                    }
                },
                "required": ["assessment_name", "context_summary"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!("Starting structure generation: {}", args.assessment_name);

        // Snapshot the bits we need, release the lock before the LLM call.
        let (chat_history, ai, storage, project_id) = {
            let ctx = self.ctx.lock().await;
            let chat_history = ctx
                .conversation
                .messages
                .iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n\n");
            (
                chat_history,
                ctx.ai.clone(),
                ctx.storage.clone(),
                ctx.project.id,
            )
        };

        // First-time-only guard: regenerating the whole structure would
        // restamp UUIDs and orphan transcript references. The surgical CRUD
        // tools (add/edit/delete_domain etc.) are the right surface for any
        // change after the initial structure exists. See draft-publish.md
        // §"Surgical CRUD tool surface".
        if let Some(yaml) = storage.load_assessment_yaml(project_id).await? {
            let existing = YamlService::parse_assessment(&yaml)?;
            if existing.domain_count() > 0 {
                return Err(AppError::BadRequest(format!(
                    "An assessment structure already exists ({} domains, {} practices). \
                     `generate_structure` is restricted to first-time generation — \
                     reshaping the structure later would rewrite UUIDs and orphan \
                     transcript links. For edits use the surgical tools \
                     (`add_domain`, `edit_domain`, `delete_domain`, `reorder_domains`, \
                     `add_practice`, `move_practice`, etc.).",
                    existing.domain_count(),
                    existing.practice_count()
                )));
            }
        }

        let assessment = ai
            .generate_structure(&args.context_summary, &chat_history)
            .await?;

        let yaml_with_ids = assessment
            .to_yaml()
            .map_err(|e| AppError::Internal(format!("Failed to serialize assessment: {}", e)))?;

        // Re-lock to persist. Focus is left alone — the orchestrator is
        // responsible for calling `switch_focus` to Authoring/Questions once
        // the SME signals they're ready to draft questions.
        let mut ctx = self.ctx.lock().await;
        let _commit_message = format!(
            "generate initial structure '{}' ({} domains, {} practices)",
            assessment.name,
            assessment.domain_count(),
            assessment.practice_count()
        );
        ctx.storage
            .save_assessment_yaml(project_id, &yaml_with_ids)
            .await?;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        ctx.assessment_generated = true;

        tracing::debug!(
            "Generated structure '{}' with {} domains, {} practices",
            assessment.name,
            assessment.domain_count(),
            assessment.practice_count(),
        );

        let structure_summary = assessment
            .domains
            .iter()
            .map(|d| {
                let practices: Vec<_> = d
                    .practices
                    .iter()
                    .map(|p| format!("  - {} (id: {})", p.name, p.id))
                    .collect();
                format!("**{}**:\n{}", d.name, practices.join("\n"))
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            "Successfully generated structure '{}' with {} domains and {} practices.\n\n\
            ## Structure Created\n{}\n\n\
            The structure is now visible in the preview panel. When the SME is ready, \
            call `switch_focus(state=\"authoring\", substate=\"questions\")` to start \
            drafting questions for each practice.",
            assessment.name,
            assessment.domain_count(),
            assessment.practice_count(),
            structure_summary
        ))
    }
}

// ============================================================================
// Tool: generate_questions
// ============================================================================

#[derive(Clone)]
pub struct GenerateQuestionsTool {
    pub ctx: SharedContext,
}

impl Tool for GenerateQuestionsTool {
    const NAME: &'static str = "generate_questions";
    type Error = AppError;
    type Args = GenerateQuestionsInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Generate questions for a specific practice. Call this during the Questions phase \
                to add questions to one practice at a time. The SME can review and provide feedback \
                before moving to the next practice."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "practice_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "UUID of the practice to generate questions for"
                    },
                    "context": {
                        "type": "string",
                        "description": "Any additional context or preferences for question generation"
                    },
                    "target_count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 255,
                        "description": "Exact number of questions to generate for this practice. Omit to let the sub-LLM pick within the advisory range. Values outside the configured per-practice min/max will be rejected and you'll be asked to retry."
                    }
                },
                "required": ["practice_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!(
            "Starting question generation for practice {} (target_count={:?})",
            args.practice_id,
            args.target_count
        );

        // Load assessment + pick out the practice to pass to the LLM.
        let (ai, storage, project_id, mut assessment, practice_name, budget_max) = {
            let ctx = self.ctx.lock().await;
            let yaml = ctx
                .storage
                .load_assessment_yaml(ctx.project.id)
                .await?
                .ok_or_else(|| AppError::Internal("No assessment structure found".to_string()))?;
            let assessment = YamlService::parse_assessment(&yaml)?;
            let practice = assessment
                .domains
                .iter()
                .flat_map(|d| d.practices.iter())
                .find(|p| p.id == args.practice_id)
                .ok_or_else(|| {
                    AppError::NotFound(format!("Practice {} not found", args.practice_id))
                })?;
            // First-time-only guard: regenerating a practice's questions
            // wholesale would replace UUIDs the SME may have referenced.
            // After the initial pass, surgical add/edit/delete/regenerate
            // tools are the right surface.
            if !practice.questions.is_empty() {
                return Err(AppError::BadRequest(format!(
                    "Practice '{}' already has {} question(s). `generate_questions` is \
                     restricted to first-time authoring per practice — replacing the set \
                     would rewrite UUIDs. Use `add_question`, `edit_question`, \
                     `delete_question`, or `regenerate_question` for surgical changes.",
                    practice.name,
                    practice.questions.len()
                )));
            }
            let name = practice.name.clone();
            (
                ctx.ai.clone(),
                ctx.storage.clone(),
                ctx.project.id,
                assessment,
                name,
                ctx.project.question_budget_max,
            )
        };

        // Bounds check `target_count` against configured policy BEFORE hitting
        // the sub-LLM. Returning `AppError::BadRequest` here surfaces the error
        // text to the orchestration LLM via rig, closing the outer feedback
        // loop so the orchestrator can self-correct.
        check_target_count_bounds(
            args.target_count,
            ai.questions_min_per_practice(),
            ai.questions_max_per_practice(),
            budget_max,
        )?;

        // Aggregate budget check: when an aggregate commitment is set on the
        // project, require `target_count` and refuse calls that would push the
        // running total past `budget_max`. Mirrors the per-practice error
        // shape so rig feeds both back to the orchestrator identically.
        if let Some(max) = budget_max {
            let Some(n) = args.target_count else {
                return Err(AppError::BadRequest(
                    "A question budget is set; target_count is required on every generate_questions call.".into(),
                ));
            };
            let current_total = assessment.question_count() as u32;
            let projected = current_total + n as u32;
            if projected > max as u32 {
                let remaining = (max as u32).saturating_sub(current_total);
                return Err(AppError::BadRequest(format!(
                    "target_count {} would bring total to {}, exceeding the committed max {}. Remaining capacity: {}. Either lower target_count, or call set_question_budget to revise the commitment.",
                    n, projected, max, remaining
                )));
            }
        }

        // Take the practice by reference to generate — no lock held here.
        let practice_ref: &Practice = assessment
            .domains
            .iter()
            .flat_map(|d| d.practices.iter())
            .find(|p| p.id == args.practice_id)
            .expect("practice existed during snapshot");

        let questions = ai
            .generate_questions_for_practice(
                practice_ref,
                args.context.as_deref(),
                args.target_count,
            )
            .await?;
        let question_count = questions.len();
        // Polarity counts go into the tool's reply message so the orchestrator
        // can restate the breakdown verbatim instead of guessing. The
        // generation tool is the only authoritative source for this fact.
        let positive_count = questions
            .iter()
            .filter(|q| q.polarity == Polarity::Positive)
            .count();
        let negative_count = question_count - positive_count;

        // Apply the questions to the assessment and persist.
        if let Some(practice) = assessment
            .domains
            .iter_mut()
            .flat_map(|d| d.practices.iter_mut())
            .find(|p| p.id == args.practice_id)
        {
            practice.questions = questions;
        }

        let updated_yaml = serde_yaml::to_string(&assessment)
            .map_err(|e| AppError::Internal(format!("Failed to serialize assessment: {}", e)))?;
        let _commit_message = format!(
            "generate {} questions for practice '{}'",
            question_count, practice_name
        );
        storage
            .save_assessment_yaml(project_id, &updated_yaml)
            .await?;

        // Per-practice completion is derived from the assessment tree itself
        // (a practice is "done" iff `!questions.is_empty()`) — there's no
        // separate marker on `Project` anymore. Recompute totals from the
        // freshly-mutated `assessment` to report progress.
        let total_practices = assessment.practice_count();
        let completed_practices = assessment
            .domains
            .iter()
            .flat_map(|d| &d.practices)
            .filter(|p| !p.questions.is_empty())
            .count();
        let remaining = total_practices - completed_practices;

        {
            let mut ctx = self.ctx.lock().await;
            ctx.project.touch();
            ctx.storage.save_project(&ctx.project).await?;
            ctx.assessment_generated = true;
            ctx.questions_generated = Some(GeneratedQuestionsInfo {
                practice_id: args.practice_id,
                practice_name: practice_name.clone(),
                count: question_count,
            });
        }

        tracing::info!(
            "Generated {} questions for practice '{}'. {}/{} practices complete.",
            question_count,
            practice_name,
            completed_practices,
            total_practices
        );

        let polarity_breakdown =
            format!("{} positive, {} negative", positive_count, negative_count);
        let message = if remaining == 0 {
            format!(
                "Generated {} questions for '{}' ({}). All {} practices now have questions — when the SME is happy, consider `switch_focus` to authoring/refining for polish, or publish.",
                question_count, practice_name, polarity_breakdown, total_practices
            )
        } else {
            format!(
                "Generated {} questions for '{}' ({}). {} of {} practices complete, {} remaining.",
                question_count,
                practice_name,
                polarity_breakdown,
                completed_practices,
                total_practices,
                remaining
            )
        };
        Ok(message)
    }
}

// ============================================================================
// Tool: set_question_budget
// ============================================================================

const BUDGET_MAX_CAP: u16 = 500;

#[derive(Clone)]
pub struct SetQuestionBudgetTool {
    pub ctx: SharedContext,
}

impl Tool for SetQuestionBudgetTool {
    const NAME: &'static str = "set_question_budget";
    type Error = AppError;
    type Args = SetQuestionBudgetInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Record an aggregate question-count commitment for the whole assessment (e.g., the SME said 'medium, 20-30 questions'). \
                 Once set, every `generate_questions` call requires `target_count` and the server rejects calls that would push the running total past `max`. \
                 Also blocks advancing out of the Questions phase if the final total is outside [min, max]. \
                 Call this as soon as a total-count commitment surfaces in the conversation; call again to revise."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "min": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": BUDGET_MAX_CAP,
                        "description": "Minimum total questions across the whole assessment."
                    },
                    "max": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": BUDGET_MAX_CAP,
                        "description": "Maximum total questions across the whole assessment. Must be >= min."
                    },
                    "reason": {
                        "type": "string",
                        "description": "Optional short note on why this budget was chosen (e.g., 'SME picked Medium')."
                    }
                },
                "required": ["min", "max"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        if args.min == 0 {
            return Err(AppError::BadRequest(
                "Question budget min must be >= 1.".into(),
            ));
        }
        if args.min > args.max {
            return Err(AppError::BadRequest(format!(
                "Question budget min ({}) cannot exceed max ({}).",
                args.min, args.max
            )));
        }
        if args.max > BUDGET_MAX_CAP {
            return Err(AppError::BadRequest(format!(
                "Question budget max {} exceeds the supported cap of {}.",
                args.max, BUDGET_MAX_CAP
            )));
        }

        let mut ctx = self.ctx.lock().await;
        ctx.project.question_budget_min = Some(args.min);
        ctx.project.question_budget_max = Some(args.max);
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;

        tracing::info!(
            "Question budget set to {}-{}{}",
            args.min,
            args.max,
            args.reason
                .as_deref()
                .map(|r| format!(" ({})", r))
                .unwrap_or_default()
        );

        Ok(format!(
            "Question budget set to {}-{} questions total. Every generate_questions call now requires target_count and is enforced against this budget.",
            args.min, args.max
        ))
    }
}

// ============================================================================
// Tool: ask_clarifying_question
// ============================================================================

#[derive(Clone)]
pub struct AskClarifyingQuestionTool {
    pub ctx: SharedContext,
}

impl Tool for AskClarifyingQuestionTool {
    const NAME: &'static str = "ask_clarifying_question";
    type Error = AppError;
    type Args = AskClarifyingQuestionInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Ask the user a clarifying question with predefined options. Use this to gather \
                specific information about the assessment scope, audience, or requirements. The user will \
                see clickable option cards and can select one or provide a custom answer."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to ask the user"
                    },
                    "options": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": {
                                    "type": "string",
                                    "description": "The main text of the option"
                                },
                                "description": {
                                    "type": "string",
                                    "description": "Optional additional context for this option"
                                }
                            },
                            "required": ["label"]
                        },
                        "description": "Available options for the user to choose from (2-6 recommended)"
                    },
                    "allow_custom": {
                        "type": "boolean",
                        "description": "Whether to allow the user to enter a custom answer (default: true)"
                    },
                    "multi_select": {
                        "type": "boolean",
                        "description": "Whether to allow selecting multiple options (default: false)"
                    }
                },
                "required": ["question", "options"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!(
            "Queuing clarifying question: {} ({} options, allow_custom={}, multi_select={})",
            args.question,
            args.options.len(),
            args.allow_custom,
            args.multi_select
        );

        let options: Vec<QuestionOption> = args
            .options
            .into_iter()
            .map(|o| QuestionOption {
                label: o.label,
                description: o.description,
            })
            .collect();
        let question = ClarifyingQuestion {
            id: ClarifyingQuestionId::new(),
            question: args.question.clone(),
            options,
            allow_custom: args.allow_custom,
            multi_select: args.multi_select,
        };

        // Append to the project's queue (persisted). The handler pops the first
        // entry off as the "active" card each interaction; any remainder drains
        // via respond_to_question / skip_question without further LLM turns.
        let mut ctx = self.ctx.lock().await;
        ctx.project.pending_clarifying_questions.push(question);
        let depth = ctx.project.pending_clarifying_questions.len();
        if depth > 5 {
            tracing::warn!(
                "ask_clarifying_question queue depth is {} — model is over-chaining",
                depth
            );
        }
        Ok(format!(
            "Queued clarifying question #{}: {}",
            depth, args.question
        ))
    }
}

// ============================================================================
// Tool: offer_suggested_replies
// ============================================================================

/// Lightweight "quick reply" pills. Unlike `ask_clarifying_question`, this
/// doesn't block the conversation — the user can still type freeform into the
/// main text area. Only the most recent turn's pills are shown in the UI.
#[derive(Clone)]
pub struct OfferSuggestedRepliesTool {
    pub ctx: SharedContext,
}

impl Tool for OfferSuggestedRepliesTool {
    const NAME: &'static str = "offer_suggested_replies";
    type Error = AppError;
    type Args = OfferSuggestedRepliesInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Offer the user 2-4 clickable quick-reply pill buttons alongside your text response. \
                Use this when your message ends with a question that has natural short answers (e.g., 'Ready', \
                'Looks good', 'Go back', 'Continue', 'Yes'). Do NOT use this for open-ended questions or when \
                the user needs to provide details — call `ask_clarifying_question` for those instead. \
                The user can always type a freeform response in the text area and ignore the pills. \
                Keep each label under 30 characters."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "labels": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 2,
                        "maxItems": 4,
                        "description": "Short quick-reply labels (2-4 items, each under 30 chars)"
                    }
                },
                "required": ["labels"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Trim + drop empties defensively; cap length so the UI doesn't blow up.
        let cleaned: Vec<String> = args
            .labels
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(60).collect::<String>())
            .take(4)
            .collect();
        tracing::info!("Offered {} suggested replies", cleaned.len());
        let count = cleaned.len();
        let mut ctx = self.ctx.lock().await;
        ctx.suggestions = cleaned;
        Ok(format!("Offered {} quick-reply suggestions", count))
    }
}

// ============================================================================
// Tool: tailor_vocabulary
// ============================================================================

#[derive(Clone)]
pub struct TailorVocabularyTool {
    pub ctx: SharedContext,
}

impl Tool for TailorVocabularyTool {
    const NAME: &'static str = "tailor_vocabulary";
    type Error = AppError;
    type Args = TailorVocabularyInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Customize this assessment's evidence and blocker vocabularies to fit the domain. \
                Evidence types describe what supports a 'yes' answer (e.g. 'Audit report', 'Temperature log'). \
                Blocker types describe what's preventing a 'no' (e.g. 'Budget', 'Staff turnover'). \
                Provide a FULL replacement list for either; omit a list to leave it unchanged. \
                Ids must be short snake_case slugs stable across edits. \
                Call this during Scoping or Structuring when the default vocabulary (generic 'People/Time/Technology/Training/Other/Unknown' etc.) doesn't fit the domain."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "evidence_types": {
                        "type": "array",
                        "description": "Full replacement list of evidence types",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Short snake_case slug (e.g. 'audit_report')" },
                                "label": { "type": "string", "description": "Human-readable label" },
                                "description": { "type": "string", "description": "Optional clarifying description" }
                            },
                            "required": ["id", "label"]
                        }
                    },
                    "blocker_types": {
                        "type": "array",
                        "description": "Full replacement list of blocker types",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Short snake_case slug (e.g. 'staff_turnover')" },
                                "label": { "type": "string", "description": "Human-readable label" },
                                "description": { "type": "string", "description": "Optional clarifying description" }
                            },
                            "required": ["id", "label"]
                        }
                    },
                    "reason": {
                        "type": "string",
                        "description": "Why the vocabulary is being changed (for the transcript)"
                    }
                },
                "required": ["reason"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut ctx = self.ctx.lock().await;
        let project_id = ctx.project.id;

        // Load the current assessment — tailor_vocabulary is only useful once
        // a structure exists, but during Scoping it may be called before one
        // does. In that case we stash the vocabulary on the project for the
        // next generate_structure call to pick up. For simplicity v1: require
        // an assessment to exist.
        let yaml = ctx
            .storage
            .load_assessment_yaml(project_id)
            .await?
            .ok_or_else(|| {
                AppError::Internal(
                    "No assessment.yaml yet — tailor_vocabulary runs after generate_structure."
                        .to_string(),
                )
            })?;
        let mut assessment = YamlService::parse_assessment(&yaml)
            .map_err(|e| AppError::Internal(format!("parse assessment: {}", e)))?;

        let mut changes = Vec::new();
        if let Some(list) = args.evidence_types {
            let n = list.len();
            assessment.evidence_types = list
                .into_iter()
                .map(|e| EvidenceType {
                    id: EvidenceTypeId::new(e.id),
                    label: e.label,
                    description: e.description,
                })
                .collect();
            changes.push(format!("{} evidence types", n));
        }
        if let Some(list) = args.blocker_types {
            let n = list.len();
            assessment.blocker_types = list
                .into_iter()
                .map(|b| BlockerType {
                    id: BlockerTypeId::new(b.id),
                    label: b.label,
                    description: b.description,
                })
                .collect();
            changes.push(format!("{} blocker types", n));
        }

        if changes.is_empty() {
            return Ok("No vocabulary changes requested.".to_string());
        }

        let new_yaml = assessment
            .to_yaml()
            .map_err(|e| AppError::Internal(format!("serialize assessment: {}", e)))?;
        let _commit_message = format!("tailor vocabulary: {}", changes.join(", "));
        ctx.storage
            .save_assessment_yaml(project_id, &new_yaml)
            .await?;
        ctx.assessment_generated = true;

        tracing::info!(
            "Tailored vocabulary: {} (reason: {})",
            changes.join(", "),
            args.reason
        );

        Ok(format!(
            "Updated vocabulary: {}. The new lists are active for all future answers.",
            changes.join(" and ")
        ))
    }
}

// ============================================================================
// Tool: publish_assessment
// ============================================================================

#[derive(Clone)]
pub struct PublishAssessmentTool {
    pub ctx: SharedContext,
}

impl Tool for PublishAssessmentTool {
    const NAME: &'static str = "publish_assessment";
    type Error = AppError;
    type Args = PublishAssessmentInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Publish the current draft as a named, immutable version. \
                Creates an annotated git tag pointing at the latest assessment commit so \
                respondents bind to that exact shape — future draft edits won't break their \
                answers. Refuses if the name is already in use; ask the user for a different \
                one and try again."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Optional descriptive name. If omitted, defaults to 'v<n>' where n is the next sequential number."
                    },
                    "notes": {
                        "type": "string",
                        "description": "Optional 'what changed since last version' note; becomes the annotated tag's message."
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (project_id, storage) = {
            let ctx = self.ctx.lock().await;
            (ctx.project.id, ctx.storage.clone())
        };

        // Resolve publish name: trimmed user-supplied or `v<n+1>` based on
        // the number of versions already on record.
        let name = match args.name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => {
                let versions = storage.list_versions(project_id).await?;
                format!("v{}", versions.len() + 1)
            }
        };

        let notes = args.notes.as_deref().filter(|s| !s.is_empty());
        storage.publish_version(project_id, &name, notes).await?;

        tracing::info!("Published assessment as '{}'", name);
        Ok(format!("Published as '{}'.", name))
    }
}

// ============================================================================
// Tool: reset_draft_from_version
// ============================================================================

#[derive(Clone)]
pub struct ResetDraftFromVersionTool {
    pub ctx: SharedContext,
}

impl Tool for ResetDraftFromVersionTool {
    const NAME: &'static str = "reset_draft_from_version";
    type Error = AppError;
    type Args = ResetDraftFromVersionInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Restore the draft assessment from a previously-published version. \
                Overwrites the working `assessment.yaml` with the tagged version's content \
                and commits the reset. Responses bound to other published versions are \
                unaffected — they read from their own tag."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "version": {
                        "type": "string",
                        "description": "The published version name (git tag) to restore from."
                    }
                },
                "required": ["version"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (project_id, storage) = {
            let ctx = self.ctx.lock().await;
            (ctx.project.id, ctx.storage.clone())
        };

        storage
            .reset_draft_from_version(project_id, &args.version)
            .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        ctx.assessment_generated = true;

        tracing::info!("Reset draft from version '{}'", args.version);
        Ok(format!("Restored draft from '{}'.", args.version))
    }
}

// ============================================================================
// Surgical question CRUD
// ============================================================================
//
// Each tool: snapshot context, call `draft::edit_yaml` with a closure that
// mutates the in-memory `Assessment`, report back. UUIDs are minted on add and
// preserved on edit/regenerate so transcript links and response bindings stay
// valid across draft churn.

/// Common JSON schema for the editable Question fields, shared between
/// add/edit so the tool definitions stay in lockstep with the struct.
fn question_field_properties() -> serde_json::Value {
    json!({
        "text": { "type": "string", "description": "The question text (binary yes/no/unknown)." },
        "polarity": {
            "type": "string",
            "enum": ["positive", "negative"],
            "description": "'positive' = yes is good; 'negative' = yes is a problem."
        },
        "guidance": { "type": "string" },
        "evidence": { "type": "string" },
        "remediation": { "type": "string" },
        "roles": { "type": "array", "items": { "type": "string" } },
        "effort": {
            "type": "object",
            "properties": {
                "min_hours": { "type": "integer", "minimum": 0 },
                "max_hours": { "type": "integer", "minimum": 0 }
            },
            "required": ["min_hours", "max_hours"]
        }
    })
}

#[derive(Clone)]
pub struct AddQuestionTool {
    pub ctx: SharedContext,
}

impl Tool for AddQuestionTool {
    const NAME: &'static str = "add_question";
    type Error = AppError;
    type Args = AddQuestionInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let mut props = question_field_properties();
        let obj = props.as_object_mut().unwrap();
        obj.insert(
            "practice_id".into(),
            json!({ "type": "string", "format": "uuid" }),
        );
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Append a single question to a practice. Mints a fresh UUID. \
                Use this for surgical 'add one more about X' edits — not for bulk authoring \
                (that's `generate_questions`)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": props,
                "required": ["practice_id", "text", "polarity"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let practice_id = args.practice_id;
        let new_question = Question {
            id: QuestionId::new(),
            text: args.text,
            polarity: args.polarity,
            guidance: args.guidance,
            evidence: args.evidence,
            remediation: args.remediation,
            roles: args.roles.unwrap_or_default(),
            effort: args.effort,
        };
        let new_id = new_question.id;
        let _commit_message = format!("add question to practice {practice_id}");

        let (_, (practice_name, count_after)) = draft::edit_yaml(&storage, project_id, move |a| {
            let practice = a
                .find_practice_mut(practice_id)
                .ok_or_else(|| AppError::NotFound(format!("Practice {practice_id} not found")))?;
            practice.questions.push(new_question);
            Ok((practice.name.clone(), practice.questions.len()))
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;

        tracing::info!(
            "Added question {} to practice '{}' (now {} questions)",
            new_id,
            practice_name,
            count_after
        );
        Ok(format!(
            "Added a question to '{practice_name}' (id: {new_id}). Practice now has {count_after} questions."
        ))
    }
}

#[derive(Clone)]
pub struct EditQuestionTool {
    pub ctx: SharedContext,
}

impl Tool for EditQuestionTool {
    const NAME: &'static str = "edit_question";
    type Error = AppError;
    type Args = EditQuestionInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let mut props = question_field_properties();
        let obj = props.as_object_mut().unwrap();
        obj.insert(
            "question_id".into(),
            json!({ "type": "string", "format": "uuid" }),
        );
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Patch fields on an existing question. Any field you omit is left \
                unchanged; UUIDs are preserved. Use this for targeted rewrites \
                (e.g. changing polarity, fixing wording). For LLM-driven rewrites \
                guided by SME feedback, use `regenerate_question` instead."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": props,
                "required": ["question_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let question_id = args.question_id;
        let mut fields_changed: Vec<&'static str> = Vec::new();
        if args.text.is_some() {
            fields_changed.push("text");
        }
        if args.polarity.is_some() {
            fields_changed.push("polarity");
        }
        if args.guidance.is_some() {
            fields_changed.push("guidance");
        }
        if args.evidence.is_some() {
            fields_changed.push("evidence");
        }
        if args.remediation.is_some() {
            fields_changed.push("remediation");
        }
        if args.roles.is_some() {
            fields_changed.push("roles");
        }
        if args.effort.is_some() {
            fields_changed.push("effort");
        }
        if fields_changed.is_empty() {
            return Err(AppError::BadRequest(
                "edit_question called with no fields set; supply at least one field to change."
                    .into(),
            ));
        }
        let _commit_message = format!(
            "edit question {} ({})",
            question_id,
            fields_changed.join(", ")
        );

        let (_, text_after) = draft::edit_yaml(&storage, project_id, move |a| {
            let q = a
                .find_question_mut(question_id)
                .ok_or_else(|| AppError::NotFound(format!("Question {question_id} not found")))?;
            if let Some(text) = args.text {
                q.text = text;
            }
            if let Some(polarity) = args.polarity {
                q.polarity = polarity;
            }
            if let Some(guidance) = args.guidance {
                q.guidance = Some(guidance);
            }
            if let Some(evidence) = args.evidence {
                q.evidence = Some(evidence);
            }
            if let Some(remediation) = args.remediation {
                q.remediation = Some(remediation);
            }
            if let Some(roles) = args.roles {
                q.roles = roles;
            }
            if let Some(effort) = args.effort {
                q.effort = Some(effort);
            }
            Ok(q.text.clone())
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;

        tracing::info!(
            "Edited question {} ({})",
            question_id,
            fields_changed.join(", ")
        );
        Ok(format!(
            "Updated question {question_id} ({}). Current text: \"{text_after}\".",
            fields_changed.join(", ")
        ))
    }
}

#[derive(Clone)]
pub struct DeleteQuestionTool {
    pub ctx: SharedContext,
}

impl Tool for DeleteQuestionTool {
    const NAME: &'static str = "delete_question";
    type Error = AppError;
    type Args = DeleteQuestionInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Remove a single question from the draft. Questions are leaves; \
                no disposition is needed. The question's UUID will no longer resolve \
                in future transcripts — only call this when the SME explicitly asks \
                to drop the question."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question_id": { "type": "string", "format": "uuid" }
                },
                "required": ["question_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let question_id = args.question_id;
        let _commit_message = format!("delete question {question_id}");

        let (_, removed_text) = draft::edit_yaml(&storage, project_id, move |a| {
            let removed = a
                .remove_question(question_id)
                .ok_or_else(|| AppError::NotFound(format!("Question {question_id} not found")))?;
            Ok(removed.text)
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;

        tracing::info!("Deleted question {} (\"{}\")", question_id, removed_text);
        Ok(format!(
            "Deleted question \"{removed_text}\" (id: {question_id})."
        ))
    }
}

#[derive(Clone)]
pub struct RegenerateQuestionTool {
    pub ctx: SharedContext,
}

impl Tool for RegenerateQuestionTool {
    const NAME: &'static str = "regenerate_question";
    type Error = AppError;
    type Args = RegenerateQuestionInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "LLM-driven rewrite of a single question, guided by SME feedback. \
                Preserves the question's UUID so transcript links stay live. Use this \
                when the SME says things like 'rewrite Q3 to be less leading' — for \
                deterministic field patches use `edit_question`."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question_id": { "type": "string", "format": "uuid" },
                    "feedback": {
                        "type": "string",
                        "description": "What should change in the rewrite (SME steer)."
                    }
                },
                "required": ["question_id", "feedback"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Snapshot: load AI handle + the existing question + its parent practice
        // (the sub-LLM needs both as context). No lock held across the LLM call.
        let (ai, storage, project_id, practice, existing) = {
            let ctx = self.ctx.lock().await;
            let yaml = ctx
                .storage
                .load_assessment_yaml(ctx.project.id)
                .await?
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "No draft assessment yet. Call generate_structure first.".into(),
                    )
                })?;
            let assessment = YamlService::parse_assessment(&yaml)?;
            let (practice, existing) = assessment
                .domains
                .iter()
                .flat_map(|d| d.practices.iter())
                .find_map(|p| {
                    p.questions
                        .iter()
                        .find(|q| q.id == args.question_id)
                        .map(|q| (p.clone(), q.clone()))
                })
                .ok_or_else(|| {
                    AppError::NotFound(format!("Question {} not found", args.question_id))
                })?;
            (
                ctx.ai.clone(),
                ctx.storage.clone(),
                ctx.project.id,
                practice,
                existing,
            )
        };

        let mut rewritten = ai
            .regenerate_question(&practice, &existing, &args.feedback)
            .await?;
        // Preserve the original UUID — the extractor mints a fresh one.
        rewritten.id = existing.id;
        let new_text = rewritten.text.clone();
        let question_id = args.question_id;
        let _commit_message = format!("regenerate question {question_id}");

        draft::edit_yaml(&storage, project_id, move |a| {
            let q = a
                .find_question_mut(question_id)
                .ok_or_else(|| AppError::NotFound(format!("Question {question_id} not found")))?;
            *q = rewritten;
            Ok(())
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;

        tracing::info!(
            "Regenerated question {} in practice '{}'",
            question_id,
            practice.name
        );
        Ok(format!(
            "Rewrote question {question_id} in '{}'. New text: \"{new_text}\".",
            practice.name
        ))
    }
}

// ============================================================================
// Surgical practice CRUD
// ============================================================================

/// JSON schema fragment for the editable Practice fields.
fn practice_field_properties() -> serde_json::Value {
    json!({
        "name": { "type": "string" },
        "context": { "type": "string", "description": "What this practice represents." },
        "value": { "type": "string", "description": "Why this practice matters." },
        "risk": { "type": "string", "description": "What's at stake if it's missing." },
        "guidance": { "type": "string" },
        "terminology": { "type": "string", "description": "Alt name (e.g. 'Control', 'Activity')." }
    })
}

#[derive(Clone)]
pub struct AddPracticeTool {
    pub ctx: SharedContext,
}

impl Tool for AddPracticeTool {
    const NAME: &'static str = "add_practice";
    type Error = AppError;
    type Args = AddPracticeInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let mut props = practice_field_properties();
        let obj = props.as_object_mut().unwrap();
        obj.insert(
            "domain_id".into(),
            json!({ "type": "string", "format": "uuid" }),
        );
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Append a new practice to a domain. Mints a fresh UUID.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": props,
                "required": ["domain_id", "name", "context", "value", "risk"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let domain_id = args.domain_id;
        let new_practice = Practice {
            id: PracticeId::new(),
            name: args.name,
            context: args.context,
            value: args.value,
            risk: args.risk,
            questions: Vec::new(),
            guidance: args.guidance,
            terminology: args.terminology,
        };
        let new_id = new_practice.id;
        let _commit_message = format!("add practice to domain {domain_id}");

        let (_, (domain_name, count_after)) = draft::edit_yaml(&storage, project_id, move |a| {
            let domain = a
                .find_domain_mut(domain_id)
                .ok_or_else(|| AppError::NotFound(format!("Domain {domain_id} not found")))?;
            domain.practices.push(new_practice);
            Ok((domain.name.clone(), domain.practices.len()))
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        tracing::info!(
            "Added practice {} to domain '{}' (now {} practices)",
            new_id,
            domain_name,
            count_after
        );
        Ok(format!(
            "Added a practice to '{domain_name}' (id: {new_id}). Domain now has {count_after} practices."
        ))
    }
}

#[derive(Clone)]
pub struct EditPracticeTool {
    pub ctx: SharedContext,
}

impl Tool for EditPracticeTool {
    const NAME: &'static str = "edit_practice";
    type Error = AppError;
    type Args = EditPracticeInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let mut props = practice_field_properties();
        let obj = props.as_object_mut().unwrap();
        obj.insert(
            "practice_id".into(),
            json!({ "type": "string", "format": "uuid" }),
        );
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Patch fields on an existing practice. Omitted fields are left \
                unchanged; UUIDs preserved."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": props,
                "required": ["practice_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let practice_id = args.practice_id;
        let mut fields_changed: Vec<&'static str> = Vec::new();
        if args.name.is_some() {
            fields_changed.push("name");
        }
        if args.context.is_some() {
            fields_changed.push("context");
        }
        if args.value.is_some() {
            fields_changed.push("value");
        }
        if args.risk.is_some() {
            fields_changed.push("risk");
        }
        if args.guidance.is_some() {
            fields_changed.push("guidance");
        }
        if args.terminology.is_some() {
            fields_changed.push("terminology");
        }
        if fields_changed.is_empty() {
            return Err(AppError::BadRequest(
                "edit_practice called with no fields set; supply at least one to change.".into(),
            ));
        }
        let _commit_message = format!(
            "edit practice {practice_id} ({})",
            fields_changed.join(", ")
        );

        let (_, name_after) = draft::edit_yaml(&storage, project_id, move |a| {
            let p = a
                .find_practice_mut(practice_id)
                .ok_or_else(|| AppError::NotFound(format!("Practice {practice_id} not found")))?;
            if let Some(name) = args.name {
                p.name = name;
            }
            if let Some(context) = args.context {
                p.context = context;
            }
            if let Some(value) = args.value {
                p.value = value;
            }
            if let Some(risk) = args.risk {
                p.risk = risk;
            }
            if let Some(guidance) = args.guidance {
                p.guidance = Some(guidance);
            }
            if let Some(terminology) = args.terminology {
                p.terminology = Some(terminology);
            }
            Ok(p.name.clone())
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        tracing::info!(
            "Edited practice {} ({})",
            practice_id,
            fields_changed.join(", ")
        );
        Ok(format!(
            "Updated practice '{name_after}' ({})",
            fields_changed.join(", ")
        ))
    }
}

#[derive(Clone)]
pub struct DeletePracticeTool {
    pub ctx: SharedContext,
}

impl Tool for DeletePracticeTool {
    const NAME: &'static str = "delete_practice";
    type Error = AppError;
    type Args = DeletePracticeInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Delete a practice. When the practice has questions you must \
                pass a `disposition`: `cascade` deletes the questions too, \
                `reparent_to` moves them under a sibling practice (UUID in `target`), \
                `abort_if_orphan` refuses and tells you what would be lost. \
                Omitting `disposition` on a practice with questions returns a structured \
                refusal listing the dependents so you can dialog with the SME."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "practice_id": { "type": "string", "format": "uuid" },
                    "disposition": {
                        "type": "object",
                        "description": "How to handle dependent questions.",
                        "properties": {
                            "kind": { "type": "string", "enum": ["cascade", "reparent_to", "abort_if_orphan"] },
                            "target": { "type": "string", "format": "uuid", "description": "Required when kind=reparent_to; the sibling practice id." }
                        },
                        "required": ["kind"]
                    }
                },
                "required": ["practice_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let practice_id = args.practice_id;
        let disposition = args.disposition;
        let _commit_message = format!("delete practice {practice_id}");

        let (_, removed_name) = draft::edit_yaml(&storage, project_id, move |a| {
            // Locate the practice and inspect its dependents.
            let (domain_idx, practice_idx, question_count) = a
                .domains
                .iter()
                .enumerate()
                .find_map(|(di, d)| {
                    d.practices.iter().enumerate().find_map(|(pi, p)| {
                        (p.id == practice_id).then_some((di, pi, p.questions.len()))
                    })
                })
                .ok_or_else(|| AppError::NotFound(format!("Practice {practice_id} not found")))?;

            let practice_name = a.domains[domain_idx].practices[practice_idx].name.clone();

            // No dependents: succeed regardless of disposition value.
            if question_count == 0 {
                return Ok(a.remove_practice(practice_id).expect("just located").name);
            }

            let Some(disposition) = disposition else {
                let dependents: Vec<String> = a.domains[domain_idx].practices[practice_idx]
                    .questions
                    .iter()
                    .map(|q| format!("- {} (id: {})", q.text, q.id))
                    .collect();
                return Err(AppError::BadRequest(format!(
                    "Practice '{practice_name}' has {question_count} question(s). \
                         Re-call delete_practice with a `disposition`:\n\
                         - `cascade` to delete them with the practice\n\
                         - `reparent_to` (with `target`: a sibling practice id) to move them\n\
                         - `abort_if_orphan` to bail out\n\n\
                         Dependents:\n{}",
                    dependents.join("\n")
                )));
            };

            match disposition {
                DeleteDisposition::Cascade => {
                    Ok(a.remove_practice(practice_id).expect("just located").name)
                }
                DeleteDisposition::AbortIfOrphan => Err(AppError::BadRequest(format!(
                    "Refusing delete: practice '{practice_name}' has {question_count} question(s)."
                ))),
                DeleteDisposition::ReparentTo { target } => {
                    let target_id: PracticeId = target.parse().map_err(|e| {
                        AppError::BadRequest(format!("invalid target practice id: {e}"))
                    })?;
                    if target_id == practice_id {
                        return Err(AppError::BadRequest(
                            "reparent target must be a different practice.".into(),
                        ));
                    }
                    // Take the questions out first to drop the borrow on the source
                    // practice before we look for the target (which may live in
                    // another domain).
                    let moved_questions = std::mem::take(
                        &mut a.domains[domain_idx].practices[practice_idx].questions,
                    );
                    let moved_count = moved_questions.len();
                    // Locate target across the whole tree.
                    let target_practice = a.find_practice_mut(target_id).ok_or_else(|| {
                        AppError::NotFound(format!("Target practice {target_id} not found"))
                    })?;
                    target_practice.questions.extend(moved_questions);
                    // Now remove the source practice.
                    let removed = a.remove_practice(practice_id).expect("just located");
                    Ok(format!(
                        "{} (moved {moved_count} question(s) to {target_id})",
                        removed.name
                    ))
                }
            }
        })
        .await?;

        {
            let mut ctx = self.ctx.lock().await;
            ctx.assessment_generated = true;
            ctx.project.touch();
            ctx.storage.save_project(&ctx.project).await?;
        }

        tracing::info!("Deleted practice {} ({})", practice_id, removed_name);
        Ok(format!("Deleted practice {removed_name}."))
    }
}

#[derive(Clone)]
pub struct MovePracticeTool {
    pub ctx: SharedContext,
}

impl Tool for MovePracticeTool {
    const NAME: &'static str = "move_practice";
    type Error = AppError;
    type Args = MovePracticeInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Move a practice (with its questions) from its current domain into \
                a different domain. UUIDs are preserved. Omit `position` to append."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "practice_id": { "type": "string", "format": "uuid" },
                    "target_domain_id": { "type": "string", "format": "uuid" },
                    "position": { "type": "integer", "minimum": 0 }
                },
                "required": ["practice_id", "target_domain_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let practice_id = args.practice_id;
        let target_domain_id = args.target_domain_id;
        let position = args.position;
        let _commit_message = format!("move practice {practice_id} to domain {target_domain_id}");

        let (_, summary) = draft::edit_yaml(&storage, project_id, move |a| {
            // Locate source domain.
            let source_domain_idx = a
                .domains
                .iter()
                .position(|d| d.practices.iter().any(|p| p.id == practice_id))
                .ok_or_else(|| {
                    AppError::NotFound(format!("Practice {practice_id} not found"))
                })?;
            // No-op when source == target.
            if a.domains[source_domain_idx].id == target_domain_id {
                return Err(AppError::BadRequest(
                    "Practice is already in the target domain. Use reorder_practices to change its position within a domain.".into(),
                ));
            }
            // Detach the practice.
            let practice_idx = a.domains[source_domain_idx]
                .practices
                .iter()
                .position(|p| p.id == practice_id)
                .expect("just located");
            let practice = a.domains[source_domain_idx].practices.remove(practice_idx);
            let practice_name = practice.name.clone();
            let source_domain_name = a.domains[source_domain_idx].name.clone();

            // Attach to target.
            let target = a.find_domain_mut(target_domain_id).ok_or_else(|| {
                AppError::NotFound(format!("Target domain {target_domain_id} not found"))
            })?;
            let target_name = target.name.clone();
            let insert_idx = position.unwrap_or(target.practices.len()).min(target.practices.len());
            target.practices.insert(insert_idx, practice);
            Ok(format!(
                "moved '{practice_name}' from '{source_domain_name}' to '{target_name}' at position {insert_idx}"
            ))
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        tracing::info!("{summary}");
        Ok(format!("Moved practice: {summary}."))
    }
}

#[derive(Clone)]
pub struct ReorderPracticesTool {
    pub ctx: SharedContext,
}

impl Tool for ReorderPracticesTool {
    const NAME: &'static str = "reorder_practices";
    type Error = AppError;
    type Args = ReorderPracticesInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Reorder the practices within a domain. `order` must be a \
                permutation of the domain's current practice ids — extras, omissions, \
                or unknowns are rejected."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "domain_id": { "type": "string", "format": "uuid" },
                    "order": {
                        "type": "array",
                        "items": { "type": "string", "format": "uuid" }
                    }
                },
                "required": ["domain_id", "order"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let domain_id = args.domain_id;
        let order = args.order;
        let _commit_message = format!("reorder practices in domain {domain_id}");

        let (_, count) = draft::edit_yaml(&storage, project_id, move |a| {
            let domain = a
                .find_domain_mut(domain_id)
                .ok_or_else(|| AppError::NotFound(format!("Domain {domain_id} not found")))?;
            validate_permutation(
                &order,
                &domain.practices.iter().map(|p| p.id).collect::<Vec<_>>(),
                "practice",
            )?;
            let mut by_id: std::collections::HashMap<PracticeId, Practice> =
                domain.practices.drain(..).map(|p| (p.id, p)).collect();
            domain.practices = order
                .iter()
                .map(|id| by_id.remove(id).expect("validated above"))
                .collect();
            Ok(domain.practices.len())
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        tracing::info!("Reordered {} practices in domain {}", count, domain_id);
        Ok(format!("Reordered {count} practices."))
    }
}

// ============================================================================
// Surgical domain CRUD
// ============================================================================

fn domain_field_properties() -> serde_json::Value {
    json!({
        "name": { "type": "string" },
        "context": { "type": "string" },
        "value": { "type": "string" },
        "risk": { "type": "string" },
        "terminology": { "type": "string", "description": "Alt name (e.g. 'Stage', 'Pillar')." }
    })
}

#[derive(Clone)]
pub struct AddDomainTool {
    pub ctx: SharedContext,
}

impl Tool for AddDomainTool {
    const NAME: &'static str = "add_domain";
    type Error = AppError;
    type Args = AddDomainInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Append a new domain to the assessment. Mints a fresh UUID.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": domain_field_properties(),
                "required": ["name", "context", "value", "risk"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let new_domain = Domain {
            id: DomainId::new(),
            name: args.name,
            context: args.context,
            value: args.value,
            risk: args.risk,
            practices: Vec::new(),
            terminology: args.terminology,
        };
        let new_id = new_domain.id;
        let _commit_message = format!("add domain {new_id}");

        let (_, (domain_name, count_after)) = draft::edit_yaml(&storage, project_id, move |a| {
            let name = new_domain.name.clone();
            a.domains.push(new_domain);
            Ok((name, a.domains.len()))
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        tracing::info!(
            "Added domain {} '{}' (now {} domains)",
            new_id,
            domain_name,
            count_after
        );
        Ok(format!(
            "Added domain '{domain_name}' (id: {new_id}). Assessment now has {count_after} domains."
        ))
    }
}

#[derive(Clone)]
pub struct EditDomainTool {
    pub ctx: SharedContext,
}

impl Tool for EditDomainTool {
    const NAME: &'static str = "edit_domain";
    type Error = AppError;
    type Args = EditDomainInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let mut props = domain_field_properties();
        let obj = props.as_object_mut().unwrap();
        obj.insert(
            "domain_id".into(),
            json!({ "type": "string", "format": "uuid" }),
        );
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Patch fields on an existing domain. Omitted fields are left \
                unchanged; UUIDs preserved."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": props,
                "required": ["domain_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let domain_id = args.domain_id;
        let mut fields_changed: Vec<&'static str> = Vec::new();
        if args.name.is_some() {
            fields_changed.push("name");
        }
        if args.context.is_some() {
            fields_changed.push("context");
        }
        if args.value.is_some() {
            fields_changed.push("value");
        }
        if args.risk.is_some() {
            fields_changed.push("risk");
        }
        if args.terminology.is_some() {
            fields_changed.push("terminology");
        }
        if fields_changed.is_empty() {
            return Err(AppError::BadRequest(
                "edit_domain called with no fields set; supply at least one to change.".into(),
            ));
        }
        let _commit_message = format!("edit domain {domain_id} ({})", fields_changed.join(", "));

        let (_, name_after) = draft::edit_yaml(&storage, project_id, move |a| {
            let d = a
                .find_domain_mut(domain_id)
                .ok_or_else(|| AppError::NotFound(format!("Domain {domain_id} not found")))?;
            if let Some(name) = args.name {
                d.name = name;
            }
            if let Some(context) = args.context {
                d.context = context;
            }
            if let Some(value) = args.value {
                d.value = value;
            }
            if let Some(risk) = args.risk {
                d.risk = risk;
            }
            if let Some(terminology) = args.terminology {
                d.terminology = Some(terminology);
            }
            Ok(d.name.clone())
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        tracing::info!(
            "Edited domain {} ({})",
            domain_id,
            fields_changed.join(", ")
        );
        Ok(format!(
            "Updated domain '{name_after}' ({})",
            fields_changed.join(", ")
        ))
    }
}

#[derive(Clone)]
pub struct DeleteDomainTool {
    pub ctx: SharedContext,
}

impl Tool for DeleteDomainTool {
    const NAME: &'static str = "delete_domain";
    type Error = AppError;
    type Args = DeleteDomainInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Delete a domain. When it contains practices you must pass a \
                `disposition`: `cascade` removes the whole subtree, `reparent_to` \
                (with `target`: a sibling domain id) moves the practices, \
                `abort_if_orphan` refuses. Omitting `disposition` on a domain with \
                practices returns a structured refusal listing dependents."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "domain_id": { "type": "string", "format": "uuid" },
                    "disposition": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["cascade", "reparent_to", "abort_if_orphan"] },
                            "target": { "type": "string", "format": "uuid" }
                        },
                        "required": ["kind"]
                    }
                },
                "required": ["domain_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let domain_id = args.domain_id;
        let disposition = args.disposition;
        let _commit_message = format!("delete domain {domain_id}");

        let (_, removed_name) = draft::edit_yaml(&storage, project_id, move |a| {
            let domain_idx = a
                .domains
                .iter()
                .position(|d| d.id == domain_id)
                .ok_or_else(|| AppError::NotFound(format!("Domain {domain_id} not found")))?;
            let practice_count = a.domains[domain_idx].practices.len();
            let domain_name = a.domains[domain_idx].name.clone();

            if practice_count == 0 {
                return Ok(a.remove_domain(domain_id).expect("just located").name);
            }

            let Some(disposition) = disposition else {
                let dependents: Vec<String> = a.domains[domain_idx]
                    .practices
                    .iter()
                    .map(|p| {
                        format!(
                            "- {} ({} question(s), id: {})",
                            p.name,
                            p.questions.len(),
                            p.id
                        )
                    })
                    .collect();
                return Err(AppError::BadRequest(format!(
                    "Domain '{domain_name}' has {practice_count} practice(s). \
                     Re-call delete_domain with a `disposition`:\n\
                     - `cascade` to delete the whole subtree\n\
                     - `reparent_to` (with `target`: a sibling domain id) to move practices\n\
                     - `abort_if_orphan` to bail out\n\n\
                     Dependents:\n{}",
                    dependents.join("\n")
                )));
            };

            match disposition {
                DeleteDisposition::Cascade => {
                    Ok(a.remove_domain(domain_id).expect("just located").name)
                }
                DeleteDisposition::AbortIfOrphan => Err(AppError::BadRequest(format!(
                    "Refusing delete: domain '{domain_name}' has {practice_count} practice(s)."
                ))),
                DeleteDisposition::ReparentTo { target } => {
                    let target_id: DomainId = target.parse().map_err(|e| {
                        AppError::BadRequest(format!("invalid target domain id: {e}"))
                    })?;
                    if target_id == domain_id {
                        return Err(AppError::BadRequest(
                            "reparent target must be a different domain.".into(),
                        ));
                    }
                    let moved_practices = std::mem::take(&mut a.domains[domain_idx].practices);
                    let target_domain = a.find_domain_mut(target_id).ok_or_else(|| {
                        AppError::NotFound(format!("Target domain {target_id} not found"))
                    })?;
                    target_domain.practices.extend(moved_practices);
                    Ok(a.remove_domain(domain_id).expect("just located").name)
                }
            }
        })
        .await?;

        {
            let mut ctx = self.ctx.lock().await;
            ctx.assessment_generated = true;
            ctx.project.touch();
            ctx.storage.save_project(&ctx.project).await?;
        }

        tracing::info!("Deleted domain {} ({})", domain_id, removed_name);
        Ok(format!("Deleted domain '{removed_name}'."))
    }
}

#[derive(Clone)]
pub struct ReorderDomainsTool {
    pub ctx: SharedContext,
}

impl Tool for ReorderDomainsTool {
    const NAME: &'static str = "reorder_domains";
    type Error = AppError;
    type Args = ReorderDomainsInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Reorder the assessment's domains. `order` must be a permutation \
                of the current domain ids."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "order": {
                        "type": "array",
                        "items": { "type": "string", "format": "uuid" }
                    }
                },
                "required": ["order"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (storage, project_id) = {
            let ctx = self.ctx.lock().await;
            (ctx.storage.clone(), ctx.project.id)
        };
        let order = args.order;
        let _commit_message = "reorder domains".to_string();

        let (_, count) = draft::edit_yaml(&storage, project_id, move |a| {
            validate_permutation(
                &order,
                &a.domains.iter().map(|d| d.id).collect::<Vec<_>>(),
                "domain",
            )?;
            let mut by_id: std::collections::HashMap<DomainId, Domain> =
                a.domains.drain(..).map(|d| (d.id, d)).collect();
            a.domains = order
                .iter()
                .map(|id| by_id.remove(id).expect("validated above"))
                .collect();
            Ok(a.domains.len())
        })
        .await?;

        let mut ctx = self.ctx.lock().await;
        ctx.assessment_generated = true;
        ctx.project.touch();
        ctx.storage.save_project(&ctx.project).await?;
        tracing::info!("Reordered {count} domains");
        Ok(format!("Reordered {count} domains."))
    }
}

/// Check that `proposed` is a permutation of `current` (same ids, same length).
/// `entity_label` is used in error text ("practice" / "domain").
fn validate_permutation<T: Eq + std::hash::Hash + std::fmt::Display + Copy>(
    proposed: &[T],
    current: &[T],
    entity_label: &str,
) -> Result<(), AppError> {
    if proposed.len() != current.len() {
        return Err(AppError::BadRequest(format!(
            "reorder list has {} {entity_label}(s) but the current set has {}.",
            proposed.len(),
            current.len()
        )));
    }
    let current_set: std::collections::HashSet<_> = current.iter().copied().collect();
    let proposed_set: std::collections::HashSet<_> = proposed.iter().copied().collect();
    if current_set != proposed_set {
        let missing: Vec<String> = current_set
            .difference(&proposed_set)
            .map(|t| t.to_string())
            .collect();
        let extra: Vec<String> = proposed_set
            .difference(&current_set)
            .map(|t| t.to_string())
            .collect();
        return Err(AppError::BadRequest(format!(
            "reorder list must be a permutation. Missing: [{}]. Extra: [{}].",
            missing.join(", "),
            extra.join(", ")
        )));
    }
    if proposed_set.len() != proposed.len() {
        return Err(AppError::BadRequest(
            "reorder list contains duplicates.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use amaker_core::models::Assessment;
    use amaker_core::models::assessment::Domain;

    fn mk_project() -> Project {
        Project::new("p".to_string(), None)
    }

    #[test]
    fn generate_questions_input_deserializes_without_target_count() {
        let json = serde_json::json!({
            "practice_id": "00000000-0000-0000-0000-000000000001",
        });
        let input: GenerateQuestionsInput = serde_json::from_value(json).unwrap();
        assert!(input.target_count.is_none());
        assert!(input.context.is_none());
    }

    #[test]
    fn generate_questions_input_deserializes_with_target_count() {
        let json = serde_json::json!({
            "practice_id": "00000000-0000-0000-0000-000000000001",
            "target_count": 5,
        });
        let input: GenerateQuestionsInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.target_count, Some(5));
    }

    #[test]
    fn generate_questions_input_deserializes_target_count_null() {
        let json = serde_json::json!({
            "practice_id": "00000000-0000-0000-0000-000000000001",
            "target_count": serde_json::Value::Null,
        });
        let input: GenerateQuestionsInput = serde_json::from_value(json).unwrap();
        assert!(input.target_count.is_none());
    }

    #[tokio::test]
    async fn generate_questions_schema_exposes_optional_target_count() {
        let project = mk_project();
        let conversation = Conversation::new(project.id);
        let ctx = Arc::new(Mutex::new(
            // RequestContext is only needed to satisfy the tool's constructor;
            // `definition()` reads nothing from it.
            RequestContext::new(
                project,
                conversation,
                StorageService::new(
                    amaker_core::build_store(&amaker_core::StorageBackend::InMemory).unwrap(),
                ),
                AiService::new("", String::new(), 3, 12, 3).expect("stub ai"),
            ),
        ));
        let tool = GenerateQuestionsTool { ctx };
        let def = tool.definition(String::new()).await;
        let params = def.parameters;
        let props = params
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("properties object");
        assert!(
            props.contains_key("target_count"),
            "schema is missing target_count"
        );
        let required = params
            .get("required")
            .and_then(|r| r.as_array())
            .expect("required array");
        assert!(
            !required.iter().any(|v| v.as_str() == Some("target_count")),
            "target_count must remain optional"
        );
    }

    // -------- SetQuestionBudget --------

    fn budget_tool_ctx() -> SharedContext {
        let project = mk_project();
        let conversation = Conversation::new(project.id);
        Arc::new(Mutex::new(RequestContext::new(
            project,
            conversation,
            StorageService::new(
                amaker_core::build_store(&amaker_core::StorageBackend::InMemory).unwrap(),
            ),
            AiService::new("", String::new(), 3, 12, 3).expect("stub ai"),
        )))
    }

    #[test]
    fn set_question_budget_input_deserializes_without_reason() {
        let json = serde_json::json!({ "min": 20, "max": 30 });
        let input: SetQuestionBudgetInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.min, 20);
        assert_eq!(input.max, 30);
        assert!(input.reason.is_none());
    }

    #[test]
    fn set_question_budget_input_deserializes_with_reason() {
        let json = serde_json::json!({ "min": 20, "max": 30, "reason": "SME picked Medium" });
        let input: SetQuestionBudgetInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.reason.as_deref(), Some("SME picked Medium"));
    }

    #[tokio::test]
    async fn set_question_budget_rejects_min_zero() {
        let tool = SetQuestionBudgetTool {
            ctx: budget_tool_ctx(),
        };
        let err = tool
            .call(SetQuestionBudgetInput {
                min: 0,
                max: 10,
                reason: None,
            })
            .await
            .expect_err("min=0 must be rejected");
        assert!(matches!(err, AppError::BadRequest(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn set_question_budget_rejects_min_greater_than_max() {
        let tool = SetQuestionBudgetTool {
            ctx: budget_tool_ctx(),
        };
        let err = tool
            .call(SetQuestionBudgetInput {
                min: 30,
                max: 20,
                reason: None,
            })
            .await
            .expect_err("min>max must be rejected");
        match err {
            AppError::BadRequest(msg) => assert!(msg.contains("cannot exceed"), "msg: {msg}"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_question_budget_rejects_max_over_cap() {
        let tool = SetQuestionBudgetTool {
            ctx: budget_tool_ctx(),
        };
        let err = tool
            .call(SetQuestionBudgetInput {
                min: 1,
                max: BUDGET_MAX_CAP + 1,
                reason: None,
            })
            .await
            .expect_err("max > cap must be rejected");
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    // -------- check_target_count_bounds --------

    #[test]
    fn check_target_count_bounds_allows_none() {
        check_target_count_bounds(None, 3, 12, None).expect("None is always ok");
        check_target_count_bounds(None, 3, 12, Some(20)).expect("None is always ok with budget");
    }

    #[test]
    fn check_target_count_bounds_rejects_zero() {
        let err = check_target_count_bounds(Some(0), 3, 12, None).expect_err("0 is never ok");
        assert!(matches!(err, AppError::BadRequest(_)));
        // Still rejected under an active budget.
        let err = check_target_count_bounds(Some(0), 3, 12, Some(20))
            .expect_err("0 is never ok, even with budget");
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn check_target_count_bounds_enforces_env_range_without_budget() {
        check_target_count_bounds(Some(3), 3, 12, None).expect("in-range");
        check_target_count_bounds(Some(12), 3, 12, None).expect("at upper");
        let err = check_target_count_bounds(Some(2), 3, 12, None).expect_err("below min");
        assert!(matches!(err, AppError::BadRequest(ref msg) if msg.contains("3..=12")));
        let err = check_target_count_bounds(Some(13), 3, 12, None).expect_err("above max");
        assert!(matches!(err, AppError::BadRequest(ref msg) if msg.contains("3..=12")));
    }

    #[test]
    fn check_target_count_bounds_waives_env_range_under_budget() {
        // Aggregate budget is the only ceiling; per-practice env floor is waived.
        check_target_count_bounds(Some(1), 3, 12, Some(20)).expect("1 allowed under budget");
        check_target_count_bounds(Some(2), 3, 12, Some(20)).expect("2 allowed under budget");
        check_target_count_bounds(Some(13), 3, 12, Some(20))
            .expect("env max waived under budget (aggregate check handles ceiling)");
    }

    // -------- ask_clarifying_question queueing --------

    fn clarifying_tool_ctx() -> SharedContext {
        let project = mk_project();
        let conversation = Conversation::new(project.id);
        Arc::new(Mutex::new(RequestContext::new(
            project,
            conversation,
            StorageService::new(
                amaker_core::build_store(&amaker_core::StorageBackend::InMemory).unwrap(),
            ),
            AiService::new("", String::new(), 3, 12, 3).expect("stub ai"),
        )))
    }

    fn clarifying_input(question: &str) -> AskClarifyingQuestionInput {
        AskClarifyingQuestionInput {
            question: question.to_string(),
            options: vec![AskClarifyingQuestionOptionInput {
                label: "A".to_string(),
                description: None,
            }],
            allow_custom: false,
            multi_select: false,
        }
    }

    #[tokio::test]
    async fn ask_clarifying_question_appends_to_queue() {
        let ctx = clarifying_tool_ctx();
        let tool = AskClarifyingQuestionTool { ctx: ctx.clone() };

        let ack1 = tool.call(clarifying_input("Q1")).await.unwrap();
        let ack2 = tool.call(clarifying_input("Q2")).await.unwrap();
        let ack3 = tool.call(clarifying_input("Q3")).await.unwrap();

        assert!(ack1.contains("#1"), "ack1: {ack1}");
        assert!(ack2.contains("#2"), "ack2: {ack2}");
        assert!(ack3.contains("#3"), "ack3: {ack3}");

        let guard = ctx.lock().await;
        let queue = &guard.project.pending_clarifying_questions;
        assert_eq!(queue.len(), 3);
        assert_eq!(queue[0].question, "Q1");
        assert_eq!(queue[1].question, "Q2");
        assert_eq!(queue[2].question, "Q3");
    }

    // --- switch_focus ----------------------------------------------------

    fn switch_focus_ctx() -> SharedContext {
        let project = mk_project();
        let conversation = Conversation::new(project.id);
        Arc::new(Mutex::new(RequestContext::new(
            project,
            conversation,
            StorageService::new(
                amaker_core::build_store(&amaker_core::StorageBackend::InMemory).unwrap(),
            ),
            AiService::new("", String::new(), 3, 12, 3).expect("stub ai"),
        )))
    }

    #[tokio::test]
    async fn switch_focus_updates_substate() {
        let ctx = switch_focus_ctx();
        SwitchFocusTool { ctx: ctx.clone() }
            .call(SwitchFocusInput {
                substate: "structuring".into(),
                reason: "user ready to draft structure".into(),
            })
            .await
            .unwrap();
        let guard = ctx.lock().await;
        assert_eq!(guard.project.focus_substate, AuthoringSubstate::Structuring);
        assert!(guard.focus_changed);
    }

    #[tokio::test]
    async fn switch_focus_unknown_substate_refuses() {
        let ctx = switch_focus_ctx();
        let err = SwitchFocusTool { ctx: ctx.clone() }
            .call(SwitchFocusInput {
                substate: "drafting".into(),
                reason: "typo".into(),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    // --- Publish / reset behavior ----------------------------------------

    /// Build a SharedContext backed by an in-memory blob store for tests
    /// that exercise publish / reset / surgical CRUD. Returns a kept-alive
    /// TempDir slot (unused but preserved for call-site compatibility),
    /// the ctx, and the project_id.
    async fn fs_ctx() -> (
        tempfile::TempDir,
        SharedContext,
        amaker_core::models::ProjectId,
    ) {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = amaker_core::build_store(&amaker_core::StorageBackend::InMemory).unwrap();
        let storage = StorageService::new(store);
        storage.init().await.unwrap();
        let project = mk_project();
        let project_id = project.id;
        let conversation = Conversation::new(project_id);
        let ctx = Arc::new(Mutex::new(RequestContext::new(
            project,
            conversation,
            storage,
            AiService::new("", String::new(), 3, 12, 3).expect("stub ai"),
        )));
        (tmp, ctx, project_id)
    }

    /// In the blob-storage world there's no per-project init step — the
    /// "repo" is just a key prefix. This helper survives as an alias for
    /// call-site compatibility; it now just persists the project envelope.
    async fn fs_ctx_with_repo() -> (
        tempfile::TempDir,
        SharedContext,
        amaker_core::models::ProjectId,
    ) {
        let (tmp, ctx, pid) = fs_ctx().await;
        let (storage, project) = {
            let locked = ctx.lock().await;
            (locked.storage.clone(), locked.project.clone())
        };
        storage.save_project(&project).await.unwrap();
        (tmp, ctx, pid)
    }

    /// Seed `assessment.yaml` so publish has something to tag against.
    /// Returns the path written.
    async fn seed_assessment_yaml(
        ctx: &SharedContext,
        pid: amaker_core::models::ProjectId,
        yaml: &str,
        _description: &str,
    ) {
        let storage = { ctx.lock().await.storage.clone() };
        storage.save_assessment_yaml(pid, yaml).await.unwrap();
    }

    fn minimal_assessment_yaml() -> String {
        let mut a = Assessment::new("A".into(), "desc".into(), "goal".into());
        let mut d = Domain::new("D1".into(), "c".into(), "v".into(), "r".into());
        d.practices.push(Practice::new(
            "P1".into(),
            "c".into(),
            "v".into(),
            "r".into(),
        ));
        a.domains.push(d);
        a.to_yaml().unwrap()
    }

    #[tokio::test]
    async fn publish_with_no_name_uses_v1_then_v2() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        seed_assessment_yaml(&ctx, pid, &minimal_assessment_yaml(), "seed").await;

        let tool = PublishAssessmentTool { ctx: ctx.clone() };
        let out1 = tool
            .call(PublishAssessmentInput {
                name: None,
                notes: None,
            })
            .await
            .unwrap();
        assert!(out1.contains("'v1'"), "first publish should be v1: {out1}");

        let out2 = tool
            .call(PublishAssessmentInput {
                name: None,
                notes: None,
            })
            .await
            .unwrap();
        assert!(out2.contains("'v2'"), "second publish should be v2: {out2}");
    }

    #[tokio::test]
    async fn publish_with_explicit_name_uses_it() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        seed_assessment_yaml(&ctx, pid, &minimal_assessment_yaml(), "seed").await;

        let tool = PublishAssessmentTool { ctx: ctx.clone() };
        let out = tool
            .call(PublishAssessmentInput {
                name: Some("initial-draft".to_string()),
                notes: Some("first cut".to_string()),
            })
            .await
            .unwrap();
        assert!(out.contains("'initial-draft'"), "wrong name in: {out}");

        let storage = { ctx.lock().await.storage.clone() };
        let versions = storage.list_versions(pid).await.unwrap();
        assert!(versions.iter().any(|v| v.name == "initial-draft"));
    }

    #[tokio::test]
    async fn publish_refuses_duplicate_name() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        seed_assessment_yaml(&ctx, pid, &minimal_assessment_yaml(), "seed").await;

        let tool = PublishAssessmentTool { ctx: ctx.clone() };
        tool.call(PublishAssessmentInput {
            name: Some("v1".to_string()),
            notes: None,
        })
        .await
        .unwrap();

        let err = tool
            .call(PublishAssessmentInput {
                name: Some("v1".to_string()),
                notes: None,
            })
            .await
            .unwrap_err();
        match err {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("v1"), "msg should name the conflict: {msg}")
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    // (publish_flushes_dirty_tree_first deleted — the auto-commit-before-
    // publish step is gone with the git layer; publish now snapshots
    // whatever's currently in `draft.yaml` directly.)

    #[tokio::test]
    async fn reset_refuses_unknown_version() {
        let (_tmp, ctx, _pid) = fs_ctx_with_repo().await;
        let tool = ResetDraftFromVersionTool { ctx: ctx.clone() };
        let err = tool
            .call(ResetDraftFromVersionInput {
                version: "nonexistent".to_string(),
            })
            .await
            .unwrap_err();
        // The storage layer treats unknown versions as NotFound; reset
        // propagates that. BadRequest used to be the case under the git
        // backend; both are reasonable error shapes for "unknown version".
        match err {
            AppError::NotFound(msg) | AppError::BadRequest(msg) => {
                assert!(
                    msg.contains("nonexistent"),
                    "msg should name version: {msg}"
                )
            }
            other => panic!("expected NotFound or BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reset_restores_prior_yaml_content() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let v1_yaml = minimal_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &v1_yaml, "v1 seed").await;

        // Publish v1.
        PublishAssessmentTool { ctx: ctx.clone() }
            .call(PublishAssessmentInput {
                name: None,
                notes: None,
            })
            .await
            .unwrap();

        // Edit forward and commit.
        let v2_yaml = {
            let mut a = Assessment::new("A".into(), "v2 desc".into(), "goal".into());
            let mut d = Domain::new("D1".into(), "c".into(), "v".into(), "r".into());
            d.practices.push(Practice::new(
                "P-new".into(),
                "c".into(),
                "v".into(),
                "r".into(),
            ));
            a.domains.push(d);
            a.to_yaml().unwrap()
        };
        seed_assessment_yaml(&ctx, pid, &v2_yaml, "edit forward").await;

        // Reset back to v1.
        ResetDraftFromVersionTool { ctx: ctx.clone() }
            .call(ResetDraftFromVersionInput {
                version: "v1".to_string(),
            })
            .await
            .unwrap();

        let storage = { ctx.lock().await.storage.clone() };
        let restored = storage.load_assessment_yaml(pid).await.unwrap().unwrap();
        assert_eq!(restored, v1_yaml, "working copy should equal v1 content");
    }

    // --- Surgical question CRUD ------------------------------------------

    /// Seed an assessment with one practice and known question ids so each
    /// CRUD test can target them deterministically.
    fn seeded_assessment_yaml() -> (String, PracticeId, QuestionId) {
        let mut a = Assessment::new("A".into(), "d".into(), "g".into());
        let mut d = Domain::new("D".into(), "c".into(), "v".into(), "r".into());
        let mut p = Practice::new("P".into(), "c".into(), "v".into(), "r".into());
        let q = Question::new("Existing?".into());
        let q_id = q.id;
        let p_id = p.id;
        p.questions.push(q);
        d.practices.push(p);
        a.domains.push(d);
        (a.to_yaml().unwrap(), p_id, q_id)
    }

    #[tokio::test]
    async fn add_question_appends_and_commits() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let (yaml, p_id, _) = seeded_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &yaml, "seed").await;

        let reply = AddQuestionTool { ctx: ctx.clone() }
            .call(AddQuestionInput {
                practice_id: p_id,
                text: "Newly added?".into(),
                polarity: Polarity::Positive,
                guidance: None,
                evidence: None,
                remediation: None,
                roles: None,
                effort: None,
            })
            .await
            .unwrap();
        assert!(reply.contains("Newly added") || reply.contains("Added a question"));

        // YAML now has 2 questions on that practice.
        let storage = ctx.lock().await.storage.clone();
        let assessment_yaml = storage.load_assessment_yaml(pid).await.unwrap().unwrap();
        let assessment = YamlService::parse_assessment(&assessment_yaml).unwrap();
        let practice = assessment.find_practice_mut_via(&p_id);
        assert_eq!(practice.questions.len(), 2);
        assert!(practice.questions.iter().any(|q| q.text == "Newly added?"));
    }

    #[tokio::test]
    async fn add_question_rejects_unknown_practice() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let (yaml, _, _) = seeded_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &yaml, "seed").await;

        let err = AddQuestionTool { ctx: ctx.clone() }
            .call(AddQuestionInput {
                practice_id: PracticeId::new(),
                text: "?".into(),
                polarity: Polarity::Positive,
                guidance: None,
                evidence: None,
                remediation: None,
                roles: None,
                effort: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn edit_question_patches_only_supplied_fields_and_preserves_id() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let (yaml, _, q_id) = seeded_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &yaml, "seed").await;

        EditQuestionTool { ctx: ctx.clone() }
            .call(EditQuestionInput {
                question_id: q_id,
                text: Some("Revised?".into()),
                polarity: None,
                guidance: Some("look here".into()),
                evidence: None,
                remediation: None,
                roles: None,
                effort: None,
            })
            .await
            .unwrap();

        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        let q = parsed
            .domains
            .iter()
            .flat_map(|d| d.practices.iter())
            .flat_map(|p| p.questions.iter())
            .find(|q| q.id == q_id)
            .expect("question survives by id");
        assert_eq!(q.text, "Revised?");
        assert_eq!(q.guidance.as_deref(), Some("look here"));
        // Polarity untouched (defaulted Positive)
        assert_eq!(q.polarity, Polarity::Positive);
    }

    #[tokio::test]
    async fn edit_question_refuses_empty_patch() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let (yaml, _, q_id) = seeded_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &yaml, "seed").await;

        let err = EditQuestionTool { ctx: ctx.clone() }
            .call(EditQuestionInput {
                question_id: q_id,
                text: None,
                polarity: None,
                guidance: None,
                evidence: None,
                remediation: None,
                roles: None,
                effort: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn delete_question_removes_and_commits() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let (yaml, _, q_id) = seeded_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &yaml, "seed").await;

        DeleteQuestionTool { ctx: ctx.clone() }
            .call(DeleteQuestionInput { question_id: q_id })
            .await
            .unwrap();

        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.question_count(), 0);

        // Second delete returns NotFound.
        let err = DeleteQuestionTool { ctx: ctx.clone() }
            .call(DeleteQuestionInput { question_id: q_id })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    /// Helper so the test for `add_question_appends_and_commits` can fetch a
    /// practice without the mutability gymnastics. Kept inside `#[cfg(test)]`
    /// so it doesn't bleed into the production surface.
    trait AssessmentTestExt {
        fn find_practice_mut_via(&self, id: &PracticeId) -> &Practice;
    }
    impl AssessmentTestExt for Assessment {
        fn find_practice_mut_via(&self, id: &PracticeId) -> &Practice {
            self.domains
                .iter()
                .flat_map(|d| d.practices.iter())
                .find(|p| p.id == *id)
                .expect("practice exists")
        }
    }

    // --- Surgical practice CRUD ------------------------------------------

    /// Seed an assessment with two domains: D1 (P1 with one question, P2 empty),
    /// D2 (P3 empty). Returns the YAML plus the ids of interest.
    fn two_domain_assessment_yaml() -> TwoDomainSeed {
        let mut a = Assessment::new("A".into(), "d".into(), "g".into());
        let mut d1 = Domain::new("D1".into(), "c".into(), "v".into(), "r".into());
        let mut d2 = Domain::new("D2".into(), "c".into(), "v".into(), "r".into());
        let mut p1 = Practice::new("P1".into(), "c".into(), "v".into(), "r".into());
        let p2 = Practice::new("P2".into(), "c".into(), "v".into(), "r".into());
        let p3 = Practice::new("P3".into(), "c".into(), "v".into(), "r".into());
        let q = Question::new("Q?".into());
        let q_id = q.id;
        p1.questions.push(q);
        let p1_id = p1.id;
        let p2_id = p2.id;
        let p3_id = p3.id;
        let d1_id = d1.id;
        let d2_id = d2.id;
        d1.practices.push(p1);
        d1.practices.push(p2);
        d2.practices.push(p3);
        a.domains.push(d1);
        a.domains.push(d2);
        TwoDomainSeed {
            yaml: a.to_yaml().unwrap(),
            d1_id,
            d2_id,
            p1_id,
            p2_id,
            p3_id,
            q_id,
        }
    }

    struct TwoDomainSeed {
        yaml: String,
        d1_id: DomainId,
        d2_id: DomainId,
        p1_id: PracticeId,
        p2_id: PracticeId,
        p3_id: PracticeId,
        #[allow(dead_code)]
        q_id: QuestionId,
    }

    #[tokio::test]
    async fn add_practice_appends_to_target_domain() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        AddPracticeTool { ctx: ctx.clone() }
            .call(AddPracticeInput {
                domain_id: seed.d2_id,
                name: "P4".into(),
                context: "c".into(),
                value: "v".into(),
                risk: "r".into(),
                guidance: None,
                terminology: None,
            })
            .await
            .unwrap();

        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        let d2 = parsed.domains.iter().find(|d| d.id == seed.d2_id).unwrap();
        assert_eq!(d2.practices.len(), 2);
        assert_eq!(d2.practices.last().unwrap().name, "P4");
    }

    #[tokio::test]
    async fn delete_practice_no_dependents_succeeds_without_disposition() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        DeletePracticeTool { ctx: ctx.clone() }
            .call(DeletePracticeInput {
                practice_id: seed.p2_id,
                disposition: None,
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        assert!(
            !parsed
                .domains
                .iter()
                .any(|d| d.practices.iter().any(|p| p.id == seed.p2_id))
        );
    }

    #[tokio::test]
    async fn delete_practice_with_questions_refuses_without_disposition() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        let err = DeletePracticeTool { ctx: ctx.clone() }
            .call(DeletePracticeInput {
                practice_id: seed.p1_id,
                disposition: None,
            })
            .await
            .unwrap_err();
        let msg = match err {
            AppError::BadRequest(s) => s,
            other => panic!("expected BadRequest, got {other:?}"),
        };
        assert!(
            msg.contains("Dependents"),
            "refusal should list dependents: {msg}"
        );
        assert!(msg.contains("cascade"));
    }

    #[tokio::test]
    async fn delete_practice_cascade_drops_questions_and_practice() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        DeletePracticeTool { ctx: ctx.clone() }
            .call(DeletePracticeInput {
                practice_id: seed.p1_id,
                disposition: Some(DeleteDisposition::Cascade),
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.question_count(), 0);
        assert!(
            !parsed
                .domains
                .iter()
                .any(|d| d.practices.iter().any(|p| p.id == seed.p1_id))
        );
    }

    #[tokio::test]
    async fn delete_practice_reparent_moves_questions_to_target() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        DeletePracticeTool { ctx: ctx.clone() }
            .call(DeletePracticeInput {
                practice_id: seed.p1_id,
                disposition: Some(DeleteDisposition::ReparentTo {
                    target: seed.p3_id.to_string(),
                }),
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        // The original practice is gone, but the question survived inside the target.
        assert!(
            !parsed
                .domains
                .iter()
                .any(|d| d.practices.iter().any(|p| p.id == seed.p1_id))
        );
        let target_practice = parsed
            .domains
            .iter()
            .flat_map(|d| d.practices.iter())
            .find(|p| p.id == seed.p3_id)
            .unwrap();
        assert_eq!(target_practice.questions.len(), 1);
        assert_eq!(target_practice.questions[0].id, seed.q_id);
    }

    #[tokio::test]
    async fn delete_practice_abort_disposition_refuses_when_dependents_exist() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        let err = DeletePracticeTool { ctx: ctx.clone() }
            .call(DeletePracticeInput {
                practice_id: seed.p1_id,
                disposition: Some(DeleteDisposition::AbortIfOrphan),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn move_practice_relocates_to_target_domain() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        MovePracticeTool { ctx: ctx.clone() }
            .call(MovePracticeInput {
                practice_id: seed.p1_id,
                target_domain_id: seed.d2_id,
                position: Some(0),
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        let d2 = parsed.domains.iter().find(|d| d.id == seed.d2_id).unwrap();
        let d1 = parsed.domains.iter().find(|d| d.id == seed.d1_id).unwrap();
        assert_eq!(d2.practices.first().unwrap().id, seed.p1_id);
        assert!(d1.practices.iter().all(|p| p.id != seed.p1_id));
    }

    #[tokio::test]
    async fn move_practice_to_same_domain_refuses() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        let err = MovePracticeTool { ctx: ctx.clone() }
            .call(MovePracticeInput {
                practice_id: seed.p1_id,
                target_domain_id: seed.d1_id,
                position: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn reorder_practices_swaps_order() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        ReorderPracticesTool { ctx: ctx.clone() }
            .call(ReorderPracticesInput {
                domain_id: seed.d1_id,
                order: vec![seed.p2_id, seed.p1_id],
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        let d1 = parsed.domains.iter().find(|d| d.id == seed.d1_id).unwrap();
        assert_eq!(
            d1.practices.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![seed.p2_id, seed.p1_id]
        );
    }

    #[tokio::test]
    async fn reorder_practices_rejects_non_permutation() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        // Missing one id
        let err = ReorderPracticesTool { ctx: ctx.clone() }
            .call(ReorderPracticesInput {
                domain_id: seed.d1_id,
                order: vec![seed.p1_id],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));

        // Wrong id (p3 belongs to d2)
        let err = ReorderPracticesTool { ctx: ctx.clone() }
            .call(ReorderPracticesInput {
                domain_id: seed.d1_id,
                order: vec![seed.p1_id, seed.p3_id],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    // --- Surgical domain CRUD --------------------------------------------

    #[tokio::test]
    async fn add_domain_appends() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        AddDomainTool { ctx: ctx.clone() }
            .call(AddDomainInput {
                name: "D3".into(),
                context: "c".into(),
                value: "v".into(),
                risk: "r".into(),
                terminology: None,
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.domain_count(), 3);
        assert_eq!(parsed.domains.last().unwrap().name, "D3");
    }

    #[tokio::test]
    async fn delete_domain_with_practices_refuses_without_disposition() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        let err = DeleteDomainTool { ctx: ctx.clone() }
            .call(DeleteDomainInput {
                domain_id: seed.d1_id,
                disposition: None,
            })
            .await
            .unwrap_err();
        let msg = match err {
            AppError::BadRequest(s) => s,
            other => panic!("expected BadRequest, got {other:?}"),
        };
        assert!(msg.contains("Dependents"));
        assert!(msg.contains("cascade"));
    }

    #[tokio::test]
    async fn delete_domain_cascade_removes_subtree() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        DeleteDomainTool { ctx: ctx.clone() }
            .call(DeleteDomainInput {
                domain_id: seed.d1_id,
                disposition: Some(DeleteDisposition::Cascade),
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.domain_count(), 1);
        assert_eq!(parsed.practice_count(), 1); // P3 in D2 survives
    }

    #[tokio::test]
    async fn delete_domain_reparent_moves_practices() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        DeleteDomainTool { ctx: ctx.clone() }
            .call(DeleteDomainInput {
                domain_id: seed.d1_id,
                disposition: Some(DeleteDisposition::ReparentTo {
                    target: seed.d2_id.to_string(),
                }),
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.domain_count(), 1);
        let d2 = parsed.domains.iter().find(|d| d.id == seed.d2_id).unwrap();
        // P1 + P2 + P3 all under d2 now.
        assert_eq!(d2.practices.len(), 3);
        assert!(d2.practices.iter().any(|p| p.id == seed.p1_id));
        assert!(d2.practices.iter().any(|p| p.id == seed.p2_id));
    }

    #[tokio::test]
    async fn generate_structure_refuses_when_draft_non_empty() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        let err = GenerateStructureTool { ctx: ctx.clone() }
            .call(GenerateStructureInput {
                assessment_name: "X".into(),
                context_summary: "y".into(),
            })
            .await
            .unwrap_err();
        let msg = match err {
            AppError::BadRequest(s) => s,
            other => panic!("expected BadRequest, got {other:?}"),
        };
        assert!(msg.contains("first-time"));
        assert!(msg.contains("add_domain") || msg.contains("surgical"));
    }

    #[tokio::test]
    async fn generate_questions_refuses_when_practice_already_has_questions() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;
        // P1 has one question (from the seed); confirm refusal.

        let err = GenerateQuestionsTool { ctx: ctx.clone() }
            .call(GenerateQuestionsInput {
                practice_id: seed.p1_id,
                context: None,
                target_count: None,
            })
            .await
            .unwrap_err();
        let msg = match err {
            AppError::BadRequest(s) => s,
            other => panic!("expected BadRequest, got {other:?}"),
        };
        assert!(msg.contains("regenerate_question") || msg.contains("add_question"));
    }

    #[tokio::test]
    async fn reorder_domains_swaps() {
        let (_tmp, ctx, pid) = fs_ctx_with_repo().await;
        let seed = two_domain_assessment_yaml();
        seed_assessment_yaml(&ctx, pid, &seed.yaml, "seed").await;

        ReorderDomainsTool { ctx: ctx.clone() }
            .call(ReorderDomainsInput {
                order: vec![seed.d2_id, seed.d1_id],
            })
            .await
            .unwrap();
        let storage = ctx.lock().await.storage.clone();
        let parsed = YamlService::parse_assessment(
            &storage.load_assessment_yaml(pid).await.unwrap().unwrap(),
        )
        .unwrap();
        assert_eq!(
            parsed.domains.iter().map(|d| d.id).collect::<Vec<_>>(),
            vec![seed.d2_id, seed.d1_id]
        );
    }
}

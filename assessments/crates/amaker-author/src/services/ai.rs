//! LLM service wrapping the rig `Agent` pipeline.
//!
//! Holds an Anthropic `Client` plus a default model string. Exposes:
//!
//! - `run_agent` — build a per-request agent (tools attached via caller-provided
//!   closure) and run the multi-turn tool-use loop.
//! - `completion_only` — single-shot prompt with no tools (used for the
//!   `/assist` endpoint).
//! - `generate_structure` / `generate_questions_for_practice` — rig
//!   `Extractor`-based structured output with retry-on-spec-violation.
//! - `list_models` — hit the provider's `/v1/models` and return `ModelOption`s
//!   suitable for the UI picker.

use rig::agent::AgentBuilder;
use rig::completion::{Message, Prompt};
use rig::message::AssistantContent;
use rig::prelude::*;
use rig::providers::anthropic;

use amaker_core::AppError;
use amaker_core::models::AuthoringSubstate;
use amaker_core::models::ModelOption;
use amaker_core::models::assessment::{Assessment, Practice, Question, QuestionList};

/// Default completion cap for the agent loop; matches the previous SDK value.
const DEFAULT_MAX_TOKENS: u64 = 4096;
/// Cap for single-shot structure generation (larger for big YAML outputs).
const STRUCTURE_MAX_TOKENS: u64 = 8192;
/// Cap for assist/simulated SME responses.
const COMPLETION_MAX_TOKENS: u64 = 4096;
/// Default cap for multi-turn agent loops (tool round-trips).
pub const DEFAULT_MAX_TURNS: usize = 5;

/// Anthropic-backed LLM service.
#[derive(Clone)]
pub struct AiService {
    client: anthropic::Client,
    default_model: String,
    /// Minimum `target_count` the outer LLM may request.
    questions_min_per_practice: u8,
    /// Maximum `target_count` the outer LLM may request.
    questions_max_per_practice: u8,
    /// Count-mismatch retries before we surface a failure.
    question_gen_max_retries: u8,
}

impl AiService {
    /// Build a service bound to an API key, default model id, and the
    /// question-generation retry/bounds policy.
    pub fn new(
        api_key: &str,
        default_model: String,
        questions_min_per_practice: u8,
        questions_max_per_practice: u8,
        question_gen_max_retries: u8,
    ) -> Result<Self, AppError> {
        let client = anthropic::Client::new(api_key)
            .map_err(|e| AppError::Ai(format!("Failed to build Anthropic client: {e}")))?;
        Ok(Self {
            client,
            default_model,
            questions_min_per_practice,
            questions_max_per_practice,
            question_gen_max_retries,
        })
    }

    /// Minimum `target_count` allowed for `generate_questions`.
    pub fn questions_min_per_practice(&self) -> u8 {
        self.questions_min_per_practice
    }

    /// Maximum `target_count` allowed for `generate_questions`.
    pub fn questions_max_per_practice(&self) -> u8 {
        self.questions_max_per_practice
    }

    /// Fetch the caller's available models from the provider. Backs the UI picker.
    /// Preserves Anthropic's native order (newest first).
    pub async fn list_models(&self) -> Result<Vec<ModelOption>, AppError> {
        let list = self.client.list_models().await?;
        let models: Vec<ModelOption> = list
            .into_iter()
            .map(|m| ModelOption {
                label: m.display_name().to_string(),
                value: m.id,
            })
            .collect();
        Ok(models)
    }

    /// System prompt composer — stitches the base prompt with the
    /// substate-specific fragment and an optional assessment summary.
    pub fn system_prompt_for_substate(
        &self,
        substate: AuthoringSubstate,
        assessment_summary: Option<&str>,
    ) -> String {
        let base = include_str!("../prompts/system_base.md");
        let fragment = match substate {
            AuthoringSubstate::Scoping => include_str!("../prompts/authoring_scoping.md"),
            AuthoringSubstate::Structuring => include_str!("../prompts/authoring_structuring.md"),
            AuthoringSubstate::Questions => include_str!("../prompts/authoring_questions.md"),
            AuthoringSubstate::Refining => include_str!("../prompts/authoring_refining.md"),
        };

        let mut out = format!("{}\n\n{}", base, fragment);
        if let Some(summary) = assessment_summary {
            out.push_str("\n\n## Current Assessment Structure\n");
            out.push_str(summary);
        }
        out
    }

    /// Build a fresh agent for a given model + system prompt.
    ///
    /// The caller attaches phase-specific tools via `attach_tools` — this keeps
    /// the tool wiring next to the phase decision in the handler.
    fn agent_builder(
        &self,
        model_override: Option<&str>,
        system_prompt: &str,
        max_tokens: u64,
    ) -> AgentBuilder<anthropic::completion::CompletionModel> {
        let model = model_override.unwrap_or(&self.default_model);
        self.client
            .agent(model)
            .preamble(system_prompt)
            .max_tokens(max_tokens)
    }

    /// Drive a multi-turn tool-use loop with chat history.
    ///
    /// The caller supplies the exact set of tools to attach for this turn
    /// (phase-specific, bound to the per-request shared context).
    pub async fn run_agent(
        &self,
        system_prompt: &str,
        history: Vec<Message>,
        user_msg: String,
        model_override: Option<&str>,
        tools: Vec<Box<dyn rig::tool::ToolDyn>>,
    ) -> Result<String, AppError> {
        let builder = self.agent_builder(model_override, system_prompt, DEFAULT_MAX_TOKENS);
        let agent = builder.tools(tools).build();

        let model = model_override.unwrap_or(&self.default_model);
        tracing::info!(
            "Running agent: model={}, history_len={}, max_turns={}",
            model,
            history.len(),
            DEFAULT_MAX_TURNS
        );

        // rig PR #1659 (commit ac51cf4) handles the previously-fatal
        // empty-end_turn case from Anthropic — when the model considers
        // its turn finished after a terminal tool call, the loop now
        // terminates cleanly with prior-turn text preserved in
        // `r.messages`. We accumulate text across all assistant messages
        // because mixed-text-and-tool-call turns can appear earlier in
        // the loop and rig's `output` field only carries the last one.
        let result = agent
            .prompt(user_msg)
            .with_history(history)
            .max_turns(DEFAULT_MAX_TURNS)
            .extended_details()
            .await?;

        let output = accumulate_assistant_text(result.messages.as_deref());
        tracing::info!("Agent output: {} chars", output.len());
        Ok(output)
    }

    /// Single-shot prompt with no tools; used by the `/assist` endpoint for
    /// simulated-SME replies.
    pub async fn completion_only(
        &self,
        system_prompt: &str,
        user_msg: String,
        max_tokens: u64,
        model_override: Option<&str>,
    ) -> Result<String, AppError> {
        let agent = self
            .agent_builder(model_override, system_prompt, max_tokens)
            .build();
        Ok(agent.prompt(user_msg).await?)
    }

    /// Generate the assessment structure (domains + practices, no questions)
    /// via a rig `Extractor<Assessment>`. The sub-LLM is forced to emit a
    /// structured `submit` tool call; rig retries malformed output internally.
    pub async fn generate_structure(
        &self,
        context: &str,
        chat_history: &str,
    ) -> Result<Assessment, AppError> {
        let preamble = include_str!("../prompts/generate_structure.md");
        let extractor = self
            .client
            .extractor::<Assessment>(&self.default_model)
            .preamble(preamble)
            .max_tokens(STRUCTURE_MAX_TOKENS)
            .retries(2)
            .build();
        let user_msg = format!(
            "Based on our conversation, generate the assessment structure (domains and practices only, no questions).\n\n\
            ## Conversation Summary\n{}\n\n\
            ## Additional Context\n{}",
            chat_history, context
        );
        extractor
            .extract(user_msg)
            .await
            .map_err(|e| AppError::Ai(e.to_string()))
    }

    /// Generate questions for a specific practice via a rig
    /// `Extractor<QuestionList>`. When `target_count` is set, wraps the
    /// extractor in a count-validation retry loop that feeds a reminder back
    /// into chat history each attempt. Gives up after
    /// `question_gen_max_retries` with an `AppError::Ai`.
    pub async fn generate_questions_for_practice(
        &self,
        practice: &Practice,
        additional_context: Option<&str>,
        target_count: Option<u8>,
    ) -> Result<Vec<Question>, AppError> {
        let preamble = include_str!("../prompts/generate_questions.md");
        let extractor = self
            .client
            .extractor::<QuestionList>(&self.default_model)
            .preamble(preamble)
            .max_tokens(COMPLETION_MAX_TOKENS)
            .retries(2)
            .build();

        let initial_user_msg =
            build_questions_user_message(practice, additional_context, target_count);
        let mut history: Vec<Message> = Vec::new();

        for attempt in 0..=self.question_gen_max_retries {
            let result = if history.is_empty() {
                extractor.extract(initial_user_msg.clone()).await
            } else {
                extractor
                    .extract_with_chat_history(initial_user_msg.clone(), history.clone())
                    .await
            };
            let QuestionList { questions } = result.map_err(|e| AppError::Ai(e.to_string()))?;

            match target_count {
                None => return Ok(questions),
                Some(n) if questions.len() == n as usize => return Ok(questions),
                Some(n) => {
                    let actual = questions.len();
                    tracing::warn!(
                        "generate_questions_for_practice: attempt {} returned {} of {} requested (practice '{}')",
                        attempt + 1,
                        actual,
                        n,
                        practice.name
                    );
                    if attempt < self.question_gen_max_retries {
                        history.push(Message::user(build_retry_reminder(actual, n)));
                    }
                }
            }
        }
        Err(AppError::Ai(format!(
            "generate_questions_for_practice: exhausted {} retries without hitting target_count for practice '{}'",
            self.question_gen_max_retries, practice.name
        )))
    }

    /// Regenerate a single question via the LLM, guided by SME feedback. The
    /// returned `Question` has a fresh UUID; the caller is expected to
    /// overwrite it with the existing question's id so transcript links
    /// survive.
    pub async fn regenerate_question(
        &self,
        practice: &Practice,
        existing: &Question,
        feedback: &str,
    ) -> Result<Question, AppError> {
        let preamble = include_str!("../prompts/regenerate_question.md");
        let extractor = self
            .client
            .extractor::<Question>(&self.default_model)
            .preamble(preamble)
            .max_tokens(COMPLETION_MAX_TOKENS)
            .retries(2)
            .build();
        let existing_yaml = serde_yaml::to_string(existing).map_err(|e| {
            AppError::Internal(format!("serialize existing question for regenerate: {e}"))
        })?;
        let user_msg = format!(
            "Rewrite the following question for the practice below.\n\n\
             ## Practice\n\
             Name: {}\n\
             Context: {}\n\
             Value: {}\n\
             Risk: {}\n\n\
             ## Existing Question\n```yaml\n{}```\n\n\
             ## Feedback\n{}",
            practice.name, practice.context, practice.value, practice.risk, existing_yaml, feedback
        );
        extractor
            .extract(user_msg)
            .await
            .map_err(|e| AppError::Ai(e.to_string()))
    }
}

/// Build the initial user message sent to the question-generation sub-LLM.
/// Appends an `## Exact Count` section when `target_count` is set so the
/// sub-LLM sees a hard constraint in addition to the preamble's advisory range.
fn build_questions_user_message(
    practice: &Practice,
    additional_context: Option<&str>,
    target_count: Option<u8>,
) -> String {
    let mut msg = format!(
        "Generate questions for this practice:\n\n\
        ## Practice\n\
        Name: {}\n\
        Context: {}\n\
        Value: {}\n\
        Risk: {}",
        practice.name, practice.context, practice.value, practice.risk,
    );
    if let Some(ctx) = additional_context {
        msg.push_str(&format!("\n\n## Additional Context\n{}", ctx));
    }
    if let Some(n) = target_count {
        msg.push_str(&format!(
            "\n\n## Exact Count\nGenerate EXACTLY {n} questions for this practice. Do not exceed {n}. Do not return fewer than {n}."
        ));
    }
    msg
}

/// Build the retry-reminder message pushed into chat history when the sub-LLM
/// returns the wrong number of questions.
fn build_retry_reminder(actual: usize, target: u8) -> String {
    format!(
        "Your previous response returned {actual} questions, but the exact count is {target}. Regenerate the complete set, honoring the count exactly."
    )
}

/// Concatenate all `AssistantContent::Text` blocks across every assistant
/// message in the turn log. This is needed because rig's `PromptResponse.output`
/// only reflects the last turn's text — when a turn mixes text with tool calls,
/// the loop advances and the intermediate text would otherwise be discarded.
fn accumulate_assistant_text(messages: Option<&[Message]>) -> String {
    let Some(msgs) = messages else {
        return String::new();
    };
    let mut chunks: Vec<String> = Vec::new();
    for msg in msgs {
        if let Message::Assistant { content, .. } = msg {
            let texts: Vec<String> = content
                .iter()
                .filter_map(|c| match c {
                    AssistantContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect();
            if !texts.is_empty() {
                chunks.push(texts.join("\n"));
            }
        }
    }
    chunks.join("\n\n")
}

/// Rewrite assistant prose so domain/practice mentions become markdown links
/// targeting the preview pane (`#domain-<uuid>` / `#practice-<uuid>`).
///
/// Preserves any existing markdown links (so we don't wrap text that's already
/// a hyperlink). Matches whole words only and favors longer names first, so
/// "Flow-Based Status Communication" beats a shorter substring match.
pub fn linkify_entities(content: &str, assessment: &Assessment) -> String {
    if content.is_empty() {
        return content.to_string();
    }

    // Collect (anchor-href-suffix, name), longest name first to avoid partial matches.
    // The anchor is pre-rendered so the typed-ID's Display is the single source of truth.
    let mut entities: Vec<(String, &str)> = Vec::new();
    for domain in &assessment.domains {
        entities.push((format!("domain-{}", domain.id), domain.name.as_str()));
        for practice in &domain.practices {
            entities.push((format!("practice-{}", practice.id), practice.name.as_str()));
        }
    }
    entities.sort_by_key(|(_, name)| std::cmp::Reverse(name.len()));
    if entities.is_empty() {
        return content.to_string();
    }

    // Split the input into (plain text) / (existing markdown link) segments so
    // we never double-wrap inside `[...](...)`.
    let link_re = regex::Regex::new(r"\[[^\]]*\]\([^)]*\)").expect("static regex");
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for mat in link_re.find_iter(content) {
        out.push_str(&linkify_plain_segment(
            &content[cursor..mat.start()],
            &entities,
        ));
        out.push_str(mat.as_str());
        cursor = mat.end();
    }
    out.push_str(&linkify_plain_segment(&content[cursor..], &entities));
    out
}

fn linkify_plain_segment(segment: &str, entities: &[(String, &str)]) -> String {
    let mut out = segment.to_string();
    for (anchor, name) in entities {
        if name.is_empty() {
            continue;
        }
        let escaped = regex::escape(name);
        let Ok(re) = regex::Regex::new(&format!(r"\b{}\b", escaped)) else {
            continue;
        };
        // Replace each match individually, preserving the exact matched casing.
        out = re
            .replace_all(&out, |caps: &regex::Captures<'_>| {
                format!("[{}](#{})", &caps[0], anchor)
            })
            .into_owned();
    }
    out
}

/// Build a lightweight structure summary for injection into system prompts.
/// When the project carries an aggregate question budget, a `## Question Budget`
/// section is appended so the orchestrator sees remaining capacity each turn.
pub fn assessment_structure_summary(
    assessment: &Assessment,
    project: &amaker_core::models::Project,
) -> String {
    let mut summary = format!(
        "# {}\n{}\n\n## Domains and Practices\n",
        assessment.name, assessment.description
    );

    for domain in &assessment.domains {
        summary.push_str(&format!("\n### {}\n", domain.name));
        for practice in &domain.practices {
            let has_questions = if practice.questions.is_empty() {
                "❌"
            } else {
                "✓"
            };
            summary.push_str(&format!(
                "- {} {} (id: `{}`)\n",
                has_questions, practice.name, practice.id
            ));
        }
    }

    if let (Some(min), Some(max)) = (project.question_budget_min, project.question_budget_max) {
        let total = assessment.question_count() as u16;
        let remaining_capacity = max.saturating_sub(total);
        let needed_for_min = min.saturating_sub(total);
        summary.push_str(&format!(
            "\n## Question Budget\n\n\
            Committed: {min}-{max} questions total\n\
            Generated so far: {total}\n\
            Remaining capacity: up to {remaining_capacity} more\
            {}\n",
            if needed_for_min > 0 {
                format!(" (you need at least {needed_for_min} more to satisfy the minimum)")
            } else {
                String::new()
            }
        ));
    }

    summary
}

//! Headless authoring: brief → assessment, no tool calls.
//!
//! The `author` pipeline drives the same prompt/parse helpers the web flow
//! uses, but as plain fenced-YAML completions — never tool calls — so it
//! stays viable on small local models (see ADR-0006). Every generation step
//! is schema-validated and retried with corrective feedback up to
//! [`MAX_GENERATION_ATTEMPTS`] times; generation runs at the provider's
//! deterministic settings, so retries only help because the failing output's
//! error is fed back into the next attempt.

use std::cell::RefCell;

use futures::stream::{self, StreamExt, TryStreamExt};

use crate::error::AppError;
use crate::models::Assessment;
use crate::models::assessment::{Practice, Question};
use crate::services::export::{DataFormat, ExportService};
use crate::services::generation;
use crate::services::provider::LlmProvider;
use crate::services::quality;
use crate::services::yaml::YamlService;

/// Maximum attempts per generation step (initial try + corrective retries).
pub const MAX_GENERATION_ATTEMPTS: usize = 3;

/// Hard ceiling on `--jobs`: each concurrent lane multiplies the ollama
/// server's KV-cache footprint at `num_ctx=8192` (see `ollama::NUM_CTX`), so
/// unbounded parallelism would trade a silent OOM/eviction for the speedup.
pub const MAX_JOBS: usize = 8;

/// Token budget for the scoping-summary turn (plain prose, no YAML).
const SUMMARY_MAX_TOKENS: u32 = 1024;

/// Progress events emitted while authoring, for CLI reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// The scoping-summary turn is starting.
    Summary,
    /// A structure-generation attempt is starting (1-based).
    Structure { attempt: usize },
    /// The structure parsed and validated.
    StructureDone {
        name: String,
        domains: usize,
        practices: usize,
    },
    /// A question-generation attempt for one practice is starting.
    Questions {
        practice: String,
        index: usize,
        total: usize,
        attempt: usize,
    },
    /// Questions for one practice parsed and validated.
    QuestionsDone { practice: String, count: usize },
    /// Duplicate practices were mechanically dropped after corrective
    /// retries failed to produce unique names (the bounded fallback).
    DuplicatePracticesDropped { dropped: Vec<String> },
}

/// Supporting `--context` material for one authoring run: the concatenated
/// document text plus the tokens that must never leak into authored output.
///
/// Context is *background signal* about the organization being assessed —
/// run-1 proved that injecting it verbatim makes a small model treat the
/// artifact itself as the subject (questions citing pulse-report.json's
/// JSON shape), so the text is framed accordingly in every prompt and the
/// banned tokens are mechanically checked on every generation step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthorContext {
    /// Concatenated document text, one `### File: <name>` section per doc.
    pub text: String,
    /// Tokens that must not appear in authored output: each document's
    /// file name plus its data-shape JSON keys
    /// (see [`quality::forbidden_context_tokens`]).
    pub forbidden_tokens: Vec<String>,
}

impl AuthorContext {
    /// Build from `(file name, content)` documents; `None` when empty.
    pub fn from_documents(docs: &[(String, String)]) -> Option<Self> {
        if docs.is_empty() {
            return None;
        }
        let mut text = String::new();
        let mut forbidden_tokens: Vec<String> = Vec::new();
        for (name, content) in docs {
            text.push_str(&format!("### File: {name}\n{content}\n\n"));
            for token in quality::forbidden_context_tokens(name, content) {
                if !forbidden_tokens.contains(&token) {
                    forbidden_tokens.push(token);
                }
            }
        }
        Some(Self {
            text: text.trim_end().to_string(),
            forbidden_tokens,
        })
    }

    /// The context under its background-signal framing header — exactly how
    /// it enters every prompt (authoring pipeline and web flow alike).
    pub fn framed(&self) -> String {
        format!("{CONTEXT_AS_BACKGROUND}\n\n{}", self.text)
    }
}

/// Build the [`AuthorContext`] for a project's uploaded documents — the web
/// flow's equivalent of the CLI's `--context` files. Documents whose text
/// was extracted at upload time (text/markdown; see `handlers::upload`)
/// contribute their content and their banned leakage tokens; binary uploads
/// contribute nothing. `None` when no uploaded text exists.
pub async fn project_context(
    storage: &crate::services::storage::StorageService,
    project_id: crate::models::ProjectId,
) -> Result<Option<AuthorContext>, AppError> {
    let documents = storage.load_documents(project_id).await?;
    let docs: Vec<(String, String)> = documents
        .into_iter()
        .filter_map(|d| d.extracted_text.map(|text| (d.filename, text)))
        .collect();
    Ok(AuthorContext::from_documents(&docs))
}

/// Author a complete assessment from a brief, headlessly.
///
/// Runs scoping summary → structure → questions per practice, validates the
/// assembled document against the JSON Schema, and returns the assessment
/// together with its final YAML (ids included). `context` carries the
/// `--context` documents, framed into every generation prompt as background
/// signal with a do-not-cite instruction and mechanically leak-checked.
///
/// `jobs` bounds how many per-practice question generations run
/// concurrently (clamped to `1..=`[`MAX_JOBS`]). `1` is the conservative
/// default — byte-for-byte the previous serial behavior. Higher values only
/// pay off when the ollama server actually serves that many requests in
/// parallel (`OLLAMA_NUM_PARALLEL`), and each lane multiplies the server's
/// KV-cache memory at `num_ctx=8192`. Assembly order is deterministic
/// regardless of completion order: results are joined back to practices in
/// structure order, and each practice keeps its own isolated bounded
/// corrective-retry loop.
pub async fn author_assessment(
    llm: &dyn LlmProvider,
    brief: &str,
    context: Option<&AuthorContext>,
    jobs: usize,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(Assessment, String), AppError> {
    let jobs = jobs.clamp(1, MAX_JOBS);
    on_progress(Progress::Summary);
    let summary = scoping_summary(llm, brief, context).await?;

    // Structure: bounded attempts; each retry feeds the previous parse or
    // validation error back into the prompt (generation is deterministic on
    // ollama, so a blind retry would fail identically). A structure that is
    // sound except for duplicate practice names is kept aside: if no attempt
    // produces unique names, the duplicates are dropped mechanically (with a
    // warning) instead of failing the run — corrective retry first,
    // mechanical normalization as the bounded fallback.
    let mut assessment = {
        let mut feedback: Option<String> = None;
        let mut last_err: Option<AppError> = None;
        let mut parsed: Option<Assessment> = None;
        let mut duplicated: Option<Assessment> = None;
        for attempt in 1..=MAX_GENERATION_ATTEMPTS {
            on_progress(Progress::Structure { attempt });
            let assembled = structure_context(context, feedback.as_deref());
            let response = generation::generate_structure(llm, &assembled, &summary).await?;
            match parse_structure_response(&response) {
                Ok(structure) => {
                    let duplicates = quality::duplicate_practice_names(&structure);
                    let leaks = match context {
                        Some(ctx) => {
                            quality::leaky_assessment_fields(&structure, &ctx.forbidden_tokens)
                        }
                        None => Vec::new(),
                    };
                    if duplicates.is_empty() && leaks.is_empty() {
                        parsed = Some(structure);
                        break;
                    }
                    let mut problems = Vec::new();
                    if !duplicates.is_empty() {
                        problems.push(format!(
                            "duplicate practice names — every practice name \
                             must be unique across ALL domains: {}. Rename or \
                             merge the duplicated practices",
                            duplicates.join("; ")
                        ));
                    }
                    if !leaks.is_empty() {
                        problems.push(format!(
                            "background context cited as subject matter — \
                             never mention the context documents, their \
                             filenames, or their JSON keys: {}. Describe the \
                             organization's practices instead",
                            leaks.join("; ")
                        ));
                    }
                    let err = AppError::ParseError(problems.join(". "));
                    feedback = Some(retry_feedback(&err));
                    last_err = Some(err);
                    // Only a leak-free structure may serve as the mechanical
                    // dedupe fallback — a leak cannot be normalized away.
                    if leaks.is_empty() {
                        duplicated = Some(structure);
                    }
                }
                Err(err) => {
                    feedback = Some(retry_feedback(&err));
                    last_err = Some(err);
                }
            }
        }
        match (parsed, duplicated) {
            (Some(structure), _) => structure,
            (None, Some(mut structure)) => {
                let dropped = quality::drop_duplicate_practices(&mut structure);
                on_progress(Progress::DuplicatePracticesDropped { dropped });
                structure
            }
            (None, None) => {
                return Err(AppError::ParseError(format!(
                    "structure generation failed after {MAX_GENERATION_ATTEMPTS} attempts; \
                     last error: {}",
                    last_err.expect("exhausted attempts always leave an error")
                )));
            }
        }
    };
    on_progress(Progress::StructureDone {
        name: assessment.name.clone(),
        domains: assessment.domain_count(),
        practices: assessment.practice_count(),
    });

    // Questions: one bounded corrective-retry loop per practice, dispatched
    // over `jobs` concurrent lanes. The practices are snapshotted first and
    // the generated lists are written back in structure order, so assembly
    // is deterministic no matter which lane finishes first. The progress
    // callback is shared through a RefCell: every lane is polled on this
    // same task (`buffered`, no spawn), so borrows never overlap.
    let total = assessment.practice_count();
    let snapshots: Vec<Practice> = assessment
        .domains
        .iter()
        .flat_map(|d| d.practices.iter().cloned())
        .collect();
    let progress = RefCell::new(on_progress);
    let generated: Vec<Vec<Question>> =
        stream::iter(snapshots.iter().enumerate().map(|(i, practice)| {
            practice_questions(llm, context, practice, i + 1, total, &progress)
        }))
        .buffered(jobs)
        .try_collect()
        .await?;

    let mut generated = generated.into_iter();
    for domain in &mut assessment.domains {
        for practice in &mut domain.practices {
            practice.questions = generated
                .next()
                .expect("one generated list per snapshotted practice");
        }
    }

    // Final gate: the assembled document (ids included) must pass the same
    // JSON Schema validation `validate` applies to any assessment file.
    let yaml = assessment
        .to_yaml()
        .map_err(|e| AppError::Internal(format!("failed to serialize assessment: {e}")))?;
    ExportService::validate_and_parse(&yaml, DataFormat::Yaml)?;
    Ok((assessment, yaml))
}

/// One practice's question generation: the bounded corrective-retry loop,
/// with feedback scoped to this practice only (a lane's failure never bleeds
/// into another practice's prompt). Progress events are emitted through the
/// shared callback; under concurrency they interleave between practices but
/// stay ordered within one practice.
async fn practice_questions(
    llm: &dyn LlmProvider,
    context: Option<&AuthorContext>,
    practice: &Practice,
    index: usize,
    total: usize,
    progress: &RefCell<&mut dyn FnMut(Progress)>,
) -> Result<Vec<Question>, AppError> {
    let mut feedback: Option<String> = None;
    let mut last_err: Option<AppError> = None;
    for attempt in 1..=MAX_GENERATION_ATTEMPTS {
        (progress.borrow_mut())(Progress::Questions {
            practice: practice.name.clone(),
            index,
            total,
            attempt,
        });
        let result = generation::generate_questions_for_practice(
            llm,
            practice,
            questions_context(context, feedback.as_deref()).as_deref(),
        )
        .await;
        match result {
            Ok(generated) if generated.is_empty() => {
                let err =
                    AppError::ParseError("the model returned an empty questions list".to_string());
                feedback = Some(retry_feedback(&err));
                last_err = Some(err);
            }
            Ok(generated) => {
                let mut problems = Vec::new();
                let degenerate = quality::degenerate_question_fields(&generated);
                if !degenerate.is_empty() {
                    problems.push(format!(
                        "degenerate questions — placeholder stand-ins \
                         instead of real checks: {}. Replace every \
                         placeholder with a real yes/no question",
                        degenerate.join("; ")
                    ));
                }
                let leaks = match context {
                    Some(ctx) => quality::leaky_question_fields(&generated, &ctx.forbidden_tokens),
                    None => Vec::new(),
                };
                if !leaks.is_empty() {
                    problems.push(format!(
                        "questions cite the background context documents \
                         — never mention their filenames or JSON keys: \
                         {}. Ask about the practice itself",
                        leaks.join("; ")
                    ));
                }
                if problems.is_empty() {
                    (progress.borrow_mut())(Progress::QuestionsDone {
                        practice: practice.name.clone(),
                        count: generated.len(),
                    });
                    return Ok(generated);
                }
                let err = AppError::ParseError(problems.join(". "));
                feedback = Some(retry_feedback(&err));
                last_err = Some(err);
            }
            // Parse failures are retried with feedback; provider errors
            // (network, backend) propagate immediately.
            Err(err @ AppError::ParseError(_)) => {
                feedback = Some(retry_feedback(&err));
                last_err = Some(err);
            }
            Err(err) => return Err(err),
        }
    }
    Err(AppError::ParseError(format!(
        "question generation for practice '{}' failed after \
         {MAX_GENERATION_ATTEMPTS} attempts; last error: {}",
        practice.name,
        last_err.expect("exhausted attempts always leave an error")
    )))
}

/// The trailing instruction that keeps small models from dropping the
/// `questions: []` arrays the schema requires on every practice; positioned
/// LAST in the structure context for recency (ADR-0004). `pub(crate)` so the
/// web flow's structure generation (`services::tools`) carries the same
/// nudge — the B5 rehearsal proved the web path fails schema validation
/// identically without it.
pub(crate) const QUESTIONS_EMPTY_NUDGE: &str = "IMPORTANT: every practice object must \
include the literal line `questions: []` after its risk field.";

/// How `--context` documents are framed wherever they enter a prompt:
/// background signal about the organization being assessed, never subject
/// matter. Run-1 injected the text verbatim and the model authored
/// questions about the artifact's JSON shape ("Check the
/// 'pulse-report.json' file under 'per_tenant'..."), so the framing names
/// the failure mode explicitly — and [`quality::leaky_assessment_fields`]
/// enforces it mechanically, because prompt phrasing alone is not a
/// contract.
const CONTEXT_AS_BACKGROUND: &str = "## Background Signal (context, not subject)\n\
The documents below are background about the organization being assessed. \
Use them ONLY to steer emphasis and priorities. The assessment's subject is \
the organization and its practices — NOT these documents: never cite or \
mention the documents, their filenames, their JSON keys or field names, or \
their data structures anywhere in the assessment you write.";

/// Context text under its background-signal framing header.
fn framed_context(context: &AuthorContext) -> String {
    context.framed()
}

/// Turn the brief (plus optional supporting context) into a scoping summary.
async fn scoping_summary(
    llm: &dyn LlmProvider,
    brief: &str,
    context: Option<&AuthorContext>,
) -> Result<String, AppError> {
    let system = include_str!("../prompts/author_scoping.md");
    let user = match context {
        Some(ctx) => format!("## Brief\n{brief}\n\n{}", framed_context(ctx)),
        None => format!("## Brief\n{brief}"),
    };
    let response = llm
        .chat(
            system,
            vec![("user".to_string(), user)],
            vec![],
            SUMMARY_MAX_TOKENS,
            None,
        )
        .await?;
    Ok(response.text)
}

/// Corrective feedback injected into the next attempt's prompt after a
/// parse or validation failure.
fn retry_feedback(error: &AppError) -> String {
    format!(
        "## Previous attempt failed\n\
         Your previous output could not be used: {error}\n\
         Fix the problem and output the complete corrected YAML in one \
         fenced ```yaml block."
    )
}

/// Assemble the structure-generation context: supporting context first,
/// corrective feedback (on retries) next, the questions nudge always last.
fn structure_context(context: Option<&AuthorContext>, feedback: Option<&str>) -> String {
    let mut ctx = String::new();
    if let Some(c) = context {
        ctx.push_str(&framed_context(c));
        ctx.push_str("\n\n");
    }
    if let Some(feedback) = feedback {
        ctx.push_str(feedback);
        ctx.push_str("\n\n");
    }
    ctx.push_str(QUESTIONS_EMPTY_NUDGE);
    ctx
}

/// Assemble the per-practice questions context (None when there is nothing
/// to add, so the prompt omits its Additional Context section).
fn questions_context(context: Option<&AuthorContext>, feedback: Option<&str>) -> Option<String> {
    match (context.map(framed_context), feedback) {
        (None, None) => None,
        (Some(extra), None) => Some(extra),
        (None, Some(feedback)) => Some(feedback.to_string()),
        (Some(extra), Some(feedback)) => Some(format!("{extra}\n\n{feedback}")),
    }
}

/// Extract, parse, and sanity-check a structure response: it must contain a
/// fenced YAML block that schema-validates and has at least one domain, each
/// with at least one practice.
fn parse_structure_response(response: &str) -> Result<Assessment, AppError> {
    let yaml = generation::extract_fenced_yaml(response).ok_or_else(|| {
        AppError::ParseError(format!(
            "no fenced YAML block in structure response; the model returned: {}",
            generation::response_preview(response)
        ))
    })?;
    let assessment = YamlService::parse_assessment(&yaml)?;
    if assessment.domains.is_empty() {
        return Err(AppError::ParseError(
            "generated structure has no domains".to_string(),
        ));
    }
    if let Some(empty) = assessment.domains.iter().find(|d| d.practices.is_empty()) {
        return Err(AppError::ParseError(format!(
            "generated domain '{}' has no practices",
            empty.name
        )));
    }
    // Degeneracy gate (iteration-1 learnings): a schema-valid structure
    // whose load-bearing fields echo the prompt scaffold ("Assessment
    // Name", ellipsis fields, ...) is not content. Failing here feeds the
    // findings back through the bounded corrective-retry loop.
    let degenerate = quality::degenerate_fields(&assessment);
    if !degenerate.is_empty() {
        return Err(AppError::ParseError(format!(
            "degenerate structure — load-bearing fields echo the prompt \
             scaffold instead of describing the assessed subject: {}. \
             Replace every placeholder with real content.",
            degenerate.join("; ")
        )));
    }
    Ok(assessment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::export::{DataFormat, ExportService};
    use crate::services::provider::ChatResponse;
    use crate::services::provider::fake::FakeProvider;

    /// A structure response: prose around a fenced YAML block, one domain
    /// with two practices, empty questions arrays (as the prompt demands).
    const STRUCTURE_RESPONSE: &str = "Here is the structure you asked for:\n\
```yaml
name: \"Engineering Maturity\"
description: \"How mature the team's engineering practices are\"
goal: \"Find the gaps that matter\"
domains:
  - name: \"Delivery\"
    context: \"How code reaches production\"
    value: \"Predictable releases\"
    risk: \"Outages and slow delivery\"
    practices:
      - name: \"Release Management\"
        context: \"How releases are planned and shipped\"
        value: \"Lower change failure rate\"
        risk: \"Ad-hoc releases cause outages\"
        questions: []
      - name: \"Continuous Integration\"
        context: \"How changes are merged and verified\"
        value: \"Fast feedback\"
        risk: \"Broken main branch\"
        questions: []
```\nLet me know if you want changes.";

    const QUESTIONS_RESPONSE_1: &str = "```yaml
questions:
  - text: \"Is there a documented release process?\"
    polarity: positive
  - text: \"Do releases require manual database edits?\"
    polarity: negative
```";

    const QUESTIONS_RESPONSE_2: &str = "```yaml
questions:
  - text: \"Does every merge run the full test suite?\"
    polarity: positive
```";

    /// Schema-valid structure whose load-bearing fields echo the prompt
    /// scaffold — the shape run-1 actually produced (an assessment literally
    /// named "Assessment Name").
    const DEGENERATE_STRUCTURE_RESPONSE: &str = "```yaml
name: \"Assessment Name\"
description: \"What this assessment evaluates\"
goal: \"Intended outcome\"
domains:
  - name: \"Delivery\"
    context: \"How code reaches production\"
    value: \"Predictable releases\"
    risk: \"Outages and slow delivery\"
    practices:
      - name: \"Release Management\"
        context: \"How releases are planned and shipped\"
        value: \"Lower change failure rate\"
        risk: \"Ad-hoc releases cause outages\"
        questions: []
      - name: \"Continuous Integration\"
        context: \"How changes are merged and verified\"
        value: \"Fast feedback\"
        risk: \"Broken main branch\"
        questions: []
```";

    #[tokio::test]
    async fn placeholder_echo_structure_triggers_corrective_retry_not_a_hard_fail() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(DEGENERATE_STRUCTURE_RESPONSE),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.name, "Engineering Maturity");
        let calls = fake.calls();
        assert_eq!(calls.len(), 5, "one extra structure attempt, no hard fail");

        // The retry prompt names the echoed placeholder so the model can fix
        // it, and the questions nudge still comes last (recency, ADR-0004).
        let retry_msg = &calls[2].messages[0].1;
        assert!(retry_msg.contains("Previous attempt failed"));
        assert!(
            retry_msg.contains("Assessment Name"),
            "feedback must name the echoed placeholder: {retry_msg}"
        );
        assert!(
            retry_msg.rfind("questions: []").unwrap()
                > retry_msg.find("Previous attempt failed").unwrap(),
            "nudge must stay last on degeneracy retries"
        );
    }

    /// Schema-valid, non-degenerate structure with the run-1 dedupe wart:
    /// "Learning from Failure" authored into BOTH Testing and Operations.
    const DUPLICATE_PRACTICE_STRUCTURE_RESPONSE: &str = "```yaml
name: \"Engineering Maturity\"
description: \"How mature the team's engineering practices are\"
goal: \"Find the gaps that matter\"
domains:
  - name: \"Testing\"
    context: \"How the team verifies its software\"
    value: \"Confidence in changes\"
    risk: \"Defects reach production\"
    practices:
      - name: \"Learning from Failure\"
        context: \"How test failures feed back into the process\"
        value: \"Fewer repeat defects\"
        risk: \"The same failures recur\"
        questions: []
      - name: \"Test Automation\"
        context: \"How tests run without human effort\"
        value: \"Fast feedback\"
        risk: \"Manual bottlenecks\"
        questions: []
  - name: \"Operations\"
    context: \"How the team runs its software\"
    value: \"Reliable service\"
    risk: \"Outages\"
    practices:
      - name: \"Learning from Failure\"
        context: \"How incidents feed back into the process\"
        value: \"Fewer repeat incidents\"
        risk: \"The same outages recur\"
        questions: []
```";

    #[tokio::test]
    async fn duplicate_practice_structure_triggers_retry_then_succeeds() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(DUPLICATE_PRACTICE_STRUCTURE_RESPONSE),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.practice_count(), 2);
        let calls = fake.calls();
        assert_eq!(calls.len(), 5, "one extra structure attempt");

        // The feedback names the duplicated practice AND both domains, so
        // the model knows exactly what to differentiate.
        let retry_msg = &calls[2].messages[0].1;
        assert!(retry_msg.contains("Previous attempt failed"));
        assert!(retry_msg.contains("Learning from Failure"), "{retry_msg}");
        assert!(
            retry_msg.contains("Testing") && retry_msg.contains("Operations"),
            "feedback must locate the duplicate: {retry_msg}"
        );
    }

    #[tokio::test]
    async fn duplicate_practices_fall_back_to_mechanical_drop_after_bounded_attempts() {
        // Every attempt keeps the duplicate: the bounded fallback drops the
        // later occurrence (and the emptied Operations domain) with a
        // warning instead of failing the run — mirroring adroit import's
        // dedupe guard on the other side of the seam.
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(DUPLICATE_PRACTICE_STRUCTURE_RESPONSE),
            FakeProvider::text(DUPLICATE_PRACTICE_STRUCTURE_RESPONSE),
            FakeProvider::text(DUPLICATE_PRACTICE_STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);
        let mut events = Vec::new();

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut |e| events.push(e))
            .await
            .unwrap();

        assert_eq!(assessment.practice_count(), 2, "first occurrences survive");
        assert_eq!(assessment.domain_count(), 1, "emptied domain dropped");
        let names: Vec<_> = assessment.domains[0]
            .practices
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["Learning from Failure", "Test Automation"]);

        // 1 summary + 3 structure attempts + questions for the 2 survivors.
        assert_eq!(fake.calls().len(), 6);

        // The drop is surfaced as a progress warning, not silent.
        let dropped: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Progress::DuplicatePracticesDropped { dropped } => Some(dropped.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(dropped.len(), 1, "exactly one drop event: {events:?}");
        assert!(dropped[0][0].contains("Learning from Failure"));
        assert!(dropped[0][0].contains("Operations"));
    }

    #[tokio::test]
    async fn placeholder_echo_structure_fails_after_bounded_attempts() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(DEGENERATE_STRUCTURE_RESPONSE),
            FakeProvider::text(DEGENERATE_STRUCTURE_RESPONSE),
            FakeProvider::text(DEGENERATE_STRUCTURE_RESPONSE),
        ]);

        let err = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap_err();

        assert_eq!(fake.calls().len(), 1 + MAX_GENERATION_ATTEMPTS);
        let msg = err.to_string();
        assert!(msg.contains("after 3 attempts"), "{msg}");
        assert!(
            msg.contains("Assessment Name"),
            "error must name the surviving placeholder echo: {msg}"
        );
    }

    fn noop() -> impl FnMut(Progress) {
        |_| {}
    }

    /// Context with text only (no banned tokens) — for prompt-wiring tests.
    fn ctx(text: &str) -> AuthorContext {
        AuthorContext {
            text: text.to_string(),
            forbidden_tokens: Vec::new(),
        }
    }

    #[tokio::test]
    async fn happy_path_runs_summary_structure_then_questions_tool_free() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("a scoping summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _yaml) = author_assessment(
            &fake,
            "THE-BRIEF",
            Some(&ctx("EXTRA-CONTEXT")),
            1,
            &mut noop(),
        )
        .await
        .unwrap();

        assert_eq!(assessment.name, "Engineering Maturity");
        assert_eq!(assessment.domain_count(), 1);
        assert_eq!(assessment.practice_count(), 2);
        assert_eq!(assessment.question_count(), 3);
        let practices: Vec<_> = assessment
            .domains
            .iter()
            .flat_map(|d| &d.practices)
            .collect();
        assert_eq!(practices[0].questions.len(), 2);
        assert_eq!(
            practices[0].questions[0].text,
            "Is there a documented release process?"
        );
        assert_eq!(practices[1].questions.len(), 1);
        assert_eq!(
            practices[1].questions[0].text,
            "Does every merge run the full test suite?"
        );

        let calls = fake.calls();
        assert_eq!(calls.len(), 4, "summary + structure + 2 practices");
        for call in &calls {
            assert!(call.tool_names.is_empty(), "authoring must be tool-free");
            assert_eq!(call.model_override, None);
        }

        // Call 0: scoping summary over the brief and the extra context.
        assert_eq!(
            calls[0].system,
            include_str!("../prompts/author_scoping.md")
        );
        assert_eq!(calls[0].max_tokens, SUMMARY_MAX_TOKENS);
        let summary_msg = &calls[0].messages[0].1;
        assert!(summary_msg.contains("THE-BRIEF"));
        assert!(summary_msg.contains("EXTRA-CONTEXT"));

        // Call 1: structure generation seeded with the summary; the
        // `questions: []` nudge sits AFTER the extra context (recency
        // positioning for small models — ADR-0004).
        assert_eq!(
            calls[1].system,
            include_str!("../prompts/generate_structure.md")
        );
        let structure_msg = &calls[1].messages[0].1;
        assert!(structure_msg.contains("## Conversation Summary\na scoping summary"));
        assert!(structure_msg.contains("EXTRA-CONTEXT"));
        assert!(structure_msg.contains("questions: []"));
        assert!(
            structure_msg.rfind("questions: []").unwrap()
                > structure_msg.find("EXTRA-CONTEXT").unwrap(),
            "the questions nudge must come after the context (recency)"
        );

        // Calls 2-3: questions per practice, in structure order, with the
        // extra context wired through.
        assert_eq!(
            calls[2].system,
            include_str!("../prompts/generate_questions.md")
        );
        assert!(calls[2].messages[0].1.contains("Name: Release Management"));
        assert!(calls[2].messages[0].1.contains("EXTRA-CONTEXT"));
        assert!(
            calls[3].messages[0]
                .1
                .contains("Name: Continuous Integration")
        );
        assert!(calls[3].messages[0].1.contains("EXTRA-CONTEXT"));
    }

    #[tokio::test]
    async fn returned_yaml_is_schema_valid_and_carries_the_assessment_ids() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, yaml) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        let parsed = ExportService::validate_and_parse(&yaml, DataFormat::Yaml).unwrap();
        assert_eq!(
            parsed.id, assessment.id,
            "ids must be persisted, not re-minted"
        );
        assert_eq!(parsed.domains[0].id, assessment.domains[0].id);
        assert_eq!(parsed.question_count(), 3);
    }

    #[tokio::test]
    async fn structure_retry_feeds_back_the_error_then_succeeds() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text("sorry, here is prose with no YAML at all"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) =
            author_assessment(&fake, "brief", Some(&ctx("EXTRA-CONTEXT")), 1, &mut noop())
                .await
                .unwrap();

        assert_eq!(assessment.question_count(), 3);
        let calls = fake.calls();
        assert_eq!(calls.len(), 5, "one extra structure attempt");

        // The second structure attempt carries corrective feedback with the
        // actual error, and the questions nudge still comes last.
        let retry_msg = &calls[2].messages[0].1;
        assert!(retry_msg.contains("Previous attempt failed"));
        assert!(retry_msg.contains("no fenced YAML block"));
        assert!(retry_msg.contains("EXTRA-CONTEXT"));
        assert!(
            retry_msg.rfind("questions: []").unwrap()
                > retry_msg.find("Previous attempt failed").unwrap(),
            "nudge must stay last on retries"
        );
        // The first attempt had no feedback.
        assert!(!calls[1].messages[0].1.contains("Previous attempt failed"));
    }

    #[tokio::test]
    async fn structure_fails_after_bounded_attempts() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text("junk 1"),
            FakeProvider::text("junk 2"),
            FakeProvider::text("junk 3"),
        ]);

        let err = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap_err();

        assert_eq!(fake.calls().len(), 1 + MAX_GENERATION_ATTEMPTS);
        let msg = err.to_string();
        assert!(
            msg.contains("after 3 attempts"),
            "error must state the attempt bound: {msg}"
        );
        assert!(
            msg.contains("structure"),
            "error must name the failing step: {msg}"
        );
        assert!(
            msg.contains("junk 3"),
            "error must preview the last response so the failure is \
             diagnosable: {msg}"
        );
    }

    #[tokio::test]
    async fn structure_in_a_bare_fence_is_accepted() {
        // Small local models sometimes drop the ```yaml language tag; the
        // schema validation downstream is the real gate, so a bare fence
        // must not fail authoring.
        let bare = STRUCTURE_RESPONSE.replacen("```yaml", "```", 1);
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(&bare),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.practice_count(), 2);
        assert_eq!(fake.calls().len(), 4, "no retry needed");
    }

    #[tokio::test]
    async fn schema_invalid_structure_is_retried() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            // Fenced YAML, but missing required fields — fails the schema.
            FakeProvider::text("```yaml\nname: only-a-name\n```"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.practice_count(), 2);
        assert_eq!(fake.calls().len(), 5);
    }

    #[tokio::test]
    async fn structure_with_no_domains_is_retried() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            // Schema-valid but useless: zero domains.
            FakeProvider::text("```yaml\nname: x\ndescription: d\ngoal: g\ndomains: []\n```"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.domain_count(), 1);
        let calls = fake.calls();
        assert_eq!(calls.len(), 5);
        assert!(calls[2].messages[0].1.contains("no domains"));
    }

    #[tokio::test]
    async fn questions_retry_feeds_back_error_then_succeeds() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text("no yaml in this one"),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.question_count(), 3);
        let calls = fake.calls();
        assert_eq!(calls.len(), 5, "one extra questions attempt");
        // The retry for the first practice carries the feedback.
        let retry_msg = &calls[3].messages[0].1;
        assert!(retry_msg.contains("Name: Release Management"));
        assert!(retry_msg.contains("Previous attempt failed"));
        assert!(retry_msg.contains("No YAML block found"));
    }

    #[tokio::test]
    async fn questions_fail_after_bounded_attempts_naming_the_practice() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text("junk 1"),
            FakeProvider::text("junk 2"),
            FakeProvider::text("junk 3"),
        ]);

        let err = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap_err();

        assert_eq!(fake.calls().len(), 2 + MAX_GENERATION_ATTEMPTS);
        let msg = err.to_string();
        assert!(
            msg.contains("Release Management"),
            "error must name the practice: {msg}"
        );
        assert!(msg.contains("after 3 attempts"), "{msg}");
    }

    #[tokio::test]
    async fn empty_questions_list_is_rejected_and_retried() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text("```yaml\nquestions: []\n```"),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.question_count(), 3);
        assert_eq!(fake.calls().len(), 5);
    }

    #[tokio::test]
    async fn progress_reports_steps_and_retry_attempts() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text("junk once"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);
        let mut events = Vec::new();

        author_assessment(&fake, "brief", None, 1, &mut |e| events.push(e))
            .await
            .unwrap();

        assert_eq!(
            events,
            vec![
                Progress::Summary,
                Progress::Structure { attempt: 1 },
                Progress::Structure { attempt: 2 },
                Progress::StructureDone {
                    name: "Engineering Maturity".to_string(),
                    domains: 1,
                    practices: 2,
                },
                Progress::Questions {
                    practice: "Release Management".to_string(),
                    index: 1,
                    total: 2,
                    attempt: 1,
                },
                Progress::QuestionsDone {
                    practice: "Release Management".to_string(),
                    count: 2,
                },
                Progress::Questions {
                    practice: "Continuous Integration".to_string(),
                    index: 2,
                    total: 2,
                    attempt: 1,
                },
                Progress::QuestionsDone {
                    practice: "Continuous Integration".to_string(),
                    count: 1,
                },
            ]
        );
    }

    // ===== context-as-background framing + leakage gate (M3) =====

    /// Question response with the run-1 leak shape: guidance citing the
    /// context artifact's filename and JSON keys as the subject.
    const LEAKY_QUESTIONS_RESPONSE: &str = "```yaml
questions:
  - text: \"Are all flows succeeding?\"
    polarity: positive
    guidance: \"Check the 'pulse-report.json' file under 'per_tenant' to see the total_flows\"
```";

    fn pulse_tokens() -> AuthorContext {
        AuthorContext {
            text: "### File: pulse-report.json\n{\"per_tenant\": []}".to_string(),
            forbidden_tokens: vec![
                "pulse-report.json".to_string(),
                "per_tenant".to_string(),
                "total_flows".to_string(),
            ],
        }
    }

    #[tokio::test]
    async fn context_is_framed_as_background_with_do_not_cite_in_every_prompt() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        author_assessment(
            &fake,
            "THE-BRIEF",
            Some(&ctx("THE-CONTEXT-TEXT")),
            1,
            &mut noop(),
        )
        .await
        .unwrap();

        let calls = fake.calls();
        // Every prompt that carries context frames it as background signal
        // with an explicit do-not-cite-artifact instruction.
        for (i, call) in calls.iter().enumerate() {
            let msg = &call.messages[0].1;
            assert!(
                msg.contains("Background Signal"),
                "call {i} must frame context as background: {msg}"
            );
            assert!(
                msg.contains("never cite"),
                "call {i} must carry the do-not-cite instruction: {msg}"
            );
            assert!(msg.contains("THE-CONTEXT-TEXT"), "call {i}: {msg}");
        }
        // The framing precedes the context text (it introduces it)...
        let scoping = &calls[0].messages[0].1;
        assert!(
            scoping.find("Background Signal").unwrap() < scoping.find("THE-CONTEXT-TEXT").unwrap()
        );
        // ...and the structure prompt's questions nudge STAYS last
        // (recency, ADR-0004) — after framing, context, everything.
        let structure = &calls[1].messages[0].1;
        assert!(
            structure.rfind("questions: []").unwrap() > structure.find("THE-CONTEXT-TEXT").unwrap()
        );
        assert!(
            structure.rfind("questions: []").unwrap()
                > structure.find("Background Signal").unwrap()
        );
    }

    #[tokio::test]
    async fn leaky_questions_trigger_retry_with_feedback_then_succeed() {
        let context = pulse_tokens();
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(LEAKY_QUESTIONS_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, yaml) = author_assessment(&fake, "brief", Some(&context), 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.question_count(), 3);
        // No banned token survives anywhere in the final document.
        for token in &context.forbidden_tokens {
            assert!(
                !yaml.to_lowercase().contains(&token.to_lowercase()),
                "token {token:?} leaked into the output"
            );
        }

        let calls = fake.calls();
        assert_eq!(calls.len(), 5, "one extra questions attempt");
        let retry_msg = &calls[3].messages[0].1;
        assert!(retry_msg.contains("Previous attempt failed"));
        assert!(
            retry_msg.contains("per_tenant") && retry_msg.contains("pulse-report.json"),
            "feedback must name the leaked tokens: {retry_msg}"
        );
    }

    #[tokio::test]
    async fn leaky_questions_fail_after_bounded_attempts_naming_practice_and_token() {
        let context = pulse_tokens();
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(LEAKY_QUESTIONS_RESPONSE),
            FakeProvider::text(LEAKY_QUESTIONS_RESPONSE),
            FakeProvider::text(LEAKY_QUESTIONS_RESPONSE),
        ]);

        let err = author_assessment(&fake, "brief", Some(&context), 1, &mut noop())
            .await
            .unwrap_err();

        assert_eq!(fake.calls().len(), 2 + MAX_GENERATION_ATTEMPTS);
        let msg = err.to_string();
        assert!(msg.contains("Release Management"), "{msg}");
        assert!(msg.contains("after 3 attempts"), "{msg}");
        assert!(msg.contains("per_tenant"), "{msg}");
    }

    #[tokio::test]
    async fn leaky_structure_triggers_retry_then_succeeds() {
        // The structure itself cites the artifact (a practice context
        // referencing the report's shape) — same gate, structure step.
        let leaky_structure = STRUCTURE_RESPONSE.replace(
            "How changes are merged and verified",
            "Tracks the per_tenant rollup from pulse-report.json",
        );
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(&leaky_structure),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) =
            author_assessment(&fake, "brief", Some(&pulse_tokens()), 1, &mut noop())
                .await
                .unwrap();

        assert_eq!(assessment.practice_count(), 2);
        let calls = fake.calls();
        assert_eq!(calls.len(), 5, "one extra structure attempt");
        let retry_msg = &calls[2].messages[0].1;
        assert!(retry_msg.contains("Previous attempt failed"));
        assert!(
            retry_msg.contains("per_tenant"),
            "feedback must name the leaked token: {retry_msg}"
        );
    }

    #[test]
    fn author_context_from_documents_concatenates_and_derives_tokens() {
        let docs = vec![
            (
                "pulse-report.json".to_string(),
                r#"{"per_tenant": [], "seed": 1}"#.to_string(),
            ),
            (
                "notes.md".to_string(),
                "remember the per_tenant gap".to_string(),
            ),
        ];

        let context = AuthorContext::from_documents(&docs).unwrap();

        assert!(context.text.contains("### File: pulse-report.json"));
        assert!(context.text.contains("### File: notes.md"));
        assert!(context.text.contains("remember the per_tenant gap"));
        assert_eq!(
            context.forbidden_tokens,
            vec![
                "pulse-report.json".to_string(),
                "per_tenant".to_string(),
                "notes.md".to_string(),
            ],
            "filename + snake_case JSON keys, deduped, non-JSON bans only its name"
        );

        assert_eq!(AuthorContext::from_documents(&[]), None);
    }

    // ===== `--jobs` concurrent question lanes (iteration-3 B2) =====

    /// A provider that PROVES concurrent dispatch: question calls block on a
    /// barrier sized to the lane count, so a serial implementation deadlocks
    /// (caught by the test timeout). Responses are keyed by the practice
    /// named in the prompt — not by call order — so the assembly assertion
    /// is meaningful regardless of which lane finishes first; the
    /// structure-order-first practice is deliberately made to finish LAST.
    struct LaneProbe {
        /// Scripted summary + structure responses (the serial prefix).
        setup: std::sync::Mutex<std::collections::VecDeque<ChatResponse>>,
        /// Lanes-in-flight rendezvous; `None` disables the rendezvous.
        barrier: Option<tokio::sync::Barrier>,
        in_flight: std::sync::atomic::AtomicUsize,
        max_in_flight: std::sync::atomic::AtomicUsize,
        /// One junk first answer for Release Management (retry isolation).
        release_junk_once: std::sync::atomic::AtomicBool,
        /// First user message of every call, in arrival order.
        seen: std::sync::Mutex<Vec<String>>,
    }

    impl LaneProbe {
        fn new(barrier_size: Option<usize>, release_junk_once: bool) -> Self {
            Self {
                setup: std::sync::Mutex::new(
                    vec![
                        FakeProvider::text("summary"),
                        FakeProvider::text(STRUCTURE_RESPONSE),
                    ]
                    .into(),
                ),
                barrier: barrier_size.map(tokio::sync::Barrier::new),
                in_flight: std::sync::atomic::AtomicUsize::new(0),
                max_in_flight: std::sync::atomic::AtomicUsize::new(0),
                release_junk_once: std::sync::atomic::AtomicBool::new(release_junk_once),
                seen: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn max_concurrency(&self) -> usize {
            self.max_in_flight.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn seen(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for LaneProbe {
        async fn chat(
            &self,
            _system: &str,
            messages: Vec<(String, String)>,
            _tools: Vec<crate::services::tools::ToolDef>,
            _max_tokens: u32,
            _model_override: Option<&str>,
        ) -> Result<ChatResponse, AppError> {
            use std::sync::atomic::Ordering;
            let msg = messages[0].1.clone();
            self.seen.lock().unwrap().push(msg.clone());
            if !msg.contains("Generate questions for this practice") {
                return Ok(self.setup.lock().unwrap().pop_front().expect("setup"));
            }
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            if let Some(barrier) = &self.barrier {
                // Every lane must be in flight before any may answer.
                barrier.wait().await;
            }
            let response = if msg.contains("Name: Release Management") {
                if self.release_junk_once.swap(false, Ordering::SeqCst) {
                    self.in_flight.fetch_sub(1, Ordering::SeqCst);
                    return Ok(FakeProvider::text("junk with no YAML"));
                }
                // First in structure order, last to finish.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                FakeProvider::text(QUESTIONS_RESPONSE_1)
            } else {
                FakeProvider::text(QUESTIONS_RESPONSE_2)
            };
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(response)
        }
    }

    #[tokio::test]
    async fn jobs_2_dispatches_lanes_concurrently_with_deterministic_assembly() {
        let probe = LaneProbe::new(Some(2), false);

        // A serial implementation never has 2 lanes at the barrier at once
        // and would hang forever; the timeout converts that into a failure.
        let (assessment, _) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            author_assessment(&probe, "brief", None, 2, &mut noop()),
        )
        .await
        .expect("serial dispatch would deadlock the 2-lane barrier")
        .unwrap();

        assert_eq!(probe.max_concurrency(), 2, "both lanes in flight at once");

        // Deterministic assembly: structure order wins even though the
        // first practice's lane finished last.
        let practices: Vec<_> = assessment
            .domains
            .iter()
            .flat_map(|d| &d.practices)
            .collect();
        assert_eq!(practices[0].name, "Release Management");
        assert_eq!(
            practices[0].questions[0].text,
            "Is there a documented release process?"
        );
        assert_eq!(practices[1].name, "Continuous Integration");
        assert_eq!(
            practices[1].questions[0].text,
            "Does every merge run the full test suite?"
        );
    }

    #[tokio::test]
    async fn concurrent_lane_retry_feedback_stays_isolated_per_practice() {
        let probe = LaneProbe::new(None, true);

        let (assessment, _) = author_assessment(&probe, "brief", None, 2, &mut noop())
            .await
            .unwrap();

        // The flaky lane recovered via its own corrective retry...
        let practices: Vec<_> = assessment
            .domains
            .iter()
            .flat_map(|d| &d.practices)
            .collect();
        assert_eq!(
            practices[0].questions[0].text,
            "Is there a documented release process?"
        );
        assert_eq!(assessment.question_count(), 3);

        // ...and its feedback never reached the other practice's prompts.
        let seen = probe.seen();
        let release_retry = seen
            .iter()
            .filter(|m| m.contains("Name: Release Management"))
            .nth(1)
            .expect("a second Release Management attempt");
        assert!(release_retry.contains("Previous attempt failed"));
        for ci_prompt in seen
            .iter()
            .filter(|m| m.contains("Name: Continuous Integration"))
        {
            assert!(
                !ci_prompt.contains("Previous attempt failed"),
                "lane feedback bled across practices: {ci_prompt}"
            );
        }
    }

    #[tokio::test]
    async fn jobs_is_clamped_to_the_bounded_range() {
        // 0 behaves as serial; an absurd value clamps to MAX_JOBS — both
        // produce the normal happy-path result.
        for jobs in [0, usize::MAX] {
            let fake = FakeProvider::new(vec![
                FakeProvider::text("summary"),
                FakeProvider::text(STRUCTURE_RESPONSE),
                FakeProvider::text(QUESTIONS_RESPONSE_1),
                FakeProvider::text(QUESTIONS_RESPONSE_2),
            ]);
            let (assessment, _) = author_assessment(&fake, "brief", None, jobs, &mut noop())
                .await
                .unwrap();
            assert_eq!(assessment.question_count(), 3, "jobs={jobs}");
        }
    }

    #[tokio::test]
    async fn works_without_extra_context() {
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text(STRUCTURE_RESPONSE),
            FakeProvider::text(QUESTIONS_RESPONSE_1),
            FakeProvider::text(QUESTIONS_RESPONSE_2),
        ]);

        let (assessment, _) = author_assessment(&fake, "just a brief", None, 1, &mut noop())
            .await
            .unwrap();

        assert_eq!(assessment.question_count(), 3);
        // The structure prompt still ends with the questions nudge.
        let calls = fake.calls();
        assert!(calls[1].messages[0].1.contains("questions: []"));
    }
}

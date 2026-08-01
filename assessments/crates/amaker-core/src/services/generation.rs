//! Prompt assembly and AI-backed generation helpers.
//!
//! Everything here is provider-agnostic: prompts, token budgets, and response
//! parsing live on this side of the [`LlmProvider`] seam, so they behave the
//! same no matter which backend is configured.

use crate::error::AppError;
use crate::models::ProjectPhase;
use crate::models::assessment::{Assessment, Practice, Question};
use crate::services::provider::LlmProvider;

/// Token budget for structure generation (larger documents).
const STRUCTURE_MAX_TOKENS: u32 = 8192;

/// Token budget for per-practice question generation.
const QUESTIONS_MAX_TOKENS: u32 = 4096;

/// Get the system prompt for a given project phase.
/// Optionally includes an assessment summary for phases that need context.
pub fn system_prompt_for_phase(phase: ProjectPhase, assessment_summary: Option<&str>) -> String {
    let base = include_str!("../prompts/system_base.md");
    let phase_context = match phase {
        ProjectPhase::Scoping => include_str!("../prompts/phase_scoping.md"),
        ProjectPhase::Structuring => include_str!("../prompts/phase_structuring.md"),
        ProjectPhase::Questions => include_str!("../prompts/phase_questions.md"),
        ProjectPhase::Refining => include_str!("../prompts/phase_refining.md"),
        ProjectPhase::Complete => include_str!("../prompts/phase_complete.md"),
    };

    match assessment_summary {
        Some(summary) => format!(
            "{}\n\n{}\n\n## Current Assessment Structure\n{}",
            base, phase_context, summary
        ),
        None => format!("{}\n\n{}", base, phase_context),
    }
}

/// Generate assessment structure (domains and practices only, no questions).
/// Uses a moderate token limit (8192) for structure generation.
pub async fn generate_structure(
    llm: &dyn LlmProvider,
    context: &str,
    chat_history: &str,
) -> Result<String, AppError> {
    let system = include_str!("../prompts/generate_structure.md");
    let user_message = format!(
        "Based on our conversation, generate the assessment structure (domains and practices only, no questions).\n\n\
        ## Conversation Summary\n{}\n\n\
        ## Additional Context\n{}",
        chat_history, context
    );

    let response = llm
        .chat(
            system,
            vec![("user".to_string(), user_message)],
            vec![],
            STRUCTURE_MAX_TOKENS,
            None,
        )
        .await?;
    Ok(response.text)
}

/// Generate questions for a specific practice.
/// Returns a vector of Questions parsed from the YAML response.
pub async fn generate_questions_for_practice(
    llm: &dyn LlmProvider,
    practice: &Practice,
    additional_context: Option<&str>,
) -> Result<Vec<Question>, AppError> {
    let system = include_str!("../prompts/generate_questions.md");
    let user_message = format!(
        "Generate questions for this practice:\n\n\
        ## Practice\n\
        Name: {}\n\
        Context: {}\n\
        Value: {}\n\
        Risk: {}\n\n\
        {}",
        practice.name,
        practice.context,
        practice.value,
        practice.risk,
        additional_context
            .map(|c| format!("## Additional Context\n{}", c))
            .unwrap_or_default()
    );

    let response = llm
        .chat(
            system,
            vec![("user".to_string(), user_message)],
            vec![],
            QUESTIONS_MAX_TOKENS,
            None,
        )
        .await?;

    // Parse questions from YAML response
    parse_questions_from_response(&response.text)
}

/// Generate a lightweight structure summary for injection into system prompts.
/// Shows domain/practice names with question status indicators.
pub fn assessment_structure_summary(assessment: &Assessment) -> String {
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
    summary
}

/// Extract the first fenced YAML block from a model response.
///
/// Prefers a fence tagged `yaml`/`yml`; falls back to the first bare
/// (untagged) fence — small local models sometimes drop the language tag,
/// and every caller schema-validates the extracted content anyway, so the
/// fence tag is not the real gate. As a last resort an *unterminated*
/// tagged fence is accepted (content from the opener to the end, trailing
/// partial backticks trimmed): a window-clipped generation opens ```yaml
/// and never closes it, and feeding the actual content to the schema gate
/// produces a diagnosable error instead of "no fenced YAML block".
pub(crate) fn extract_fenced_yaml(response: &str) -> Option<String> {
    let tagged = regex::Regex::new(r"```ya?ml\s*\n([\s\S]*?)\n```").expect("static regex is valid");
    let bare = regex::Regex::new(r"```\s*\n([\s\S]*?)\n```").expect("static regex is valid");
    if let Some(found) = tagged
        .captures(response)
        .or_else(|| bare.captures(response))
        .and_then(|cap| cap.get(1))
    {
        return Some(found.as_str().to_string());
    }
    let open_tagged = regex::Regex::new(r"```ya?ml\s*\n([\s\S]+)$").expect("static regex is valid");
    open_tagged
        .captures(response)
        .and_then(|cap| cap.get(1))
        .map(|m| {
            m.as_str()
                .trim_end()
                .trim_end_matches('`')
                .trim_end()
                .to_string()
        })
}

/// A short single-line preview of a model response, for parse errors:
/// "no YAML block" alone is undiagnosable after the fact — the error must
/// show what the model actually returned.
pub(crate) fn response_preview(response: &str) -> String {
    const MAX_CHARS: usize = 160;
    let flat = response.trim().replace('\n', " ");
    if flat.chars().count() <= MAX_CHARS {
        flat
    } else {
        let truncated: String = flat.chars().take(MAX_CHARS).collect();
        format!("{truncated}…")
    }
}

/// Parse questions from a YAML response.
fn parse_questions_from_response(response: &str) -> Result<Vec<Question>, AppError> {
    let yaml_content = extract_fenced_yaml(response).ok_or_else(|| {
        AppError::ParseError(format!(
            "No YAML block found in question generation response; the model \
             returned: {}",
            response_preview(response)
        ))
    })?;
    let yaml_content = yaml_content.as_str();

    // Try to parse as a questions wrapper first
    #[derive(serde::Deserialize)]
    struct QuestionsWrapper {
        questions: Vec<Question>,
    }

    if let Ok(wrapper) = serde_yaml::from_str::<QuestionsWrapper>(yaml_content) {
        return Ok(wrapper.questions);
    }

    // Try parsing as a direct array
    if let Ok(questions) = serde_yaml::from_str::<Vec<Question>>(yaml_content) {
        return Ok(questions);
    }

    let mut end = yaml_content.len().min(200);
    while end > 0 && !yaml_content.is_char_boundary(end) {
        end -= 1;
    }
    Err(AppError::ParseError(format!(
        "Failed to parse questions from YAML: {}",
        &yaml_content[..end]
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::provider::fake::FakeProvider;

    #[test]
    fn system_prompt_combines_base_and_phase_context() {
        let prompt = system_prompt_for_phase(ProjectPhase::Scoping, None);
        assert!(prompt.starts_with(include_str!("../prompts/system_base.md")));
        assert!(prompt.contains(include_str!("../prompts/phase_scoping.md")));
        assert!(!prompt.contains("## Current Assessment Structure"));
    }

    #[test]
    fn system_prompt_appends_assessment_summary_when_present() {
        let prompt = system_prompt_for_phase(ProjectPhase::Questions, Some("THE-SUMMARY"));
        assert!(prompt.contains(include_str!("../prompts/phase_questions.md")));
        assert!(prompt.contains("## Current Assessment Structure\nTHE-SUMMARY"));
    }

    #[tokio::test]
    async fn generate_structure_assembles_prompt_and_returns_text() {
        let fake = FakeProvider::new(vec![FakeProvider::text("the structure")]);

        let out = generate_structure(&fake, "extra context", "the chat history")
            .await
            .unwrap();

        assert_eq!(out, "the structure");
        let calls = fake.calls();
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(
            call.system,
            include_str!("../prompts/generate_structure.md")
        );
        assert_eq!(call.max_tokens, 8192);
        assert!(call.tool_names.is_empty(), "generation is tool-free");
        assert_eq!(call.model_override, None);
        assert_eq!(call.messages.len(), 1);
        let (role, content) = &call.messages[0];
        assert_eq!(role, "user");
        assert!(content.contains("## Conversation Summary\nthe chat history"));
        assert!(content.contains("## Additional Context\nextra context"));
    }

    #[tokio::test]
    async fn generate_questions_assembles_practice_prompt_and_parses_yaml() {
        let yaml_response = "Here you go:\n```yaml\nquestions:\n  - text: \"Is there a documented release process?\"\n    polarity: positive\n  - text: \"Do releases require manual database edits?\"\n    polarity: negative\n```";
        let fake = FakeProvider::new(vec![FakeProvider::text(yaml_response)]);
        let practice = Practice::new(
            "Release Management".to_string(),
            "How software reaches production".to_string(),
            "Predictable delivery".to_string(),
            "Outages from ad-hoc releases".to_string(),
        );

        let questions = generate_questions_for_practice(&fake, &practice, Some("focus on CI"))
            .await
            .unwrap();

        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].text, "Is there a documented release process?");
        let calls = fake.calls();
        assert_eq!(calls.len(), 1);
        let call = &calls[0];
        assert_eq!(
            call.system,
            include_str!("../prompts/generate_questions.md")
        );
        assert_eq!(call.max_tokens, 4096);
        assert!(call.tool_names.is_empty());
        let content = &call.messages[0].1;
        assert!(content.contains("Name: Release Management"));
        assert!(content.contains("Context: How software reaches production"));
        assert!(content.contains("Value: Predictable delivery"));
        assert!(content.contains("Risk: Outages from ad-hoc releases"));
        assert!(content.contains("## Additional Context\nfocus on CI"));
    }

    #[tokio::test]
    async fn generate_questions_parses_direct_yaml_array() {
        let yaml_response =
            "```yaml\n- text: \"Is the build reproducible?\"\n  polarity: positive\n```";
        let fake = FakeProvider::new(vec![FakeProvider::text(yaml_response)]);
        let practice = Practice::new(
            "Builds".to_string(),
            "ctx".to_string(),
            "value".to_string(),
            "risk".to_string(),
        );

        let questions = generate_questions_for_practice(&fake, &practice, None)
            .await
            .unwrap();

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].text, "Is the build reproducible?");
    }

    #[tokio::test]
    async fn generate_questions_errors_when_no_yaml_block() {
        let fake = FakeProvider::new(vec![FakeProvider::text("sorry, no YAML here")]);
        let practice = Practice::new(
            "Builds".to_string(),
            "ctx".to_string(),
            "value".to_string(),
            "risk".to_string(),
        );

        let err = generate_questions_for_practice(&fake, &practice, None)
            .await
            .unwrap_err();

        assert!(matches!(err, AppError::ParseError(_)));
        // The error must show what the model actually returned — a bare
        // "no YAML block" is undiagnosable after the fact.
        assert!(
            err.to_string().contains("sorry, no YAML here"),
            "error must preview the response: {err}"
        );
    }

    #[test]
    fn extract_fenced_yaml_accepts_yaml_yml_and_bare_fences() {
        for fence in ["yaml", "yml", ""] {
            let response = format!("prose before\n```{fence}\nname: x\n```\nprose after");
            assert_eq!(
                extract_fenced_yaml(&response).as_deref(),
                Some("name: x"),
                "fence tag {fence:?} must be accepted"
            );
        }
    }

    #[test]
    fn extract_fenced_yaml_prefers_a_tagged_fence_over_an_earlier_bare_one() {
        let response = "```\nnot: the-yaml\n```\nand then\n```yaml\nname: x\n```";
        assert_eq!(extract_fenced_yaml(response).as_deref(), Some("name: x"));
    }

    #[test]
    fn extract_fenced_yaml_still_rejects_fence_free_responses() {
        assert_eq!(extract_fenced_yaml("name: x\ndomains: []"), None);
        assert_eq!(extract_fenced_yaml("plain prose"), None);
    }

    /// A truncated response opens a ```yaml fence and never closes it (a
    /// window-clipped generation, observed live). The content after the
    /// opener must still be extracted — schema validation downstream is the
    /// real gate, and a YAML parse error on the actual content is far more
    /// actionable retry feedback than "no fenced YAML block".
    #[test]
    fn extract_fenced_yaml_accepts_an_unterminated_tagged_fence() {
        let truncated = "Here you go:\n```yaml\nname: \"X\"\ndescription: \"d\"";
        assert_eq!(
            extract_fenced_yaml(truncated).as_deref(),
            Some("name: \"X\"\ndescription: \"d\"")
        );
        // Trailing partial fence backticks are trimmed.
        let partial_close = "```yaml\nname: x\n``";
        assert_eq!(
            extract_fenced_yaml(partial_close).as_deref(),
            Some("name: x")
        );
    }

    #[test]
    fn extract_fenced_yaml_prefers_a_closed_fence_over_an_unterminated_one() {
        let response = "```yaml\nname: closed\n```\nand then\n```yaml\nname: open";
        assert_eq!(
            extract_fenced_yaml(response).as_deref(),
            Some("name: closed")
        );
    }

    /// An unterminated BARE fence stays rejected — without the yaml tag it
    /// is indistinguishable from prose formatting.
    #[test]
    fn extract_fenced_yaml_rejects_an_unterminated_bare_fence() {
        assert_eq!(extract_fenced_yaml("```\nname: x"), None);
    }

    #[test]
    fn response_preview_flattens_and_truncates() {
        assert_eq!(response_preview("short\nresponse"), "short response");
        let long = "x".repeat(500);
        let preview = response_preview(&long);
        assert!(preview.chars().count() < 200);
        assert!(preview.ends_with('…'));
    }
}

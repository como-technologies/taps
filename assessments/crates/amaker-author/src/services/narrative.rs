//! LLM-driven narrative report.
//!
//! Takes the deterministic analysis outputs (scorecard, gap inventory,
//! roadmap) plus the assessment metadata, composes a compact structured
//! briefing, and asks the LLM to write a grounded Markdown findings
//! report. Callers use [`regenerate`] to force a fresh LLM call; cached
//! reads go through `StorageService::load_analysis_cache` directly.

use std::fmt::Write;

use serde::Serialize;

use amaker_core::AppError;
use amaker_core::models::{Assessment, AssessmentResponse, ProjectId};
use amaker_core::services::StorageService;
use amaker_core::services::analysis::gaps::{Gap, GapInventory};
use amaker_core::services::analysis::roadmap::Roadmap;
use amaker_core::services::analysis::scorecard::{DomainSummary, Scorecard};

use crate::services::AiService;

/// System prompt — loaded at compile time from the prompts directory.
const SYSTEM_PROMPT: &str = include_str!("../prompts/generate_narrative.md");

/// Token cap for the narrative generation call.
const NARRATIVE_MAX_TOKENS: u64 = 4096;

/// Top-N priority-ordered gaps to include in the briefing. Keeps the
/// prompt compact on large assessments without losing the lede.
const MAX_BRIEFED_GAPS: usize = 20;

/// Result of a narrative generation: the Markdown body + whether it
/// came from the cache (always `false` for now; kept as a struct so
/// future staleness-aware callers can return cached reads without
/// changing the signature).
#[derive(Debug, Clone, Serialize)]
pub struct NarrativeReport {
    pub markdown: String,
    pub cached: bool,
}

/// Force a fresh LLM call, bypassing the cache. Writes the new report
/// to the cache so later cached reads are free.
///
/// The wide arg list is deliberate — this function sits at the seam
/// between the handler and the analysis layer, so it stays an honest
/// list of dependencies rather than being hidden behind a builder or
/// a grab-bag context struct.
#[allow(clippy::too_many_arguments)]
pub async fn regenerate(
    storage: &StorageService,
    ai: &AiService,
    project_id: ProjectId,
    assessment: &Assessment,
    response: &AssessmentResponse,
    scorecard: &Scorecard,
    gaps: &GapInventory,
    roadmap: &Roadmap,
    model_override: Option<&str>,
) -> Result<NarrativeReport, AppError> {
    let markdown = generate(
        ai,
        assessment,
        response,
        scorecard,
        gaps,
        roadmap,
        model_override,
    )
    .await?;
    storage.save_report_cache(project_id, &markdown).await?;
    Ok(NarrativeReport {
        markdown,
        cached: false,
    })
}

/// Run the LLM call and return the raw Markdown output.
async fn generate(
    ai: &AiService,
    assessment: &Assessment,
    response: &AssessmentResponse,
    scorecard: &Scorecard,
    gaps: &GapInventory,
    roadmap: &Roadmap,
    model_override: Option<&str>,
) -> Result<String, AppError> {
    let briefing = build_briefing(assessment, response, scorecard, gaps, roadmap);
    tracing::info!(
        "Generating narrative: briefing={} chars, top_gaps={}",
        briefing.len(),
        gaps.gaps.len().min(MAX_BRIEFED_GAPS),
    );
    ai.completion_only(
        SYSTEM_PROMPT,
        briefing,
        NARRATIVE_MAX_TOKENS,
        model_override,
    )
    .await
}

/// Compose the structured briefing text the LLM reads. The shape is
/// Markdown because the model is comfortable with it; the content is
/// a condensed view of the deterministic layer's outputs.
fn build_briefing(
    assessment: &Assessment,
    response: &AssessmentResponse,
    scorecard: &Scorecard,
    gaps: &GapInventory,
    roadmap: &Roadmap,
) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# Assessment briefing");
    let _ = writeln!(out, "\n**Name**: {}", assessment.name);
    let _ = writeln!(out, "\n**Description**: {}", assessment.description);
    let _ = writeln!(out, "\n**Goal**: {}", assessment.goal);

    let completeness = if response.answers.is_empty() {
        "not started".to_string()
    } else if scorecard.overall.unanswered > 0 {
        format!(
            "incomplete — {} of {} questions still unanswered",
            scorecard.overall.unanswered, scorecard.overall.total
        )
    } else {
        "complete".to_string()
    };
    let _ = writeln!(out, "\n**Response status**: {}", completeness);

    let _ = writeln!(out, "\n## Scorecard\n");
    let _ = writeln!(
        out,
        "**Overall**: {} passed, {} failed, {} unknown, {} unanswered ({} total) — {}%",
        scorecard.overall.passed,
        scorecard.overall.failed,
        scorecard.overall.unknown,
        scorecard.overall.unanswered,
        scorecard.overall.total,
        scorecard.overall.percent,
    );
    for domain in &scorecard.domains {
        write_domain_scorecard(&mut out, domain);
    }

    let _ = writeln!(out, "\n## Top prioritized gaps\n");
    if gaps.gaps.is_empty() {
        let _ = writeln!(out, "_None — every question resolves to a pass._");
    } else {
        // Use the roadmap's priority ordering directly so the narrative
        // inherits the same ordering the UI renders.
        for (i, entry) in roadmap
            .priority_ordered
            .iter()
            .take(MAX_BRIEFED_GAPS)
            .enumerate()
        {
            write_gap_briefing(&mut out, i + 1, &entry.gap);
        }
    }

    let _ = writeln!(out, "\n## Roadmap by role\n");
    if roadmap.by_role.is_empty() {
        let _ = writeln!(out, "_None._");
    } else {
        for (role, entries) in roadmap.by_role.iter() {
            let _ = writeln!(
                out,
                "- **{}** ({} gap{})",
                role,
                entries.len(),
                if entries.len() == 1 { "" } else { "s" }
            );
            for entry in entries.iter().take(5) {
                let _ = writeln!(
                    out,
                    "  - {} · {}",
                    entry.gap.practice_name, entry.gap.question_text
                );
            }
        }
    }

    out
}

fn write_domain_scorecard(out: &mut String, domain: &DomainSummary) {
    let _ = writeln!(
        out,
        "\n### {} — {}% ({} / {} passed)",
        domain.name, domain.summary.percent, domain.summary.passed, domain.summary.total,
    );
    for p in &domain.practices {
        let percent = if p.summary.passed + p.summary.failed > 0 {
            format!("{}%", p.summary.percent)
        } else {
            "—".to_string()
        };
        let _ = writeln!(
            out,
            "- {} — {} ({} passed, {} failed, {} unknown, {} unanswered)",
            p.name,
            percent,
            p.summary.passed,
            p.summary.failed,
            p.summary.unknown,
            p.summary.unanswered,
        );
    }
}

fn write_gap_briefing(out: &mut String, idx: usize, gap: &Gap) {
    let _ = writeln!(
        out,
        "\n**{}. {} · {}**",
        idx, gap.domain_name, gap.practice_name
    );
    let _ = writeln!(out, "- Question: {}", gap.question_text);
    let answer_desc = match gap.answer_value {
        Some(v) => format!("{:?}", v),
        None => "Unanswered".to_string(),
    };
    let _ = writeln!(
        out,
        "- Polarity: {:?}; Answer: {}",
        gap.polarity, answer_desc
    );
    if !gap.practice_risk.is_empty() {
        let _ = writeln!(out, "- Risk: {}", gap.practice_risk);
    }
    if !gap.blockers.is_empty() {
        let labels: Vec<&str> = gap.blockers.iter().map(|b| b.label.as_str()).collect();
        let _ = writeln!(out, "- Blockers: {}", labels.join(", "));
    }
    if let Some(planned) = gap.planned {
        let _ = writeln!(out, "- Planned: {}", if planned { "yes" } else { "no" });
    }
    if !gap.roles.is_empty() {
        let _ = writeln!(out, "- Roles: {}", gap.roles.join(", "));
    }
    if let Some(effort) = &gap.effort {
        let _ = writeln!(
            out,
            "- Effort: {}–{} hours",
            effort.min_hours, effort.max_hours
        );
    }
    if let Some(r) = &gap.remediation {
        let _ = writeln!(out, "- Remediation: {}", r);
    }
    if let Some(n) = &gap.notes {
        let _ = writeln!(out, "- Notes: {}", n);
    }
}

/// Names the LLM may mention without it counting as a hallucination.
///
/// Collects every domain name, practice name, and distinct role label
/// from the assessment. Used by [`extract_suspicious_names`] for
/// test-time grounding checks; may later drive a post-generation
/// validator in prod.
#[allow(dead_code)]
pub fn valid_entity_names(assessment: &Assessment) -> Vec<String> {
    let mut names = Vec::new();
    for d in &assessment.domains {
        names.push(d.name.clone());
        for p in &d.practices {
            names.push(p.name.clone());
            for q in &p.questions {
                for role in &q.roles {
                    names.push(role.clone());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Simple grounding check: extract strings that look like entity names
/// from the H2/H3 headings and verify they overlap with the
/// assessment's actual names. Any mention of a name *not* in the valid
/// set is returned as a suspicious entry.
///
/// This is intentionally loose — it catches invented practice names
/// ("Secrets Management" appearing in a report for an assessment that
/// has no such practice) without flagging every adjective the model
/// might use. False positives are tolerated; false negatives are the
/// failure mode we care about. Currently used from tests; wiring
/// into prod (as a retry loop) is future work.
#[allow(dead_code)]
pub fn extract_suspicious_names(markdown: &str, assessment: &Assessment) -> Vec<String> {
    let valid = valid_entity_names(assessment);
    let mut suspicious = Vec::new();

    for line in markdown.lines() {
        // Only look at H3 headings — "### {Role}" in the By Role
        // section is a structured promise the prompt makes, so we can
        // reliably expect it to be a known role.
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("### ") {
            let candidate = rest.trim();
            if candidate.is_empty() {
                continue;
            }
            if !valid.iter().any(|v| v == candidate) {
                suspicious.push(candidate.to_string());
            }
        }
    }

    suspicious
}

#[cfg(test)]
mod tests {
    use super::*;
    use amaker_core::models::assessment::{Domain, Practice, Question};
    use amaker_core::models::ids::RespondentId;
    use amaker_core::models::{Answer, AnswerValue, Polarity};
    use amaker_core::services::analysis::{compute_gaps, compute_roadmap, compute_scorecard};

    fn sample() -> (Assessment, AssessmentResponse) {
        let mut a = Assessment::new(
            "Test Assessment".into(),
            "A small fixture".into(),
            "Demonstrate".into(),
        );
        let mut d = Domain::new("Security".into(), "c".into(), "v".into(), "r".into());
        let mut p = Practice::new(
            "Access Control".into(),
            "Who can get in".into(),
            "Limits exposure".into(),
            "Unauthorized access".into(),
        );
        p.questions.push(Question::new("Q1 yes?".into()));
        p.questions.push({
            let mut q = Question::new("Q2 neg?".into());
            q.polarity = Polarity::Negative;
            q
        });
        d.practices.push(p);
        a.domains.push(d);

        let mut r = AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string());
        let qs = &a.domains[0].practices[0].questions;
        r.upsert_answer(qs[0].id, Answer::new(AnswerValue::No)); // Pos+No → fail
        r.upsert_answer(qs[1].id, Answer::new(AnswerValue::Yes)); // Neg+Yes → fail
        (a, r)
    }

    #[test]
    fn briefing_includes_scorecard_and_gaps() {
        let (a, r) = sample();
        let sc = compute_scorecard(&a, &r);
        let gi = compute_gaps(&a, &r);
        let rm = compute_roadmap(&gi);

        let briefing = build_briefing(&a, &r, &sc, &gi, &rm);
        assert!(briefing.contains("Test Assessment"));
        assert!(briefing.contains("## Scorecard"));
        assert!(briefing.contains("Access Control"));
        assert!(briefing.contains("Q1 yes?"));
        assert!(briefing.contains("Q2 neg?"));
        assert!(briefing.contains("Top prioritized gaps"));
    }

    #[test]
    fn valid_entity_names_includes_domain_and_practice() {
        let (a, _r) = sample();
        let names = valid_entity_names(&a);
        assert!(names.contains(&"Security".to_string()));
        assert!(names.contains(&"Access Control".to_string()));
    }

    #[test]
    fn extract_suspicious_names_catches_invented_h3() {
        let (a, _r) = sample();
        let good = "## Executive Summary\n\nblah.\n\n### Access Control\n\nblah\n";
        assert!(extract_suspicious_names(good, &a).is_empty());

        // "Supply Chain" is not in the fixture — should be flagged.
        let bad = "### Supply Chain\n\nmade up.\n";
        let sus = extract_suspicious_names(bad, &a);
        assert_eq!(sus, vec!["Supply Chain"]);
    }
}

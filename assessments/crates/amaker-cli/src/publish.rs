//! `amaker publish` — land a project's assessment and its analysis in the KB
//! as amaker-owned pages, over the transport.
//!
//! amaker owns the `assessment` and `assessment-report` classes: it ships
//! their schemas (in `assessments/schemas/`) and registers them on first
//! contact, idempotently. Pages enter through the same admission gates as
//! everything else (`wiki_ingest`), and the outcome — including how many
//! pages the index actually picked up — is surfaced, not assumed.

use amaker_core::models::{Assessment, ProjectId};
use amaker_core::services::analysis::{
    Roadmap, Scorecard, compute_gaps, compute_roadmap, compute_scorecard,
};
use amaker_core::services::{ResponseService, StorageService};
use anyhow::{Context, Result, bail};
use como_kb_client::{KbClient, KbTarget};
use serde::Serialize;

const ASSESSMENT_SCHEMA: &str = include_str!("../../../schemas/assessment.json");
const REPORT_SCHEMA: &str = include_str!("../../../schemas/assessment-report.json");

/// What `publish` did, for the caller's eyes.
#[derive(Debug, Serialize)]
pub struct PublishReport {
    /// Slug of the assessment page.
    pub assessment_page: String,
    /// Slug of the report page.
    pub report_page: String,
    /// Per-schema registration outcome (`registered` / `unchanged`).
    pub schemas: Vec<String>,
    /// The admission gate's report for the written pages.
    pub ingest: serde_json::Value,
}

/// Publish `project_id`'s current published assessment version and the
/// primary respondent's analysis. Fails visibly when the KB is absent or
/// unreachable — amaker's own storage stays the source of truth and a
/// re-publish is always safe.
pub async fn publish_cmd(
    storage: StorageService,
    project_id: ProjectId,
    wiki_flag: Option<String>,
) -> Result<PublishReport> {
    let Some(mut target) = KbTarget::discover() else {
        bail!(
            "no KB configured: set KB_URL to your appliance's MCP endpoint \
             (e.g. http://localhost:8080/mcp; `just kb-dev` serves one) and \
             optionally KB_WIKI for the target space — in the environment, \
             a .env here, or ~/.config/taps/env"
        );
    };
    if wiki_flag.is_some() {
        target.wiki = wiki_flag;
    }

    // Load everything before touching the network: project, published
    // assessment at the responded version, primary response.
    let project = storage.load_project(project_id).await?;
    let responses = ResponseService::new(storage.clone());
    let (assessment, response) = responses
        .load_primary_context(project_id)
        .await
        .context("publish needs a published assessment version (author and publish one first)")?;

    let scorecard = compute_scorecard(&assessment, &response);
    let gaps = compute_gaps(&assessment, &response);
    let roadmap = compute_roadmap(&gaps);

    let slug_base = slugify(&project.name);
    let assessment_page = format!("assessments/{slug_base}");
    let report_page = format!("assessments/{slug_base}-report");

    let kb = KbClient::connect(&target).await?;

    // First contact: our classes, our schemas. `unchanged` on every
    // re-publish; a conflict (someone else's type under our name) is fatal.
    let mut schemas = Vec::new();
    for (type_name, schema) in [
        ("assessment", ASSESSMENT_SCHEMA),
        ("assessment-report", REPORT_SCHEMA),
    ] {
        let report = kb
            .call_json(
                "wiki_schema",
                serde_json::json!({"action": "register", "type": type_name, "schema": schema}),
            )
            .await?;
        schemas.push(format!(
            "{type_name}: {}",
            report["status"].as_str().unwrap_or("?")
        ));
    }

    let generated = chrono::Utc::now().format("%Y-%m-%d").to_string();
    kb.call(
        "wiki_content_write",
        serde_json::json!({
            "uri": assessment_page,
            "content": render_assessment_page(&project.name, &assessment, &response.version, &report_page),
        }),
    )
    .await?;
    kb.call(
        "wiki_content_write",
        serde_json::json!({
            "uri": report_page,
            "content": render_report_page(
                &project.name, &assessment, &response.version, &assessment_page,
                &scorecard, &roadmap, &generated,
            ),
        }),
    )
    .await?;

    // The gates decide; their report (validated counts, indexed counts,
    // warnings) is the publish outcome.
    let ingest = kb
        .call_json("wiki_ingest", serde_json::json!({"path": "assessments"}))
        .await?;

    kb.close().await.ok();

    Ok(PublishReport {
        assessment_page,
        report_page,
        schemas,
        ingest,
    })
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = true; // suppress leading dashes
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "assessment".into()
    } else {
        out
    }
}

fn yaml_str(s: &str) -> String {
    serde_yaml::to_string(s)
        .map(|y| y.trim_end().to_string())
        .unwrap_or_else(|_| format!("\"{}\"", s.replace('"', "'")))
}

fn render_assessment_page(
    project_name: &str,
    assessment: &Assessment,
    version: &str,
    report_page: &str,
) -> String {
    let domains: Vec<String> = assessment.domains.iter().map(|d| d.name.clone()).collect();
    let questions: usize = assessment
        .domains
        .iter()
        .flat_map(|d| &d.practices)
        .map(|p| p.questions.len())
        .sum();

    let mut body = String::new();
    body.push_str(&format!(
        "{}\n\n**Goal:** {}\n\n",
        assessment.description, assessment.goal
    ));
    body.push_str("## Structure\n\n");
    for domain in &assessment.domains {
        body.push_str(&format!("### {}\n\n{}\n\n", domain.name, domain.context));
        for practice in &domain.practices {
            body.push_str(&format!(
                "- **{}** — {} ({} questions)\n",
                practice.name,
                practice.context,
                practice.questions.len()
            ));
        }
        body.push('\n');
    }
    body.push_str(&format!("Current analysis: [[{report_page}]]\n"));

    format!(
        "---\ntitle: {title}\ntype: assessment\nstatus: active\nsummary: {summary}\ngoal: {goal}\nassessment_id: \"{aid}\"\nversion: {version}\ndomains:\n{domain_list}questions: {questions}\n---\n\n{body}",
        title = yaml_str(&format!("{project_name}: {}", assessment.name)),
        summary = yaml_str(&assessment.description),
        goal = yaml_str(&assessment.goal),
        aid = assessment.id,
        version = yaml_str(version),
        domain_list = domains
            .iter()
            .map(|d| format!("  - {}\n", yaml_str(d)))
            .collect::<String>(),
    )
}

fn render_report_page(
    project_name: &str,
    assessment: &Assessment,
    version: &str,
    assessment_page: &str,
    scorecard: &Scorecard,
    roadmap: &Roadmap,
    generated: &str,
) -> String {
    let overall = &scorecard.overall;
    let definite_gaps = roadmap
        .priority_ordered
        .iter()
        .filter(|e| e.gap.is_definite())
        .count();

    let mut body = String::new();
    body.push_str(&format!(
        "Analysis of [[{assessment_page}]] (version {version}), primary respondent.\n\n"
    ));

    body.push_str("## Scorecard\n\n");
    body.push_str("| Domain | Pass % | Passed | Failed | Unknown | Unanswered |\n");
    body.push_str("|---|---|---|---|---|---|\n");
    body.push_str(&format!(
        "| **Overall** | {}{} | {} | {} | {} | {} |\n",
        overall.percent,
        if overall.has_definite_answers() {
            "%"
        } else {
            "% (no definite answers yet)"
        },
        overall.passed,
        overall.failed,
        overall.unknown,
        overall.unanswered,
    ));
    for domain in &scorecard.domains {
        body.push_str(&format!(
            "| {} | {}% | {} | {} | {} | {} |\n",
            domain.name,
            domain.summary.percent,
            domain.summary.passed,
            domain.summary.failed,
            domain.summary.unknown,
            domain.summary.unanswered,
        ));
    }
    body.push('\n');

    if definite_gaps > 0 {
        body.push_str("## Top gaps (priority order)\n\n");
        for entry in roadmap
            .priority_ordered
            .iter()
            .filter(|e| e.gap.is_definite())
            .take(10)
        {
            let gap = &entry.gap;
            body.push_str(&format!(
                "- **{} / {}**: {}\n",
                gap.domain_name, gap.practice_name, gap.question_text
            ));
            if let Some(remediation) = &gap.remediation {
                body.push_str(&format!("  - Remediation: {remediation}\n"));
            }
            if !gap.roles.is_empty() {
                body.push_str(&format!("  - Roles: {}\n", gap.roles.join(", ")));
            }
        }
        body.push('\n');
    }

    format!(
        "---\ntitle: {title}\ntype: assessment-report\nstatus: active\nassessment: {page}\nassessment_id: \"{aid}\"\nversion: {version}\ngenerated: \"{generated}\"\noverall_percent: {percent}\npassed: {passed}\nfailed: {failed}\nunknown: {unknown}\nunanswered: {unanswered}\ngaps: {gaps}\n---\n\n{body}",
        title = yaml_str(&format!("{project_name}: assessment report {generated}")),
        page = yaml_str(assessment_page),
        aid = assessment.id,
        version = yaml_str(version),
        percent = overall.percent,
        passed = overall.passed,
        failed = overall.failed,
        unknown = overall.unknown,
        unanswered = overall.unanswered,
        gaps = definite_gaps,
    )
}

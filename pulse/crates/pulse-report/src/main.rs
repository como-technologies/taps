//! pulse's KB lane: run the poll and land the result in the knowledge base.
//!
//! `pulse-report` runs the seeded dogfood survey (the same run as `just
//! dogfood`), reduces the Signal zone's k-anonymous aggregation to one row
//! per question, and writes one typed `pulse-report` page at
//! `pulse/<survey-name>` over the appliance transport — no filesystem
//! access to the wiki. The run is seeded, so re-runs write the same bytes
//! and converge instead of churning history. The `data_source` field says
//! plainly that the respondents are simulated; a real poll needs a real
//! team and is future work.

use como_kb_client::{KbClient, KbTarget};
use pulse_test_harness::simulation::{
    SimulationCluster, SimulationConfig, SimulationReport, SimulationRunner, SurveyFile,
};

/// The pulse-report schema this tool ships and registers on first contact.
const PULSE_REPORT_SCHEMA: &str = include_str!("../../../schemas/pulse-report.json");

/// The dogfood run, pinned to match `just dogfood` in `pulse/`.
const DEFAULT_SURVEY: &str = "dogfood/iteration-retro.toml";
const SEED: u64 = 42;
const EMPLOYEES: usize = 10;
const K_THRESHOLD: usize = 5;

type Error = Box<dyn std::error::Error + Send + Sync>;

// ── The report model (pure, testable) ──────────────────────────────────────

/// One question's k-anonymous aggregate for one segment.
struct QuestionRow {
    text: String,
    segment: String,
    responses: u64,
    /// Mean Scale5 score; `None` when the segment is suppressed.
    average: Option<f64>,
    suppressed: bool,
}

/// The whole poll, reduced to what the page carries.
struct PollReport {
    survey: String,
    data_source: String,
    seed: Option<u64>,
    total: u64,
    passed: u64,
    failed: u64,
    rows: Vec<QuestionRow>,
}

/// Reduce a simulation run to the poll report: one row per question and
/// segment, in survey order.
fn summarize(survey: &str, sim: &SimulationReport) -> PollReport {
    let rows = sim
        .batches
        .iter()
        .flat_map(|batch| {
            batch.aggregation.segments.iter().map(|seg| QuestionRow {
                text: batch.question_text.clone(),
                segment: seg.segment_label.0.clone(),
                responses: seg.response_count as u64,
                average: seg.average_score,
                suppressed: seg.suppressed,
            })
        })
        .collect();
    PollReport {
        survey: survey.to_string(),
        data_source: sim.data_source.to_string(),
        seed: sim.seed,
        total: sim.total_flows as u64,
        passed: sim.successful as u64,
        failed: sim.failed as u64,
        rows,
    }
}

/// Double-quote a YAML scalar.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Render the typed page: frontmatter the schema validates, then a body a
/// human reads. Deterministic — no timestamps, no run-local values.
fn render_page(report: &PollReport) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("title: \"Pulse report — {}\"\n", report.survey));
    out.push_str("type: pulse-report\n");
    out.push_str("status: active\n");
    out.push_str(&format!(
        "summary: \"{} question(s); {} of {} respondents completed the protocol.\"\n",
        report.rows.len(),
        report.passed,
        report.total
    ));
    out.push_str(&format!("survey: {}\n", quote(&report.survey)));
    out.push_str("instrument: pulse\n");
    out.push_str(&format!("data_source: {}\n", quote(&report.data_source)));
    if let Some(seed) = report.seed {
        out.push_str(&format!("seed: {seed}\n"));
    }
    out.push_str(&format!(
        "flows: {{total: {}, passed: {}, failed: {}}}\n",
        report.total, report.passed, report.failed
    ));
    if !report.rows.is_empty() {
        out.push_str("questions:\n");
        for row in &report.rows {
            out.push_str(&format!("  - text: {}\n", quote(&row.text)));
            out.push_str(&format!("    segment: {}\n", quote(&row.segment)));
            out.push_str(&format!("    responses: {}\n", row.responses));
            match row.average {
                Some(avg) => out.push_str(&format!("    average: {avg}\n")),
                None => out.push_str("    average: null\n"),
            }
            out.push_str(&format!("    suppressed: {}\n", row.suppressed));
        }
    }
    out.push_str("---\n\n");
    out.push_str(&body(report));
    out
}

/// The readable half of the page.
fn body(report: &PollReport) -> String {
    let mut out = String::new();
    out.push_str("Answers are anonymous. Blind signatures prevent anyone from linking\n");
    out.push_str("a respondent to an answer. A segment with fewer than k unique\n");
    out.push_str("respondents is suppressed and shows no average.\n\n");
    for row in &report.rows {
        if row.suppressed {
            out.push_str(&format!(
                "- suppressed — {} ({}, fewer than k unique respondents)\n",
                row.text, row.segment
            ));
        } else {
            out.push_str(&format!(
                "- {} — {} ({}, {} responses)\n",
                row.average.unwrap_or(0.0),
                row.text,
                row.segment,
                row.responses
            ));
        }
    }
    out.push_str(&format!(
        "\n{} of {} respondents completed the protocol.\n",
        report.passed, report.total
    ));
    out.push_str(&format!("Data source: {}.\n", report.data_source));
    out
}

/// The stdout briefing: scores with their questions, then the page name.
fn render_briefing(report: &PollReport, wiki: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("Pulse — {wiki}, survey {}\n\n", report.survey));
    for row in &report.rows {
        match row.average {
            Some(avg) if !row.suppressed => out.push_str(&format!(
                "  {avg:>4.1}  {} ({}, {} responses)\n",
                row.text, row.segment, row.responses
            )),
            _ => out.push_str(&format!(
                "     —  {} ({}, suppressed: fewer than k unique respondents)\n",
                row.text, row.segment
            )),
        }
    }
    out.push_str(&format!(
        "\nflows   {} of {} respondents completed the protocol\n",
        report.passed, report.total
    ));
    out.push_str(&format!("source  {}\n", report.data_source));
    out.push_str(&format!(
        "\nfull report: pulse/{} — a typed page beside the decisions it speaks to\n",
        report.survey
    ));
    out
}

// ── The transport shell ────────────────────────────────────────────────────

/// Find the survey file: `--batch-file <path>`, else the dogfood survey
/// relative to `pulse/` or the repo root.
fn survey_path() -> Result<std::path::PathBuf, Error> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--batch-file") {
        let path = args.get(i + 1).ok_or("--batch-file needs a path")?;
        return Ok(std::path::PathBuf::from(path));
    }
    for candidate in [DEFAULT_SURVEY, "pulse/dogfood/iteration-retro.toml"] {
        let path = std::path::PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(format!(
        "survey file not found: run from your taps clone's pulse/ directory, \
         or pass --batch-file <path> (default: {DEFAULT_SURVEY})"
    )
    .into())
}

/// The whole lane: run the poll, write the page, return the briefing and
/// the count of failed flows.
async fn run() -> Result<(String, u64), Error> {
    let Some(target) = KbTarget::discover() else {
        return Err("no KB configured: set KB_URL (and optionally KB_WIKI) — \
             in the environment, a .env here, or ~/.config/taps/env"
            .into());
    };
    let wiki = target.wiki.clone().unwrap_or_else(|| "default".into());

    let survey = SurveyFile::load(&survey_path()?)?;
    let tenant = survey.to_tenant_setup(EMPLOYEES, 1);
    let name = tenant.name.clone();
    let config = SimulationConfig {
        tenants: vec![tenant],
        concurrency: EMPLOYEES,
        with_analytics: true,
        k_threshold: K_THRESHOLD,
        seed: Some(SEED),
    };
    let cluster = SimulationCluster::start(&config).await;
    let runner = SimulationRunner::new(cluster, config.concurrency, config.seed);
    // Seeded run: strip wall-clock fields so the page is deterministic.
    let sim = runner.run().await.without_timings();
    let report = summarize(&name, &sim);

    let kb = KbClient::connect(&target).await?;
    kb.ensure_schema("pulse-report", PULSE_REPORT_SCHEMA)
        .await?;
    let slug = format!("pulse/{}", report.survey);
    kb.call(
        "wiki_content_write",
        serde_json::json!({"uri": slug, "content": render_page(&report)}),
    )
    .await?;
    kb.call_json("wiki_ingest", serde_json::json!({"path": "pulse"}))
        .await?;
    kb.close().await.ok();

    Ok((render_briefing(&report, &wiki), report.failed))
}

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("pulse-report {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter("pulse=warn")
        .with_writer(std::io::stderr)
        .init();
    match run().await {
        Ok((briefing, failed)) => {
            print!("{briefing}");
            if failed > 0 {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

// ── Tests (the pure half) ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PollReport {
        PollReport {
            survey: "iteration-retro".into(),
            data_source: "simulated respondents — synthetic demo data, not a real survey"
                .into(),
            seed: Some(42),
            total: 10,
            passed: 10,
            failed: 0,
            rows: vec![
                QuestionRow {
                    text: "How confident are you that this iteration's changes improved the portfolio?".into(),
                    segment: "company".into(),
                    responses: 10,
                    average: Some(3.7),
                    suppressed: false,
                },
                QuestionRow {
                    text: "How sustainable is the current iteration pace?".into(),
                    segment: "engineering".into(),
                    responses: 3,
                    average: None,
                    suppressed: true,
                },
            ],
        }
    }

    #[test]
    fn page_frontmatter_carries_the_poll() {
        let page = render_page(&sample());
        assert!(page.starts_with("---\n"));
        assert!(page.contains("type: pulse-report\n"));
        assert!(page.contains("survey: \"iteration-retro\"\n"));
        assert!(page.contains("seed: 42\n"));
        assert!(page.contains("flows: {total: 10, passed: 10, failed: 0}\n"));
        assert!(page.contains("    average: 3.7\n"));
        assert!(page.contains("    average: null\n"));
        assert!(page.contains("    suppressed: true\n"));
    }

    #[test]
    fn page_render_is_deterministic() {
        assert_eq!(render_page(&sample()), render_page(&sample()));
    }

    #[test]
    fn suppressed_segment_shows_no_average_anywhere() {
        let page = render_page(&sample());
        let briefing = render_briefing(&sample(), "walk");
        for text in [&page, &briefing] {
            let after = text.split("sustainable").nth(1).expect("row present");
            assert!(
                !after.starts_with("3."),
                "suppressed row must not leak a score"
            );
        }
        assert!(briefing.contains("suppressed: fewer than k unique respondents"));
    }

    #[test]
    fn briefing_names_the_page_and_the_source() {
        let briefing = render_briefing(&sample(), "walk");
        assert!(briefing.starts_with("Pulse — walk, survey iteration-retro\n"));
        assert!(briefing.contains("  3.7  How confident"));
        assert!(briefing.contains("flows   10 of 10 respondents completed the protocol\n"));
        assert!(briefing.contains("source  simulated respondents"));
        assert!(briefing.contains("full report: pulse/iteration-retro"));
    }

    #[test]
    fn quote_escapes_yaml_specials() {
        assert_eq!(quote(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }
}

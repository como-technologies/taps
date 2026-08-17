//! The KB lane: price decisions from door-stamped work-item pages.
//!
//! No forge, no labels, no hours. conduit's doors stamped everything this
//! report needs onto the pages (`work_ms`, `merged_at`, the friction
//! counters, approval and close records); attribution is the graph the PM
//! drew — task → story → project → `implements` → decision. tuesday reads
//! pages over the appliance transport, assembles the month, and writes one
//! deterministic `measure-report` page back through the same admission
//! gates as everything else. Two currencies, and neither is hours: machine
//! milliseconds between claim and the merge door, and human gate actions
//! (taps 116 — attention is the scarce input).
//!
//! Pages landed by pre-telemetry doors miss `merged_at`/`closed_at`; the
//! honest fallback derives the landing instant from door-witnessed numbers
//! (`claimed_at + work_ms`) rather than dropping the work from the record.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use como_kb_client::{KbClient, KbTarget};
use serde_yaml_ng::{Mapping, Value};

use crate::window::YearMonth;

/// The measure-report schema tuesday ships and registers on first contact.
const MEASURE_REPORT_SCHEMA: &str = include_str!("../../../schemas/measure-report.json");

type Error = Box<dyn std::error::Error + Send + Sync>;

// ── Page model ─────────────────────────────────────────────────────────────

/// One work-item (or decision) page, reduced to frontmatter.
#[derive(Debug, Clone)]
pub struct Page {
    pub slug: String,
    pub fm: Mapping,
}

impl Page {
    /// Parse a wiki page's frontmatter; the body is not this lane's business.
    pub fn parse(slug: &str, text: &str) -> Result<Self, Error> {
        let trimmed = text.trim_start();
        let after = trimmed
            .strip_prefix("---")
            .ok_or_else(|| format!("{slug}: no frontmatter"))?
            .trim_start_matches(['\r', '\n']);
        let close = after
            .find("\n---")
            .ok_or_else(|| format!("{slug}: unterminated frontmatter"))?;
        let fm: Mapping = serde_yaml_ng::from_str(&after[..close + 1])?;
        Ok(Page {
            slug: slug.to_string(),
            fm,
        })
    }

    fn str(&self, key: &str) -> Option<&str> {
        self.fm.get(key).and_then(Value::as_str)
    }

    fn u64(&self, key: &str) -> u64 {
        self.fm.get(key).and_then(Value::as_u64).unwrap_or(0)
    }

    fn time(&self, key: &str) -> Option<DateTime<Utc>> {
        self.str(key)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc))
    }

    /// Does `target` (an id or slug, with or without `work/`) name this page?
    fn is_target_of(&self, target: &str) -> bool {
        let t = target.trim();
        !t.is_empty()
            && (self.slug == t
                || self.slug.strip_prefix("work/") == Some(t)
                || self.slug.strip_prefix("decisions/") == Some(t)
                || self.str("id").is_some_and(|id| id.eq_ignore_ascii_case(t)))
    }

    /// When this task landed: `merged_at` where the door stamped it, else
    /// derived from door-witnessed numbers (`claimed_at + work_ms`).
    fn landed_at(&self) -> Option<DateTime<Utc>> {
        self.time("merged_at").or_else(|| {
            self.time("claimed_at")
                .map(|t| t + Duration::milliseconds(self.u64("work_ms") as i64))
        })
    }

    /// When this story/project closed: `closed_at`, with the same style of
    /// fallback for pages closed by pre-telemetry doors — the approval
    /// instant is the closest door-witnessed proxy.
    fn closed_at(&self) -> Option<DateTime<Utc>> {
        self.time("closed_at").or_else(|| {
            (self.str("status") == Some("done"))
                .then(|| self.approval_at())
                .flatten()
        })
    }

    fn approval_at(&self) -> Option<DateTime<Utc>> {
        self.fm
            .get("approval")
            .and_then(|a| a.get("at"))
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc))
    }
}

fn in_period(t: DateTime<Utc>, period: YearMonth) -> bool {
    use chrono::Datelike;
    t.year() == period.year as i32 && t.month() == period.month
}

// ── The corpus and the assembly (pure) ─────────────────────────────────────

/// Every page the report reads, by class.
#[derive(Debug, Default)]
pub struct Corpus {
    pub projects: Vec<Page>,
    pub stories: Vec<Page>,
    pub tasks: Vec<Page>,
    pub decisions: Vec<Page>,
}

/// One decision's price for the period.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DecisionRow {
    pub projects: u64,
    pub stories: u64,
    pub tasks: u64,
    pub merge_commits: Vec<String>,
    pub machine_ms: u64,
    pub signoffs: u64,
    pub closes: u64,
    pub bounces: u64,
    pub door_refusals: u64,
    /// Decision page slugs, for the report's graph edge.
    pub decision_slugs: Vec<String>,
}

/// The assembled month.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub period: String,
    pub decisions: BTreeMap<String, DecisionRow>,
    pub unattributed_tasks: u64,
    pub unattributed_ms: u64,
    pub discarded_items: u64,
    pub discarded_ms: u64,
}

/// A task's resolved lineage: (decision reference, decision slug) pairs,
/// plus the story and project the walk passed through.
type Attribution<'a> = (Vec<(String, String)>, &'a Page, &'a Page);

impl Corpus {
    fn find<'a>(&'a self, pages: &'a [Page], target: &str) -> Option<&'a Page> {
        pages.iter().find(|p| p.is_target_of(target))
    }

    /// task → story → project, then the project's `implements` targets
    /// resolved to decision references (`ADR-NNNN`).
    fn attribution(&self, task: &Page) -> Option<Attribution<'_>> {
        let story = self.find(&self.stories, task.str("story")?)?;
        let project = self.find(&self.projects, story.str("project")?)?;
        let implements = project.fm.get("implements")?.as_sequence()?;
        let mut refs = Vec::new();
        for target in implements.iter().filter_map(Value::as_str) {
            let decision = self.find(&self.decisions, target)?;
            refs.push((
                decision.str("reference")?.to_string(),
                decision.slug.clone(),
            ));
        }
        (!refs.is_empty()).then_some((refs, story, project))
    }
}

/// Assemble the period's report from the corpus. Pure and deterministic:
/// same pages, same report.
pub fn assemble(period: YearMonth, corpus: &Corpus) -> Report {
    let mut report = Report {
        period: period.to_string(),
        ..Report::default()
    };

    // Discarded work: cancelled items to date, and any execution they carried.
    for page in corpus
        .projects
        .iter()
        .chain(&corpus.stories)
        .chain(&corpus.tasks)
    {
        if page.str("status") == Some("cancelled") {
            report.discarded_items += 1;
            report.discarded_ms += page.u64("work_ms");
        }
    }

    // Tasks landed in the period, attributed through the graph. Parents
    // count once per decision, not once per task under them.
    let mut counted: std::collections::BTreeSet<(String, String)> = Default::default();
    for task in &corpus.tasks {
        if task.str("status") != Some("done") {
            continue;
        }
        let Some(landed) = task.landed_at() else {
            continue;
        };
        if !in_period(landed, period) {
            continue;
        }
        let Some((refs, story, project)) = corpus.attribution(task) else {
            report.unattributed_tasks += 1;
            report.unattributed_ms += task.u64("work_ms");
            continue;
        };
        for (reference, decision_slug) in refs {
            let row_key = reference.clone();
            let row = report.decisions.entry(reference).or_default();
            row.tasks += 1;
            row.machine_ms += task.u64("work_ms");
            if let Some(sha) = task.str("merge_commit") {
                row.merge_commits.push(sha.to_string());
            }
            row.bounces += task.u64("bounces");
            row.door_refusals += task.u64("door_refusals");
            if task.approval_at().is_some_and(|t| in_period(t, period)) {
                row.signoffs += 1;
            }
            for (parent, kind) in [(story, "story"), (project, "project")] {
                if !counted.insert((row_key.clone(), parent.slug.clone())) {
                    continue;
                }
                match kind {
                    "story" => row.stories += 1,
                    _ => row.projects += 1,
                }
                row.bounces += parent.u64("bounces");
                row.door_refusals += parent.u64("door_refusals");
                if parent.approval_at().is_some_and(|t| in_period(t, period)) {
                    row.signoffs += 1;
                }
                if parent.closed_at().is_some_and(|t| in_period(t, period)) {
                    row.closes += 1;
                }
            }
            if !row.decision_slugs.contains(&decision_slug) {
                row.decision_slugs.push(decision_slug);
            }
        }
    }
    for row in report.decisions.values_mut() {
        row.merge_commits.sort();
        row.merge_commits.dedup();
    }
    report
}

// ── Rendering (deterministic) ──────────────────────────────────────────────

/// The `measure-report` page. No emission timestamp anywhere: the same
/// pages produce byte-identical bytes, so a re-run converges instead of
/// churning history.
pub fn render_page(report: &Report, wiki: &str) -> String {
    let mut out = String::new();
    let total_ms: u64 = report.decisions.values().map(|r| r.machine_ms).sum();
    out.push_str("---\n");
    out.push_str(&format!(
        "title: \"Measure report — {wiki} {}\"\n",
        report.period
    ));
    out.push_str("type: measure-report\n");
    out.push_str("status: active\n");
    out.push_str(&format!(
        "summary: \"{} decision(s) priced in {}: {} task(s) landed, {} minutes of machine time, {} human gate action(s).\"\n",
        report.decisions.len(),
        report.period,
        report.decisions.values().map(|r| r.tasks).sum::<u64>(),
        total_ms / 60_000,
        report
            .decisions
            .values()
            .map(|r| r.signoffs + r.closes + r.bounces + r.door_refusals)
            .sum::<u64>(),
    ));
    out.push_str(&format!("period: \"{}\"\n", report.period));
    out.push_str("instrument: tuesday\n");
    out.push_str(&format!("source: \"kb:{wiki}\"\n"));
    if !report.decisions.is_empty() {
        out.push_str("decisions:\n");
        for (reference, row) in &report.decisions {
            out.push_str(&format!("  {reference}:\n"));
            out.push_str(&format!(
                "    landed: {{projects: {}, stories: {}, tasks: {}}}\n",
                row.projects, row.stories, row.tasks
            ));
            out.push_str(&format!(
                "    merge_commits: [{}]\n",
                row.merge_commits.join(", ")
            ));
            out.push_str(&format!("    machine_ms: {}\n", row.machine_ms));
            out.push_str(&format!(
                "    human_gates: {{signoffs: {}, closes: {}, bounces: {}, door_refusals: {}}}\n",
                row.signoffs, row.closes, row.bounces, row.door_refusals
            ));
        }
    }
    out.push_str(&format!(
        "unattributed: {{tasks: {}, machine_ms: {}}}\n",
        report.unattributed_tasks, report.unattributed_ms
    ));
    out.push_str(&format!(
        "discarded: {{items: {}, machine_ms: {}}}\n",
        report.discarded_items, report.discarded_ms
    ));
    let mut slugs: Vec<&str> = report
        .decisions
        .values()
        .flat_map(|r| r.decision_slugs.iter().map(String::as_str))
        .collect();
    slugs.sort();
    slugs.dedup();
    if !slugs.is_empty() {
        out.push_str("relates_to:\n");
        for slug in slugs {
            out.push_str(&format!("  - {slug}\n"));
        }
    }
    out.push_str("---\n\n");
    out.push_str(&body(report));
    out
}

/// The readable half: the thread per decision, and the honest remainders.
fn body(report: &Report) -> String {
    let mut out = String::new();
    if report.decisions.is_empty() {
        out.push_str(&format!(
            "No work landed against any decision in {}.\n",
            report.period
        ));
    }
    for (reference, row) in &report.decisions {
        out.push_str(&format!("## {reference}\n\n"));
        out.push_str(&format!(
            "Landed {} task(s) under {} story(ies) in {} project(s) — merge commits {}.\n\n",
            row.tasks,
            row.stories,
            row.projects,
            row.merge_commits.join(", "),
        ));
        out.push_str(&format!(
            "Machine time: {} ms (~{} min) of execution between claim and the merge door.\n",
            row.machine_ms,
            row.machine_ms / 60_000
        ));
        out.push_str(&format!(
            "Human attention at the gates: {} sign-off(s), {} close(s), {} bounce(s), {} refused knock(s).\n\n",
            row.signoffs, row.closes, row.bounces, row.door_refusals
        ));
    }
    out.push_str(&format!(
        "Unattributed: {} task(s), {} ms — work whose graph walk reaches no decision.\n",
        report.unattributed_tasks, report.unattributed_ms
    ));
    out.push_str(&format!(
        "Discarded to date: {} cancelled item(s), {} ms of execution set aside.\n",
        report.discarded_items, report.discarded_ms
    ));
    out.push_str(
        "\nReview verdicts are not yet a typed class (taps 115); \
         friction counters are lifetime-to-date on the items involved.\n",
    );
    out
}

/// The compact human table for stdout.
pub fn render_table(report: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!("Measure — {}\n", report.period));
    for (reference, row) in &report.decisions {
        out.push_str(&format!(
            "{reference:<10} {} task(s)  {:>6} min machine  {} gate action(s)\n",
            row.tasks,
            row.machine_ms / 60_000,
            row.signoffs + row.closes + row.bounces + row.door_refusals
        ));
    }
    if report.decisions.is_empty() {
        out.push_str("no attributed work landed\n");
    }
    if report.unattributed_tasks > 0 {
        out.push_str(&format!(
            "unattributed: {} task(s), {} ms\n",
            report.unattributed_tasks, report.unattributed_ms
        ));
    }
    out.push_str(&format!(
        "discarded to date: {} item(s)\n",
        report.discarded_items
    ));
    out
}

// ── The transport shell ────────────────────────────────────────────────────

/// Read every page of one type over the transport.
async fn read_type(kb: &KbClient, type_name: &str) -> Result<Vec<Page>, Error> {
    let mut pages = Vec::new();
    let mut page_no = 1;
    loop {
        let list = kb
            .call_json(
                "wiki_list",
                serde_json::json!({"type": type_name, "page": page_no, "page_size": 200}),
            )
            .await?;
        let batch: Vec<String> = list["pages"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r["slug"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        for slug in batch {
            let text = kb
                .call("wiki_content_read", serde_json::json!({"uri": slug}))
                .await?;
            pages.push(Page::parse(&slug, &text)?);
        }
        let total = list["total"].as_u64().unwrap_or(0) as usize;
        if pages.len() >= total {
            break;
        }
        page_no += 1;
    }
    pages.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(pages)
}

/// The whole lane: connect, read, assemble, write the report page, return
/// the human/json rendering for stdout.
pub async fn run(period: YearMonth, json: bool) -> Result<String, Error> {
    let Some(target) = KbTarget::discover() else {
        return Err("no KB configured: set KB_URL (and optionally KB_WIKI) — \
             in the environment, a .env here, or ~/.config/taps/env"
            .into());
    };
    let wiki = target.wiki.clone().unwrap_or_else(|| "default".into());
    let kb = KbClient::connect(&target).await?;
    kb.ensure_schema("measure-report", MEASURE_REPORT_SCHEMA)
        .await?;

    let corpus = Corpus {
        projects: read_type(&kb, "project").await?,
        stories: read_type(&kb, "story").await?,
        tasks: read_type(&kb, "task").await?,
        decisions: read_type(&kb, "decision").await?,
    };
    let report = assemble(period, &corpus);

    let slug = format!("measures/{}", report.period);
    let page = render_page(&report, &wiki);
    kb.call(
        "wiki_content_write",
        serde_json::json!({"uri": slug, "content": page}),
    )
    .await?;
    let ingest = kb
        .call_json("wiki_ingest", serde_json::json!({"path": "measures"}))
        .await?;
    kb.close().await.ok();

    let mut out = if json {
        serde_json::to_string_pretty(&serde_json::json!({
            "period": report.period,
            "page": slug,
            "decisions": report.decisions.iter().map(|(k, r)| {
                (k.clone(), serde_json::json!({
                    "landed": {"projects": r.projects, "stories": r.stories, "tasks": r.tasks},
                    "merge_commits": r.merge_commits,
                    "machine_ms": r.machine_ms,
                    "human_gates": {
                        "signoffs": r.signoffs, "closes": r.closes,
                        "bounces": r.bounces, "door_refusals": r.door_refusals,
                    },
                }))
            }).collect::<serde_json::Map<_, _>>(),
            "unattributed": {"tasks": report.unattributed_tasks, "machine_ms": report.unattributed_ms},
            "discarded": {"items": report.discarded_items, "machine_ms": report.discarded_ms},
            "ingest": ingest,
        }))?
    } else {
        format!(
            "{}\nreport page: {} (ingest: {})\n",
            render_table(&report).trim_end(),
            slug,
            ingest["pages_validated"].as_u64().unwrap_or(0),
        )
    };
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

// ── Tests (the pure half) ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn page(slug: &str, yaml: &str) -> Page {
        Page::parse(slug, &format!("---\n{yaml}\n---\n\nbody\n")).unwrap()
    }

    fn period() -> YearMonth {
        YearMonth {
            year: 2026,
            month: 8,
        }
    }

    /// The walk's shape: one decision, one project/story, two landed tasks —
    /// one door-stamped (`merged_at`), one pre-telemetry (fallback from
    /// `claimed_at + work_ms`).
    fn corpus() -> Corpus {
        Corpus {
            decisions: vec![page(
                "decisions/0001-write-it-down",
                "id: DEC00000000000000000000001\nreference: ADR-0001\ntype: decision\nstatus: accepted",
            )],
            projects: vec![page(
                "work/project-1-p",
                "id: PRJ00000000000000000000001\ntype: project\nstatus: done\n\
                 implements:\n  - decisions/0001-write-it-down\n\
                 approval: {by: m@x, at: \"2026-08-17T02:53:59Z\", content_sha256: aa}\n\
                 closed_at: \"2026-08-17T04:00:00Z\"",
            )],
            stories: vec![page(
                "work/story-1-s",
                "id: STY00000000000000000000001\ntype: story\nstatus: done\n\
                 project: PRJ00000000000000000000001\n\
                 approval: {by: m@x, at: \"2026-08-17T02:54:27Z\", content_sha256: bb}",
            )],
            tasks: vec![
                page(
                    "work/task-3-t",
                    "id: TSK00000000000000000000003\ntype: task\nstatus: done\n\
                     story: STY00000000000000000000001\n\
                     approval: {by: m@x, at: \"2026-08-17T02:54:39Z\", content_sha256: cc}\n\
                     claimed_at: \"2026-08-17T02:57:00Z\"\nwork_ms: 2184038\n\
                     merge_commit: ea6c661aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ),
                page(
                    "work/task-4-t",
                    "id: TSK00000000000000000000004\ntype: task\nstatus: done\n\
                     story: STY00000000000000000000001\n\
                     approval: {by: m@x, at: \"2026-08-17T02:54:42Z\", content_sha256: dd}\n\
                     merged_at: \"2026-08-17T04:10:00Z\"\nwork_ms: 524307\n\
                     merge_commit: c6908beaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\
                     door_refusals: 1",
                ),
            ],
        }
    }

    #[test]
    fn the_graph_attributes_and_the_currencies_add_up() {
        let report = assemble(period(), &corpus());
        assert_eq!(report.decisions.len(), 1);
        let row = &report.decisions["ADR-0001"];
        assert_eq!((row.projects, row.stories, row.tasks), (1, 1, 2));
        assert_eq!(row.machine_ms, 2184038 + 524307);
        assert_eq!(row.merge_commits.len(), 2);
        // Four sign-offs (project, story, both tasks); two closes — the
        // project's stamped closed_at plus the story's pre-telemetry
        // fallback (done, no closed_at → approval proxy); one refused
        // knock carried from the page.
        assert_eq!(row.signoffs, 4);
        assert_eq!(row.closes, 2);
        assert_eq!(row.bounces, 0);
        assert_eq!(row.door_refusals, 1);
        assert_eq!(report.unattributed_tasks, 0);
    }

    #[test]
    fn a_pre_telemetry_task_lands_via_the_claimed_at_fallback() {
        let mut c = corpus();
        // Push the fallback task's claim into July: it leaves the period.
        c.tasks[0].fm.insert(
            Value::String("claimed_at".into()),
            Value::String("2026-07-01T00:00:00Z".into()),
        );
        let report = assemble(period(), &c);
        assert_eq!(report.decisions["ADR-0001"].tasks, 1);
    }

    #[test]
    fn a_task_with_no_graph_path_is_unattributed_not_hidden() {
        let mut c = corpus();
        c.stories.clear(); // sever the walk
        let report = assemble(period(), &c);
        assert!(report.decisions.is_empty());
        assert_eq!(report.unattributed_tasks, 2);
        assert_eq!(report.unattributed_ms, 2184038 + 524307);
    }

    #[test]
    fn cancelled_items_are_discarded_cost() {
        let mut c = corpus();
        c.tasks.push(page(
            "work/task-9-dead",
            "id: TSK00000000000000000000009\ntype: task\nstatus: cancelled\n\
             story: STY00000000000000000000001\nwork_ms: 5000",
        ));
        let report = assemble(period(), &c);
        assert_eq!(report.discarded_items, 1);
        assert_eq!(report.discarded_ms, 5000);
    }

    #[test]
    fn the_page_is_deterministic_and_typed() {
        let report = assemble(period(), &corpus());
        let a = render_page(&report, "myproject");
        let b = render_page(&report, "myproject");
        assert_eq!(a, b, "same pages, same bytes");
        assert!(a.starts_with("---\n"));
        assert!(a.contains("type: measure-report"));
        assert!(a.contains("period: \"2026-08\""));
        assert!(a.contains("  ADR-0001:"));
        assert!(a.contains("machine_ms: 2708345"));
        assert!(a.contains("relates_to:\n  - decisions/0001-write-it-down"));
        // No emission timestamp anywhere — determinism is the contract.
        assert!(!a.contains("last_updated"));
    }
}

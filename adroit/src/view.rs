//! Shared serde view types — the single source of truth for "what a surface
//! can show". Every surface (CLI, future TUI, future web JSON API) consumes
//! these structs, so read/derive logic is written once in [`crate::query`].
//!
//! These are **pure data**: no filesystem, ratatui, or axum types here. They
//! all derive [`Serialize`] so the future web surface can emit them as JSON
//! with zero extra mapping (Step 4). HTML rendering is deliberately *not* done
//! here — that is a web-only concern deferred to Step 4; bodies stay raw.

use serde::Serialize;

use crate::adr::Status;

// The read-slice types conduit consumes — list rows, full detail, the plan
// envelope, and their leaf types — live in `como-contract` (the suite's
// shared-seam crate): adroit serializes and conduit deserializes the SAME
// structs, so the seam cannot drift. Re-exported so `crate::view::*` paths
// and the `-o json` contract stay unchanged.
pub use como_contract::adroit::{AdrDetail, AdrSummary, EdgeKind, ForgeData, Plan, RelatedLink};

#[cfg(test)]
mod forge_data_tests {
    use super::*;

    fn data() -> ForgeData {
        ForgeData {
            issue_url: None,
            pr_url: None,
            pr_approvals: None,
            pr_ci: None,
            pr_merged: None,
            pr_closed: None,
            issue_state: None,
        }
    }

    #[test]
    fn status_parts_shows_pr_and_tracker_issue_independently() {
        // Split setup: a merged forge PR alongside a closed tracker issue → both.
        let f = ForgeData {
            pr_url: Some("…/pull/13".into()),
            pr_merged: Some(true),
            issue_url: Some("…/issue/COM-5".into()),
            issue_state: Some("closed".into()),
            ..data()
        };
        assert_eq!(f.status_parts(), vec!["PR merged", "issue closed"]);

        // Issue-only (tracker, no PR), state known then unknown.
        let mut g = data();
        g.issue_url = Some("…/COM-5".into());
        g.issue_state = Some("open".into());
        assert_eq!(g.status_parts(), vec!["issue open"]);
        g.issue_state = None;
        assert_eq!(g.status_parts(), vec!["issue linked"]);

        // PR with live review state, no tracker issue.
        let h = ForgeData {
            pr_url: Some("…/pull/1".into()),
            pr_approvals: Some(2),
            pr_ci: Some("ok".into()),
            ..data()
        };
        assert_eq!(h.status_parts(), vec!["PR 2 approvals, ci ok"]);

        // Closed-without-merge (e.g. a rejected ADR's PR) reads `closed`, not its
        // approvals/CI — and `merged` still wins over `closed` if both were set.
        let k = ForgeData {
            pr_url: Some("…/pull/14".into()),
            pr_approvals: Some(0),
            pr_ci: Some("none".into()),
            pr_closed: Some(true),
            issue_url: Some("…/COM-6".into()),
            issue_state: Some("closed".into()),
            ..data()
        };
        assert_eq!(k.status_parts(), vec!["PR closed", "issue closed"]);
    }
}

#[cfg(test)]
mod sanitize_report_tests {
    use super::*;

    #[test]
    fn empty_report_has_no_human_line_and_serializes_to_nothing() {
        let r = SanitizeReport::default();
        assert!(r.is_empty());
        assert_eq!(r.human_line(), None);
        // Every field is skip-if-zero, so a clean report is an empty object.
        assert_eq!(serde_json::to_string(&r).unwrap(), "{}");
    }

    #[test]
    fn human_line_lists_only_non_zero_rules_in_a_stable_order() {
        let r = SanitizeReport {
            bracket_placeholder: 2,
            residue: 1,
            skeleton_echo: 0,
            identity_echo: 3,
            marker_echo: 0,
        };
        assert!(!r.is_empty());
        // Stable order: bracket-placeholder, residue, skeleton-echo,
        // identity-echo, marker-echo; zero rules omitted.
        assert_eq!(
            r.human_line().unwrap(),
            "2 bracket-placeholder, 1 residue, 3 identity-echo"
        );
    }

    #[test]
    fn add_accumulates_per_seed_reports() {
        let mut total = SanitizeReport::default();
        total.add(&SanitizeReport {
            bracket_placeholder: 1,
            residue: 2,
            ..SanitizeReport::default()
        });
        total.add(&SanitizeReport {
            bracket_placeholder: 3,
            skeleton_echo: 1,
            ..SanitizeReport::default()
        });
        assert_eq!(total.bracket_placeholder, 4);
        assert_eq!(total.residue, 2);
        assert_eq!(total.skeleton_echo, 1);
    }

    #[test]
    fn json_omits_zero_rules_present_non_zero_ones() {
        let r = SanitizeReport {
            bracket_placeholder: 2,
            residue: 1,
            ..SanitizeReport::default()
        };
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["bracket_placeholder"], 2);
        assert_eq!(v["residue"], 1);
        // Zero rules are absent entirely, not serialized as `0`.
        assert!(v.get("skeleton_echo").is_none(), "{v}");
        assert!(v.get("identity_echo").is_none(), "{v}");
        assert!(v.get("marker_echo").is_none(), "{v}");
    }

    #[test]
    fn import_summary_omits_sanitized_when_none() {
        let s = ImportSummary {
            source: "x.yaml".into(),
            assessment: "X".into(),
            dry_run: false,
            seeded: Vec::new(),
            skipped: Vec::new(),
            sanitized: None,
        };
        let v: serde_json::Value = serde_json::to_value(&s).unwrap();
        // The additive field is absent unless drops occurred — the legacy shape.
        assert!(v.get("sanitized").is_none(), "{v}");
    }
}

/// Aggregate statistics across all ADRs, for a stats dashboard.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct Stats {
    /// Total number of ADRs.
    pub total: usize,
    /// Count of ADRs per status (every status present, including zeroes), in
    /// lifecycle order.
    pub by_status: Vec<StatusCount>,
    /// How long each still-`Proposed` ADR has been sitting, oldest first.
    pub proposed_age: Vec<ProposedAge>,
    /// ADRs flagged as due for review (still `Proposed` and past their
    /// `review_by` deadline — see [`AdrSummary::review_due`]).
    pub review_due: Vec<AdrSummary>,
    /// Number of ADRs created per calendar month (`YYYY-MM`), oldest first.
    pub created_over_time: Vec<CreatedBucket>,
}

/// Count of ADRs in a single status.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct StatusCount {
    pub status: Status,
    pub count: usize,
}

/// How long a `Proposed` ADR has been waiting.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct ProposedAge {
    pub number: Option<u32>,
    /// Display id and routing token (so the surface can link it under any scheme).
    pub reference: String,
    pub address: String,
    pub title: String,
    /// Whole days since creation (best-effort; `None` if the created date is
    /// unknown).
    pub age_days: Option<i64>,
    /// `true` when this still-`Proposed` ADR is also flagged review-due (past its
    /// `review_by` deadline or aged past the staleness threshold) — the same
    /// signal as [`AdrSummary::review_due`], carried here so a surface can flag
    /// the row inline without cross-referencing [`Stats::review_due`].
    pub review_due: bool,
}

/// ADRs created in a given calendar month.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct CreatedBucket {
    /// Calendar month as `YYYY-MM`.
    pub month: String,
    pub count: usize,
}

/// The supersession / relationship graph across all ADRs.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// A node in the [`Graph`]: one ADR. Keyed by `reference` (its display id);
/// `address` is the routing token (`None` for an unassigned ADR).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct GraphNode {
    pub reference: String,
    pub address: Option<String>,
    pub title: String,
    pub status: Status,
}

/// A directed edge in the [`Graph`], connecting nodes by their `reference`.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct GraphEdge {
    /// Source ADR reference.
    pub from: String,
    /// Target ADR reference.
    pub to: String,
    pub kind: EdgeKind,
}

/// A structured repo-validation report — the same checks as `adroit check`,
/// surfaced through the shared query layer so every surface (CLI, web, future
/// TUI) reports identical problems.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct CheckReport {
    /// Number of ADR files inspected.
    pub checked: usize,
    /// Every problem found, sorted by severity (errors first) then message.
    /// Empty when the repo is clean.
    pub problems: Vec<Problem>,
}

/// One validation problem found by [`crate::query::check`].
///
/// Carries both a flat `message` (rendered verbatim by the CLI — byte-identical
/// to historical `adroit check` output) and structured fields (`label` /
/// `summary` / `paths`) so a richer surface, like the web repo-health panel, can
/// lay it out instead of printing one line.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct Problem {
    /// How serious the problem is.
    pub severity: Severity,
    /// Which category of check produced it (for grouping / filtering).
    pub kind: ProblemKind,
    /// Headline identifier: the ADR reference (`"ADR-0009"`) for a duplicate, or
    /// the offending file's relative path otherwise.
    pub label: String,
    /// Short description with neither the leading `label` nor the path list —
    /// e.g. `"duplicate number"`, `"broken link [..] — target file not found"`.
    pub summary: String,
    /// Affected files (relative to the repo root), each with its size. The
    /// duplicate check lists every colliding file here — the line/byte counts let
    /// a surface flag a header-only stub vs. a full ADR; empty when `label`
    /// already names the single file.
    pub paths: Vec<ProblemFile>,
    /// The full one-line message — byte-identical to the `adroit check` line, so
    /// the CLI renders it verbatim.
    pub message: String,
}

/// One file implicated in a [`Problem`], with its size so a surface can hint at
/// what's worth diffing — e.g. a few-line header-only stub vs. a full ADR.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct ProblemFile {
    /// Path relative to the repo root.
    pub path: String,
    /// Line count (`0` if the file can't be read as text).
    pub lines: usize,
    /// Byte size on disk.
    pub bytes: u64,
}

/// Severity of a [`Problem`]. `Error` sorts before `Warning`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// A real defect (duplicate id, unparseable file, a link to a nonexistent
    /// ADR) — `adroit check` exits non-zero when any error is present.
    Error,
    /// A fixable inconsistency (a stale cross-ADR link `adroit relink` repairs).
    /// `adroit check` reports warnings but does **not** fail on them, so a
    /// deferred-relink PR branch still passes CI.
    Warning,
}

/// The category of a validation [`Problem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// Two ADR files share one identity (number / slug / uuid).
    DuplicateId,
    /// Two or more ADRs share the same (case-insensitive) title — usually an
    /// accidental re-run of `new`. Advisory (a `Warning`): titles *can* repeat.
    DuplicateTitle,
    /// An ADR page whose frontmatter fails to parse.
    Unparseable,
    /// A `Supersedes` / `Superseded by` note references a nonexistent ADR.
    BrokenSupersession,
    /// A relative `.md` link that names an ADR which no longer exists anywhere
    /// — it points nowhere. (A missing target that *does* name a known ADR is
    /// a [`StaleLink`](ProblemKind::StaleLink) instead; a missing target that
    /// is not an ADR link at all is an advisory
    /// [`ExternalLink`](ProblemKind::ExternalLink).)
    BrokenLink,
    /// A relative `.md` link that points somewhere other than its ADR's current
    /// home — the ADR exists, so `adroit relink` repairs it. Covers both a
    /// wrong-but-present path and a missing path whose ADR lives elsewhere.
    StaleLink,
    /// A relative `.md` link whose target is missing and which is not an ADR
    /// link — it leaves the corpus (a book page, an asset). Advisory: in a
    /// seeded ephemeral space (ADR-0020) such targets can never resolve, and
    /// validating them is the owning repo's job.
    ExternalLink,
    /// Forge state disagrees with the ADR (e.g. an accepted ADR whose PR isn't
    /// merged, or a closed issue with no matching status change). Surfaced only
    /// by the opt-in `check --forge`.
    ForgeIntegration,
}

/// The `-o json` shape of `adroit ask`: the model's answer plus the references of
/// the ADRs it was grounded on (the mechanically-retrieved sources, so a caller
/// can cite or re-open them).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct AskAnswer {
    /// The synthesized answer (prose).
    pub answer: String,
    /// References of the ADRs used as context, most relevant first.
    pub sources: Vec<String>,
}

/// Per-rule counts of the lines the AI-draft sanitizer dropped from model
/// output during an `import --ai` run, aggregated across every fleshed-out
/// seed. The sanitizer ([`crate::ai::sanitize_draft`]) silently removes
/// model-shaped filler before the splice — without these counts the output
/// artifacts can't distinguish "the model emitted nothing bad" from "the
/// sanitizer ate it" (the run-3 observability wart). One field per drop rule
/// `sanitize_draft` actually has; the no-op retitle (`## Implementation` →
/// `## Implementation notes`) keeps its content, so it is *not* a drop and is
/// not counted here.
///
/// **Zero-rules-omitted, house serde convention:** every field is
/// `skip_serializing_if`-zero, so only rules that actually fired appear in
/// `import -o json`. A run with drops carries a `sanitized` object listing
/// just the non-zero rules; a clean run omits the field entirely (it is
/// `Option`al on [`ImportSummary`]). Additive in `manifest_schema` 1 —
/// consumers that predate it simply never see the key.
///
/// **Counts are non-blank content lines only** — the telemetry measures how
/// much *content* the sanitizer removed, not the surrounding whitespace it
/// normalizes (a dropped skeleton section's interior blank lines, or the blank
/// lines around a trailing-residue paragraph, don't inflate the tally).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct SanitizeReport {
    /// Whole-line bracket placeholders dropped (`[Insert …]` / `[Your Name]` /
    /// `[TBD]` — the run-2 novel-filler class; fenced/indented code exempt).
    #[serde(skip_serializing_if = "is_zero")]
    pub bracket_placeholder: u32,
    /// Trailing conversational-residue lines dropped (a recognized closer
    /// paragraph — "Please review this revised ADR body…", "Let me know if…" —
    /// plus any horizontal rule it orphaned).
    #[serde(skip_serializing_if = "is_zero")]
    pub residue: u32,
    /// Skeleton-echo lines dropped (a re-emitted `## Status` / `## Stakeholders`
    /// section — heading and content — that duplicates the mechanical preamble
    /// the splice preserves).
    #[serde(skip_serializing_if = "is_zero")]
    pub skeleton_echo: u32,
    /// Leading identity-echo lines dropped (a re-emitted title `# ` H1 or a
    /// `> State:` banner before real content begins).
    #[serde(skip_serializing_if = "is_zero")]
    pub identity_echo: u32,
    /// Echoed adroit-marker lines dropped (`<!-- adroit:ai-suggested -->` /
    /// `<!-- adroit:seeded-from-assessment -->` re-emitted by the model — the
    /// wrapper/seed path owns those).
    #[serde(skip_serializing_if = "is_zero")]
    pub marker_echo: u32,
}

impl SanitizeReport {
    /// Total lines dropped across every rule — `true` when nothing was dropped,
    /// so the caller can omit an all-zero `sanitized` object and skip the human
    /// telemetry line.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Accumulate another report's counts into this one (one fleshed-out seed's
    /// drops folded into the run total).
    pub fn add(&mut self, other: &SanitizeReport) {
        self.bracket_placeholder += other.bracket_placeholder;
        self.residue += other.residue;
        self.skeleton_echo += other.skeleton_echo;
        self.identity_echo += other.identity_echo;
        self.marker_echo += other.marker_echo;
    }

    /// The human one-liner for `import --ai` output: the non-zero rules in a
    /// stable order, e.g. `2 bracket-placeholder, 1 residue`. `None` when
    /// nothing was dropped (no line to print).
    pub fn human_line(&self) -> Option<String> {
        let parts: Vec<String> = [
            (self.bracket_placeholder, "bracket-placeholder"),
            (self.residue, "residue"),
            (self.skeleton_echo, "skeleton-echo"),
            (self.identity_echo, "identity-echo"),
            (self.marker_echo, "marker-echo"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The `-o json` shape of `adroit import`: a machine summary of one ingest run,
/// so a loop runner (the assessments seam-check, the Adopt-stage engine) can
/// assert what was seeded without scraping the human report. Counts are the
/// array lengths — no duplicated tallies to drift.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct ImportSummary {
    /// The assessment export file, as given on the command line.
    pub source: String,
    /// The assessment's own `name` field (provenance).
    pub assessment: String,
    /// `true` for a `--dry-run` preview — nothing was written; `seeded` lists
    /// what a wet run *would* create.
    pub dry_run: bool,
    /// One entry per ADR seeded (or, under `dry_run`, per ADR that would be).
    pub seeded: Vec<ImportSeed>,
    /// Titles of practices skipped by the dedupe guard — an ADR with that title
    /// already exists (or was seeded earlier in this same run). `--force` empties
    /// this by seeding anyway.
    pub skipped: Vec<String>,
    /// Per-rule counts of what the AI-draft sanitizer dropped while fleshing out
    /// seeds (`--ai` only). Present **only** when at least one drop occurred —
    /// a clean run (or any run without `--ai`) omits the field. Additive in
    /// `manifest_schema` 1: makes the silent sanitizer observable so a loop
    /// runner can tell "the model emitted nothing bad" from "the sanitizer ate
    /// it" (the run-3 wart).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanitized: Option<SanitizeReport>,
}

/// One seeded (or would-be-seeded) ADR in an [`ImportSummary`].
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "manifest", derive(schemars::JsonSchema))]
pub struct ImportSeed {
    /// The created ADR's display reference (e.g. `ADR-0012`). `null` under
    /// `--dry-run`: identity is allocated only on write (and isn't predictable
    /// for every naming scheme), so a preview truthfully carries none.
    pub reference: Option<String>,
    /// The ADR title — the practice name verbatim.
    pub title: String,
    /// The seeded lifecycle status — the configured `default_status`
    /// (`Proposed` unless overridden; an import never decides anything),
    /// carried explicitly so consumers needn't hardcode it.
    pub status: Status,
    /// The source domain in the assessment (provenance).
    pub domain: String,
}

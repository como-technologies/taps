//! Adroit's machine-readable read slice: the `-o json` shapes conduit (the
//! Adopt stage) consumes — list rows, full detail, the plan envelope — plus
//! the manifest handshake. Adroit serializes these types; conduit
//! deserializes the same types, so the seam cannot drift (pre-workspace,
//! conduit maintained hand-written tolerant mirrors and pinned adroit by
//! revision).
//!
//! Fields that arrived after `manifest_schema` 1 shipped are `#[serde(default)]`
//! so a reader stays compatible with envelopes that predate them.

use serde::{Deserialize, Serialize};

/// The manifest handshake (conduit spec §Handshake): a consumer runs
/// `adroit manifest -o json` and requires exactly this tool name and schema.
pub const ADROIT_TOOL: &str = "adroit";
pub const ADROIT_MANIFEST_SCHEMA: u32 = 1;

/// The lifecycle status of an Architecture Decision Record.
///
/// Serialized lowercase to match the KB `decision` schema's status enum
/// (adroit ADR-0020).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    strum::Display,
    strum::EnumString,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[strum(ascii_case_insensitive)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Proposed,
    Accepted,
    Rejected,
    Deprecated,
    Superseded,
}

impl Status {
    /// All statuses in lifecycle order. Useful for iterating layout dirs
    /// and rendering grouped indexes.
    pub const ALL: [Status; 5] = [
        Status::Proposed,
        Status::Accepted,
        Status::Rejected,
        Status::Superseded,
        Status::Deprecated,
    ];
}

/// One row in a list / table of ADRs. Enough to render a list line without
/// reading the full body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AdrSummary {
    /// Numeric ADR number (e.g. `6`). `None` for non-numeric naming schemes
    /// (date/uuid) or an ADR with no number yet.
    pub number: Option<u32>,
    /// Zero-padded display form of the number (e.g. `"0006"`, or `"????"`).
    #[serde(default)]
    pub number_display: String,
    /// The naming scheme's canonical display identifier — `"ADR-0006"` for the
    /// sequential scheme, the `YYYYMMDD-slug` for date, `"ADR-<short-uuid>"` for
    /// uuid. The surface-facing identity that works across all schemes.
    pub reference: String,
    /// The canonical **addressing** token — what a URL/CLI passes to reach this
    /// ADR (the bare number for numeric schemes, the slug/uuid for slug schemes).
    /// Surfaces route by this so date/uuid ADRs are reachable too.
    pub address: String,
    /// Short title describing the decision.
    pub title: String,
    /// Current lifecycle status.
    pub status: Status,
    /// Creation timestamp as an RFC 3339 string (`None` if unknown).
    ///
    /// Stored as a string so the contract carries no `time` types and
    /// serializes identically across surfaces.
    pub created: Option<String>,
    /// Display references of older ADRs this record supersedes (e.g.
    /// `["ADR-0002"]` or `["20260601-x"]`).
    #[serde(default)]
    pub supersedes: Vec<String>,
    /// Display reference of the newer ADR that supersedes this record, if any.
    pub superseded_by: Option<String>,
    /// "This ADR is due for review": `true` when the ADR is still `Proposed`,
    /// has a `review_by` deadline, and that deadline is on or before today.
    /// Computed by adroit's query layer from the ADR model's `review_by` field.
    #[serde(default)]
    pub review_due: bool,
    /// Live forge state (issue/PR), attached only by the opt-in `--forge`
    /// enrichment; omitted from JSON when absent so the contract is unchanged
    /// for non-forge surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forge_data: Option<ForgeData>,
}

/// Live forge state for a row, attached by `--forge` enrichment. Always
/// compiled (feature-independent view contract); populated from adroit's
/// `forge` adapters when the feature is built in and enrichment is requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ForgeData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_approvals: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_ci: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_merged: Option<bool>,
    /// The PR/MR is **closed without merging** (distinct from `pr_merged`); lets a
    /// row show `PR closed` instead of an open PR's approvals/CI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_closed: Option<bool>,
    /// The linked tracker issue's lifecycle — `"open"` / `"closed"` (the tracker's
    /// native state), attached read-only by `list --forge` + the dashboard. A
    /// non-mutating probe of the `read_refs` → resolve path for split trackers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_state: Option<String>,
}

impl ForgeData {
    /// Compact, color-free status fragments for a `list --forge` row or a panel —
    /// e.g. `["PR merged", "issue closed"]`. A PR and a tracker issue are
    /// independent, so a split setup (forge PR + tracker issue) yields **both**.
    pub fn status_parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.pr_url.is_some() {
            let state = if self.pr_merged == Some(true) {
                "merged".to_string()
            } else if self.pr_closed == Some(true) {
                "closed".to_string()
            } else {
                match (self.pr_approvals, &self.pr_ci) {
                    (Some(a), Some(ci)) => format!("{a} approvals, ci {ci}"),
                    _ => "open".to_string(),
                }
            };
            parts.push(format!("PR {state}"));
        }
        if self.issue_url.is_some() {
            let state = self.issue_state.as_deref().unwrap_or("linked");
            parts.push(format!("issue {state}"));
        }
        parts
    }
}

/// Full detail for a single ADR: the summary fields plus the raw markdown body
/// and resolved related links.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct AdrDetail {
    /// The list-row summary for this ADR (flattened so JSON callers get the
    /// summary fields at the top level alongside the body).
    #[serde(flatten)]
    pub summary: AdrSummary,
    /// Raw markdown body (everything after the H1 / frontmatter). Not rendered.
    pub body: String,
    /// Rendered HTML body. `None` unless a web surface filled it server-side;
    /// present in the contract so that surface needs no shape change.
    pub body_html: Option<String>,
    /// The implementation plan persisted in the document (the
    /// `<!-- adroit:plan -->`-marked `## Implementation` section, adroit
    /// ADR-0008), without the heading/markers. `None` when no plan is stored.
    /// Additive: the section also remains part of `body` verbatim.
    pub plan: Option<String>,
    /// Other ADRs this one links to, resolved from supersession fields and
    /// markdown links in the body.
    #[serde(default)]
    pub related: Vec<RelatedLink>,
    /// Most recent commit date touching this ADR, as an RFC 3339 string
    /// (`None` when the date is unknown — e.g. an untracked file).
    pub last_modified: Option<String>,
}

/// A resolved link from one ADR to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RelatedLink {
    /// The target ADR's display reference (e.g. `"ADR-0006"` or a slug).
    pub reference: String,
    /// The target ADR's addressing token (for routing/links).
    pub address: String,
    /// The kind of relationship.
    pub kind: EdgeKind,
}

/// The kind of relationship an edge / link represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// `from` supersedes `to` (`from` is the newer decision). Directed.
    Supersedes,
    /// `from` depends on `to` (a typed relational link). Directed.
    DependsOn,
    /// `from` refines / elaborates `to` (a typed relational link). Directed.
    Refines,
    /// `from` is related to `to` (a typed, non-directional relational link).
    RelatesTo,
    /// `from` links to `to` via a markdown link in its body (non-supersession).
    Related,
}

/// The `-o json` shape of `adroit plan`: the implementation plan (markdown body)
/// tagged with the ADR it's for, so a downstream agent (the Adopt-stage engine)
/// can route the plan to the right decision without re-deriving identity. The
/// `plan` text stays markdown — the model writes prose, and adroit doesn't pretend
/// to parse it into structured steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Plan {
    /// The ADR's display reference (e.g. `ADR-0009`).
    pub reference: String,
    /// The ADR's title.
    pub title: String,
    /// The implementation plan, as markdown (the model's output).
    pub plan: String,
    /// `true` when `plan` is the one persisted in the ADR document (the
    /// `<!-- adroit:plan -->`-marked `## Implementation` section, adroit
    /// ADR-0008) — a deterministic, provider-free read, or just written by
    /// `--save`. `false` for a fresh unsaved generation (nondeterministic,
    /// provider-backed). Additive in `manifest_schema` 1; consumers may
    /// ignore it, and its absence in older envelopes reads as `false`.
    #[serde(default)]
    pub stored: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&Status::Accepted).unwrap(),
            "\"accepted\""
        );
    }

    #[test]
    fn summary_roundtrips_and_tolerates_absent_additive_fields() {
        // A consumer reading a minimal (schema-1-era) row: additive fields
        // default instead of failing the parse.
        let row: AdrSummary = serde_json::from_str(
            r#"{"number": 3, "reference": "ADR-0003", "address": "3",
                "title": "t", "status": "accepted", "created": null,
                "superseded_by": null}"#,
        )
        .unwrap();
        assert_eq!(row.status, Status::Accepted);
        assert!(!row.review_due);
        assert!(row.supersedes.is_empty());
        assert!(row.forge_data.is_none());
    }

    #[test]
    fn detail_flattens_summary_fields_to_the_top_level() {
        let detail: AdrDetail = serde_json::from_str(
            r###"{"number": 3, "number_display": "0003", "reference": "ADR-0003",
                "address": "3", "title": "t", "status": "accepted",
                "created": null, "supersedes": [], "superseded_by": null,
                "review_due": false, "body": "## Context\n", "body_html": null,
                "plan": null, "related": [], "last_modified": null}"###,
        )
        .unwrap();
        assert_eq!(detail.summary.reference, "ADR-0003");
        let back = serde_json::to_value(&detail).unwrap();
        assert_eq!(back["reference"], "ADR-0003", "summary stays flattened");
        assert_eq!(back["body"], "## Context\n");
    }

    #[test]
    fn plan_stored_defaults_false_for_older_envelopes() {
        let p: Plan =
            serde_json::from_str(r#"{"reference": "ADR-0003", "title": "t", "plan": "steps\n"}"#)
                .unwrap();
        assert!(!p.stored);
    }
}

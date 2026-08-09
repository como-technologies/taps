//! The one command surface, defined once.
//!
//! Each verb's parameters are a struct deriving both `clap::Args` (the
//! terminal surface) and `schemars::JsonSchema` + `serde::Deserialize` (the
//! MCP tool surface) — the `///` doc comments are simultaneously `--help`
//! text and tool-parameter descriptions. Core functions take these structs
//! plus a [`PageStore`] and return structured reports; the clap dispatch
//! prints them, the MCP wrappers ([`crate::mcp`]) return them verbatim. One
//! definition, three doors: human, automation (`-o json`), agent.
//!
//! Prescribing is judgment: `new`'s title, context, and body come from the
//! caller — an agent reading a gap report, or a human at the terminal.
//! adroit never parses another tool's shapes; provenance is recorded as
//! graph edges (`--relates`), which the engine's `broken-link` lint keeps
//! honest.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::lifecycle;
use crate::lint::{self, Severity};
use crate::naming::NamingScheme;
use crate::page::{self, Decision, Status};
use crate::plan;
use crate::store::{Corpus, DECISIONS_ROOT, Entry, PageStore};

// ── Parameters ────────────────────────────────────────────────────────────────

/// Parameters for `new`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NewParams {
    /// Short title describing the decision
    pub title: String,
    /// One-line scope, stored as the page's `summary`
    #[arg(long)]
    pub summary: Option<String>,
    /// Markdown body — the caller's judgment: context, options, outcome
    #[arg(long, conflicts_with = "body_file")]
    pub body: Option<String>,
    /// Read the markdown body from a file instead
    #[arg(long, value_name = "PATH")]
    pub body_file: Option<PathBuf>,
    /// Slug or id of a page this decision came from (repeatable) — recorded
    /// as a relates-to graph edge; the engine's broken-link lint enforces
    /// the target is real
    #[arg(long = "relates", value_name = "PAGE")]
    #[serde(default)]
    pub relates: Vec<String>,
}

/// Parameters for `list`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListParams {
    /// Only decisions with this status (proposed, accepted, rejected,
    /// deprecated, superseded)
    #[arg(long)]
    pub status: Option<String>,
}

/// Parameters for `show`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ShowParams {
    /// Decision identifier: a reference (ADR-0007, 7), a slug, or a page id
    pub id: String,
}

/// Parameters for `lint`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct LintParams {
    /// Decision identifier: a reference (ADR-0007, 7), a slug, or a page id
    pub id: String,
}

/// Parameters for `edit`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EditParams {
    /// Decision identifier: a reference (ADR-0007, 7), a slug, or a page id
    pub id: String,
    /// The replacement markdown body — the whole body, not a patch
    #[arg(long, conflicts_with = "body_file")]
    pub body: Option<String>,
    /// Read the replacement body from a file instead
    #[arg(long, value_name = "PATH")]
    pub body_file: Option<PathBuf>,
}

/// Parameters for `set-status`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetStatusParams {
    /// Decision identifier: a reference (ADR-0007, 7), a slug, or a page id
    pub id: String,
    /// New status: proposed → accepted/rejected; accepted → deprecated
    /// (superseded goes through `supersede`)
    pub status: String,
}

/// Parameters for `set-review`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetReviewParams {
    /// Decision identifier: a reference (ADR-0007, 7), a slug, or a page id
    pub id: String,
    /// Review deadline as YYYY-MM-DD (omit together with --clear)
    #[arg(required_unless_present = "clear")]
    pub date: Option<String>,
    /// Remove the review deadline instead of setting one
    #[arg(long, conflicts_with = "date")]
    #[serde(default)]
    pub clear: bool,
}

/// Parameters for `supersede`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SupersedeParams {
    /// The new (superseding) decision's identifier
    pub new: String,
    /// The old (superseded) decision's identifier
    pub old: String,
}

/// Parameters for `plan`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PlanParams {
    /// Decision identifier: a reference (ADR-0007, 7), a slug, or a page id
    pub id: String,
    /// Save a plan into the page instead of reading the stored one
    #[arg(long, requires = "plan_source")]
    #[serde(default)]
    pub save: bool,
    /// The plan text to save (markdown)
    #[arg(long, group = "plan_source", conflicts_with = "file")]
    pub text: Option<String>,
    /// Read the plan text from a file instead
    #[arg(long, group = "plan_source", value_name = "PATH")]
    pub file: Option<PathBuf>,
    /// With --save: overwrite an existing stored plan or replace a
    /// hand-written `## Implementation` section
    #[arg(long)]
    #[serde(default)]
    pub force: bool,
}

// ── Core functions ────────────────────────────────────────────────────────────
//
// Every function returns a `serde_json::Value` report. Where a shape is a
// documented contract (`plan`'s envelope is `como_contract::adroit::Plan` —
// what the Adopt stage reads), the keys are that contract — change them in
// lockstep with the consumers or not at all.

/// `new`: create a proposed decision page — schemas registered on first
/// contact, reference allocated by the naming default, admitted through the
/// engine's gates.
pub async fn new_core(store: &dyn PageStore, naming: NamingScheme, p: &NewParams) -> Result<Value> {
    let schemas = store.ensure_schemas().await?;
    let corpus = Corpus::load(store).await?;

    let mut decision = Decision::new(p.title.clone())?;
    decision.body = match (&p.body, &p.body_file) {
        (Some(text), _) => text.clone(),
        (None, Some(path)) => std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?,
        (None, None) => String::new(),
    };
    decision.relates_to = p.relates.clone();
    if let Some(summary) = &p.summary {
        decision.extra.insert(
            serde_yaml_ng::Value::String("summary".into()),
            serde_yaml_ng::Value::String(summary.clone()),
        );
    }

    let today = OffsetDateTime::now_utc().date();
    let reference = naming.assign(&corpus.refs(), &p.title, today, &decision.id.slug());
    decision.reference = Some(reference.clone());
    let slug = format!("{DECISIONS_ROOT}/{}", naming.stem(&reference, &p.title));
    if corpus.entries.iter().any(|e| e.slug == slug) {
        bail!("page {slug} already exists — pick a different title");
    }

    store.write(&slug, &page::serialize(&decision)?).await?;
    let ingest = store.ingest().await?;
    Ok(json!({
        "slug": slug,
        "reference": page::display_reference(&reference),
        "id": decision.id.to_string(),
        "title": decision.title,
        "status": decision.status,
        "schemas": schemas,
        "ingest": ingest,
    }))
}

/// `list`: the corpus as rows, reference-sorted.
pub async fn list_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &ListParams,
) -> Result<Value> {
    let filter = p.status.as_deref().map(parse_status).transpose()?;
    let corpus = Corpus::load(store).await?;
    let today = OffsetDateTime::now_utc().date();
    let rows: Vec<Value> = corpus
        .entries
        .iter()
        .filter(|e| filter.is_none_or(|s| e.decision.status == s))
        .map(|e| {
            json!({
                "reference": e.reference(),
                "slug": e.slug,
                "id": e.decision.id.to_string(),
                "title": e.decision.title,
                "status": e.decision.status,
                "created": e.decision.created.to_string(),
                "review_by": e.decision.review_by.map(|r| r.to_string()),
                "review_due": e.decision.status == Status::Proposed
                    && e.decision.review_by.is_some_and(|r| r.get() <= today),
                "superseded_by": e.decision.superseded_by,
            })
        })
        .collect();
    let _ = naming; // resolution-free read; the scheme shaped the stored references
    Ok(json!({ "decisions": rows }))
}

/// `show`: one decision in full, reference-resolved.
pub async fn show_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &ShowParams,
) -> Result<Value> {
    let corpus = Corpus::load(store).await?;
    let e = corpus.resolve(naming, &p.id)?;
    let d = &e.decision;
    Ok(json!({
        "reference": e.reference(),
        "slug": e.slug,
        "id": d.id.to_string(),
        "title": d.title,
        "status": d.status,
        "created": d.created.to_string(),
        "review_by": d.review_by.map(|r| r.to_string()),
        "supersedes": d.supersedes,
        "superseded_by": d.superseded_by,
        "relates_to": d.relates_to,
        "depends_on": d.depends_on,
        "refines": d.refines,
        "body": d.body,
        "plan_stored": plan::extract(&d.body).is_some(),
    }))
}

/// `lint`: the mechanical authoring-quality gate on one decision's body.
pub async fn lint_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &LintParams,
) -> Result<Value> {
    let corpus = Corpus::load(store).await?;
    let e = corpus.resolve(naming, &p.id)?;
    let findings = lint::lint(&e.decision.body);
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings.len() - errors;
    Ok(json!({
        "reference": e.reference(),
        "slug": e.slug,
        "findings": findings,
        "errors": errors,
        "warnings": warnings,
    }))
}

/// `edit`: replace a **proposed** decision's body in place — the refine
/// seat (#93: the walk found refine had no door). The frontmatter
/// round-trip preserves every field, including keys adroit doesn't own.
/// Decided records don't get edited — supersede them.
pub async fn edit_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &EditParams,
) -> Result<Value> {
    let body = match (&p.body, &p.body_file) {
        (Some(text), _) => text.clone(),
        (None, Some(path)) => std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?,
        (None, None) => bail!("provide the replacement body with --body or --body-file"),
    };
    store.ensure_schemas().await?;
    let corpus = Corpus::load(store).await?;
    let e = corpus.resolve(naming, &p.id)?.clone();
    if e.decision.status != Status::Proposed {
        bail!(
            "{} is {} — only proposed decisions are refined in place; \
             a decided record is replaced with `supersede`",
            e.reference(),
            e.decision.status
        );
    }
    let reference = e.reference();
    let mut d = e.decision;
    d.body = body;
    store.write(&e.slug, &page::serialize(&d)?).await?;
    let ingest = store.ingest().await?;
    Ok(json!({
        "reference": reference,
        "slug": e.slug,
        "status": d.status,
        "ingest": ingest,
    }))
}

/// `set-status`: a lifecycle transition, rewritten in place (the round-trip
/// preserves every frontmatter key adroit doesn't own).
pub async fn set_status_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &SetStatusParams,
) -> Result<Value> {
    let to = parse_status(&p.status)?;
    store.ensure_schemas().await?;
    let corpus = Corpus::load(store).await?;
    let e = corpus.resolve(naming, &p.id)?.clone();
    let from = e.decision.status;
    lifecycle::check_transition(from, to)?;
    let changed = from != to;
    let mut report = json!({
        "reference": e.reference(),
        "slug": e.slug,
        "from": from,
        "to": to,
        "changed": changed,
    });
    if changed {
        let mut d = e.decision;
        d.status = to;
        store.write(&e.slug, &page::serialize(&d)?).await?;
        report["ingest"] = store.ingest().await?;
    }
    Ok(report)
}

/// `set-review`: set or clear the review-by date, in place.
pub async fn set_review_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &SetReviewParams,
) -> Result<Value> {
    let review_by = match (&p.date, p.clear) {
        (None, true) => None,
        (Some(date), false) => Some(date.parse::<page::ReviewBy>()?),
        (Some(_), true) => bail!("--clear conflicts with a date"),
        (None, false) => bail!("pass a YYYY-MM-DD date, or --clear to remove the deadline"),
    };
    store.ensure_schemas().await?;
    let corpus = Corpus::load(store).await?;
    let e = corpus.resolve(naming, &p.id)?.clone();
    let reference = e.reference();
    let mut d = e.decision;
    d.review_by = review_by;
    store.write(&e.slug, &page::serialize(&d)?).await?;
    let ingest = store.ingest().await?;
    Ok(json!({
        "reference": reference,
        "slug": e.slug,
        "review_by": review_by.map(|r| r.to_string()),
        "ingest": ingest,
    }))
}

/// `supersede`: link `new` over `old` — both sides' frontmatter in one
/// stroke (`old.superseded_by` / `new.supersedes`, by page id), statuses
/// consistent (`old` becomes `superseded`).
pub async fn supersede_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &SupersedeParams,
) -> Result<Value> {
    store.ensure_schemas().await?;
    let corpus = Corpus::load(store).await?;
    let new_e = corpus.resolve(naming, &p.new)?.clone();
    let old_e = corpus.resolve(naming, &p.old)?.clone();
    if new_e.slug == old_e.slug {
        bail!("a decision cannot supersede itself ({})", new_e.reference());
    }

    let (new_ref, old_ref) = (new_e.reference(), old_e.reference());
    let mut old_d = old_e.decision;
    old_d.status = Status::Superseded;
    old_d.superseded_by = Some(new_e.decision.id.to_string());
    let mut new_d = new_e.decision;
    new_d.supersedes = Some(old_d.id.to_string());

    store.write(&old_e.slug, &page::serialize(&old_d)?).await?;
    store.write(&new_e.slug, &page::serialize(&new_d)?).await?;
    let ingest = store.ingest().await?;
    Ok(json!({
        "new": { "reference": new_ref, "slug": new_e.slug, "supersedes": new_d.supersedes },
        "old": { "reference": old_ref, "slug": old_e.slug, "status": Status::Superseded,
                 "superseded_by": old_d.superseded_by },
        "ingest": ingest,
    }))
}

/// `plan`: the stored-plan contract. Reads are provider-free and
/// deterministic; `--save` splices the marker-bracketed `## Implementation`
/// section in place. The report is exactly the pinned
/// [`como_contract::adroit::Plan`] envelope: `{reference, title, plan,
/// stored}`.
pub async fn plan_core(
    store: &dyn PageStore,
    naming: NamingScheme,
    p: &PlanParams,
) -> Result<Value> {
    let corpus = Corpus::load(store).await?;
    let e = corpus.resolve(naming, &p.id)?.clone();
    let envelope = if p.save {
        let text = match (&p.text, &p.file) {
            (Some(text), _) => text.clone(),
            (None, Some(path)) => std::fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?,
            (None, None) => bail!("--save needs the plan: pass --text or --file"),
        };
        if plan::has_hand_written_section(&e.decision.body) && !p.force {
            bail!(
                "{} has a hand-written `## Implementation` section — adroit never \
                 overwrites or shadows it (pass --force to replace it)",
                e.reference()
            );
        }
        if plan::extract(&e.decision.body).is_some() && !p.force {
            bail!(
                "{} already has a stored plan — overwrite it with --force",
                e.reference()
            );
        }
        let reference = e.reference();
        let mut d = e.decision;
        d.body = plan::splice(&d.body, &text);
        store.ensure_schemas().await?;
        store.write(&e.slug, &page::serialize(&d)?).await?;
        store.ingest().await?;
        como_contract::adroit::Plan {
            reference,
            title: d.title,
            plan: text.trim().to_string(),
            stored: true,
        }
    } else {
        let Some(text) = plan::extract(&e.decision.body) else {
            bail!(
                "{} has no stored plan — author one and save it with `plan {} --save --text …`",
                e.reference(),
                e.reference()
            );
        };
        como_contract::adroit::Plan {
            reference: e.reference(),
            title: e.decision.title,
            plan: text.to_string(),
            stored: true,
        }
    };
    Ok(serde_json::to_value(envelope)?)
}

/// `check`: corpus invariants — supersession integrity and duplicate
/// identities. Link integrity (`relates_to` targets and friends) is the
/// engine's job (`wiki_lint broken-link`), not re-implemented here.
pub async fn check_core(store: &dyn PageStore, naming: NamingScheme) -> Result<Value> {
    let corpus = Corpus::load(store).await?;
    let mut problems: Vec<Value> = Vec::new();
    let mut problem = |severity: Severity, subject: &str, message: String| {
        problems.push(json!({
            "severity": severity,
            "subject": subject,
            "message": message,
        }));
    };

    // Duplicate identities: reference and id are errors (two pages claiming
    // one identity), a repeated title is advisory (often an accidental
    // re-`new`).
    duplicates(
        &corpus,
        |e| Some(e.reference()),
        |subject, slugs| {
            problem(
                Severity::Error,
                subject,
                format!("duplicate reference used by {slugs}"),
            );
        },
    );
    duplicates(
        &corpus,
        |e| Some(e.decision.id.to_string()),
        |subject, slugs| {
            problem(
                Severity::Error,
                subject,
                format!("duplicate page id used by {slugs}"),
            );
        },
    );
    duplicates(
        &corpus,
        |e| Some(e.decision.title.trim().to_lowercase()),
        |_, slugs| {
            problem(
                Severity::Warning,
                "title",
                format!("duplicate title used by {slugs}"),
            );
        },
    );

    // Supersession integrity: every ref resolves, statuses agree, and the
    // two sides point at each other.
    for e in &corpus.entries {
        let d = &e.decision;
        let reference = e.reference();
        if let Some(target) = &d.superseded_by {
            match corpus.entries.iter().find(|c| c.is_target_of(target)) {
                None => problem(
                    Severity::Error,
                    &reference,
                    format!("superseded_by {target} does not resolve to any decision"),
                ),
                Some(newer) => {
                    if !newer
                        .decision
                        .supersedes
                        .as_deref()
                        .is_some_and(|back| e.is_target_of(back))
                    {
                        problem(
                            Severity::Warning,
                            &reference,
                            format!(
                                "superseded by {} but that page's supersedes does not point back",
                                newer.reference()
                            ),
                        );
                    }
                }
            }
            if d.status != Status::Superseded {
                problem(
                    Severity::Error,
                    &reference,
                    format!("superseded_by is set but status is {}", d.status),
                );
            }
        } else if d.status == Status::Superseded {
            problem(
                Severity::Error,
                &reference,
                "status is superseded but superseded_by is missing".to_string(),
            );
        }
        if let Some(target) = &d.supersedes {
            match corpus.entries.iter().find(|c| c.is_target_of(target)) {
                None => problem(
                    Severity::Error,
                    &reference,
                    format!("supersedes {target} does not resolve to any decision"),
                ),
                Some(older) if older.decision.status != Status::Superseded => problem(
                    Severity::Warning,
                    &reference,
                    format!(
                        "supersedes {} but its status is {}, not superseded",
                        older.reference(),
                        older.decision.status
                    ),
                ),
                Some(_) => {}
            }
        }
    }

    let errors = problems.iter().filter(|p| p["severity"] == "error").count();
    let warnings = problems.len() - errors;
    let _ = naming;
    Ok(json!({
        "decisions": corpus.entries.len(),
        "problems": problems,
        "errors": errors,
        "warnings": warnings,
    }))
}

/// Group entries by a key and report groups larger than one as `subject` +
/// a comma-separated slug list.
fn duplicates(
    corpus: &Corpus,
    key: impl Fn(&Entry) -> Option<String>,
    mut report: impl FnMut(&str, &str),
) {
    let mut by_key: std::collections::BTreeMap<String, Vec<&str>> = Default::default();
    for e in &corpus.entries {
        if let Some(k) = key(e) {
            by_key.entry(k).or_default().push(&e.slug);
        }
    }
    for (k, slugs) in by_key {
        if slugs.len() > 1 {
            report(&k, &slugs.join(", "));
        }
    }
}

/// Parse a status word (case-insensitive) with the valid set in the error.
fn parse_status(s: &str) -> Result<Status> {
    s.trim().parse::<Status>().map_err(|_| {
        anyhow::anyhow!(
            "unknown status '{s}' (expected proposed, accepted, rejected, deprecated, \
             or superseded)"
        )
    })
}

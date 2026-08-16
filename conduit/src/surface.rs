//! The one work-item command surface, defined once (the #85 pattern).
//!
//! Each verb's parameters derive both `clap::Args` (the terminal door) and
//! `schemars::JsonSchema` + `serde::Deserialize` (the MCP door) — the `///`
//! doc comments are simultaneously `--help` text and tool descriptions.
//! Core functions take the params plus a [`WorkStore`] and an
//! [`Actor`], and return structured reports.
//!
//! **Actor honesty is the doors' contract**: the terminal passes
//! `Actor::HumanSeat`, the MCP door passes `Actor::Harness`, and
//! `Actor::MergeDoor` is never accepted from a caller — `complete` runs its
//! transition under it internally, after its own mechanical checks. On top
//! of that, `signoff` is not exposed as an MCP tool at all: sign-off is not
//! merely refused to the harness, it is physically absent from its door.
//!
//! Every verb that acts on a signed item checks the seal first; a broken
//! seal is never worked around — the door executes the bounce (strip +
//! draft, cascading downhill) and refuses the requested action.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::approval::{self, SealState};
use crate::item::{Item, Loaded, WORK_ROOT, WorkCorpus, class_name, stem};
use crate::work::{WorkStore, load_corpus};
use crate::workitem::{Actor, Class, Status, check_transition};

/// The gate command a project runs at the merge door when its frontmatter
/// doesn't name one.
pub const DEFAULT_GATE: &str = "just ci";

// ── Parameters ─────────────────────────────────────────────────────────────

/// Parameters for `new`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NewParams {
    /// Work-item class: project, story, or task
    pub class: Class,
    /// Short title stating the goal at this altitude
    pub title: String,
    /// Parent item (slug or id): the project for a story, the story for a
    /// task. Required for story/task, refused for a project.
    #[arg(long)]
    pub parent: Option<String>,
    /// Markdown body — the contract: the goal and its verification form
    /// (executive terms / BDD scenarios / the test set)
    #[arg(long, conflicts_with = "body_file")]
    pub body: Option<String>,
    /// Read the markdown body from a file instead
    #[arg(long, value_name = "PATH")]
    pub body_file: Option<PathBuf>,
    /// Accepted decision (slug or id) this project implements (repeatable;
    /// projects only) — the provenance edge Measure walks
    #[arg(long = "implements", value_name = "DECISION")]
    #[serde(default)]
    pub implements: Vec<String>,
}

/// Parameters for `list`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListParams {
    /// Only items of this class
    #[arg(long)]
    pub class: Option<Class>,
    /// Only items with this status (draft, ready, in-progress, done,
    /// cancelled)
    #[arg(long)]
    pub status: Option<String>,
}

/// Parameters for `show`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ShowParams {
    /// Work-item identifier: a slug (with or without `work/`) or a page id
    pub id: String,
}

/// Parameters for `signoff`.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SignoffParams {
    /// Work-item identifier: a slug (with or without `work/`) or a page id
    pub id: String,
    /// Who signs (defaults to `git config user.email`)
    #[arg(long)]
    pub by: Option<String>,
}

/// Parameters for verbs that act on one item.
#[derive(clap::Args, Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IdParams {
    /// Work-item identifier: a slug (with or without `work/`) or a page id
    pub id: String,
}

// ── Reports plumbing ───────────────────────────────────────────────────────

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .expect("zero nanosecond is valid")
        .format(&Rfc3339)
        .expect("utc formats")
}

fn seal_word(text: &str) -> &'static str {
    match approval::check(text) {
        Ok(SealState::Intact(_)) => "intact",
        Ok(SealState::Broken { .. }) => "broken",
        Ok(SealState::Unsealed) => "unsealed",
        Err(_) => "malformed",
    }
}

fn row(e: &Loaded) -> Value {
    json!({
        "slug": e.slug,
        "id": e.item.id(),
        "class": e.item.class().map(class_name),
        "title": e.item.title(),
        "status": e.item.status(),
        "parent": e.item.parent_ref(),
        "seal": seal_word(&e.text),
    })
}

/// Rewrite one item's page (frontmatter mutation via `f`) and return the
/// new text. The body is untouched by construction.
fn rewrite(e: &Loaded, f: impl FnOnce(&mut Item)) -> Result<String> {
    let mut item = Item::parse(&e.slug, &e.text)?;
    f(&mut item);
    Ok(item.serialize()?)
}

/// Self plus every descendant, parents before children.
fn subtree<'a>(corpus: &'a WorkCorpus, root: &'a Loaded) -> Vec<&'a Loaded> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        let kids = corpus.children_of(out[i]);
        out.extend(kids);
        i += 1;
    }
    out
}

/// The forced bounce: a broken seal is never worked around. Bounce the item
/// (and its signed, non-terminal descendants), then refuse the caller's
/// verb with an error naming both hashes.
async fn bounce_broken(
    store: &dyn WorkStore,
    corpus: &WorkCorpus,
    e: &Loaded,
    expected: &str,
    actual: &str,
    verb: &str,
) -> Result<Value> {
    let bounced = bounce_subtree(store, corpus, e).await?;
    store.ingest().await?;
    bail!(
        "{verb} refused: the seal on {} is broken (signed {expected}, body now {actual}) — \
         the item was bounced to draft ({}); revise and re-sign at the human seat",
        e.slug,
        bounced.join(", "),
    )
}

/// Strip + draft on every ready/in-progress item in the subtree (the
/// downhill cascade). Returns the slugs bounced. Writes pages; the caller
/// ingests.
async fn bounce_subtree(
    store: &dyn WorkStore,
    corpus: &WorkCorpus,
    root: &Loaded,
) -> Result<Vec<String>> {
    let mut bounced = Vec::new();
    for e in subtree(corpus, root) {
        if e.item.status().is_some_and(Status::is_signed_open) {
            let stripped = approval::strip(&e.text)?;
            let item_text = {
                let mut item = Item::parse(&e.slug, &stripped)?;
                item.set_status(Status::Draft);
                item.serialize()?
            };
            store.write(&e.slug, &item_text).await?;
            bounced.push(e.slug.clone());
        }
    }
    Ok(bounced)
}

/// Require an intact seal or execute the forced bounce and refuse.
async fn require_intact(
    store: &dyn WorkStore,
    corpus: &WorkCorpus,
    e: &Loaded,
    verb: &str,
) -> Result<()> {
    match approval::check(&e.text)? {
        SealState::Intact(_) => Ok(()),
        SealState::Broken {
            seal,
            actual_sha256,
        } => {
            bounce_broken(store, corpus, e, &seal.content_sha256, &actual_sha256, verb).await?;
            unreachable!("bounce_broken always errors")
        }
        SealState::Unsealed => bail!(
            "{verb} refused: {} carries no approval seal — it needs sign-off at the human seat first",
            e.slug
        ),
    }
}

// ── Core functions ─────────────────────────────────────────────────────────

/// `new`: create a draft work item — schemas registered on first contact,
/// parentage resolved and class-checked, admitted through the engine's
/// gates. Drafting is open to every actor; it creates nothing executable.
pub async fn new_core(store: &dyn WorkStore, p: &NewParams) -> Result<Value> {
    let schemas = store.ensure_schemas().await?;
    let corpus = load_corpus(store).await?;

    let parent_id = match (p.class.parent(), &p.parent) {
        (None, None) => None,
        (None, Some(_)) => bail!("a project has no parent — drop --parent"),
        (Some(parent_class), Some(target)) => {
            let parent = corpus.resolve(target)?;
            if parent.item.class() != Some(parent_class) {
                bail!(
                    "{} is a {}, but a {} hangs under a {}",
                    parent.slug,
                    parent.item.class().map(class_name).unwrap_or("?"),
                    class_name(p.class),
                    class_name(parent_class),
                );
            }
            Some(parent.item.id().unwrap_or(&parent.slug).to_string())
        }
        (Some(parent_class), None) => bail!(
            "a {} needs --parent naming its {}",
            class_name(p.class),
            class_name(parent_class)
        ),
    };

    let body = match (&p.body, &p.body_file) {
        (Some(text), _) => text.clone(),
        (None, Some(path)) => std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?,
        (None, None) => String::new(),
    };
    if !p.implements.is_empty() && p.class != Class::Project {
        bail!(
            "--implements records a project's decisions — drop it on a {}",
            class_name(p.class)
        );
    }

    let mut item = Item::new(p.class, &p.title, &now_rfc3339(), &body);
    match p.class {
        Class::Project => {}
        Class::Story => item.set("project", parent_id.clone().unwrap()),
        Class::Task => item.set("story", parent_id.clone().unwrap()),
    }
    if !p.implements.is_empty() {
        item.set(
            "implements",
            serde_yaml_ng::Value::Sequence(
                p.implements
                    .iter()
                    .map(|s| serde_yaml_ng::Value::String(s.clone()))
                    .collect(),
            ),
        );
    }

    let slug = format!("{WORK_ROOT}/{}", stem(p.class, &p.title));
    if corpus.entries.iter().any(|e| e.slug == slug) {
        bail!("page {slug} already exists — pick a different title");
    }
    store.write(&slug, &item.serialize()?).await?;
    let ingest = store.ingest().await?;
    Ok(json!({
        "slug": slug,
        "id": item.id(),
        "class": class_name(p.class),
        "title": p.title,
        "status": "draft",
        "parent": parent_id,
        "schemas": schemas,
        "ingest": ingest,
    }))
}

/// `list`: the work tree as rows with seal states.
pub async fn list_core(store: &dyn WorkStore, p: &ListParams) -> Result<Value> {
    let status = p.status.as_deref().map(parse_status).transpose()?;
    let corpus = load_corpus(store).await?;
    let rows: Vec<Value> = corpus
        .entries
        .iter()
        .filter(|e| p.class.is_none_or(|c| e.item.class() == Some(c)))
        .filter(|e| status.is_none_or(|s| e.item.status() == Some(s)))
        .map(row)
        .collect();
    Ok(json!({ "items": rows }))
}

/// `show`: one item in full — frontmatter, body, seal state, children.
pub async fn show_core(store: &dyn WorkStore, p: &ShowParams) -> Result<Value> {
    let corpus = load_corpus(store).await?;
    let e = corpus.resolve(&p.id)?;
    let children: Vec<Value> = corpus.children_of(e).iter().map(|c| row(c)).collect();
    let mut report = row(e);
    report["body"] = Value::String(e.item.body.clone());
    report["frontmatter"] = serde_json::to_value(&e.item.fm)?;
    report["children"] = Value::Array(children);
    Ok(report)
}

/// `signoff`: the human seat seals the contract and the item becomes
/// `ready`. Sign-off flows downhill — the parent must be signed first.
pub async fn signoff_core(store: &dyn WorkStore, actor: Actor, p: &SignoffParams) -> Result<Value> {
    store.ensure_schemas().await?;
    let corpus = load_corpus(store).await?;
    let e = corpus.resolve(&p.id)?;
    let (class, from) = class_and_status(e)?;
    let parent = corpus.parent_of(e).and_then(|pe| pe.item.status());
    if class.parent().is_some() && corpus.parent_of(e).is_none() {
        bail!(
            "{}: parent reference does not resolve — fix the tree first",
            e.slug
        );
    }
    check_transition(class, actor, from, Status::Ready, parent, &[])?;

    let by = match &p.by {
        Some(by) => by.clone(),
        None => default_signer().ok_or_else(|| {
            anyhow::anyhow!("no signer: pass --by, or set `git config user.email`")
        })?,
    };
    let sealed = approval::seal(&e.text, &by, &now_rfc3339())?;
    let text = {
        let mut item = Item::parse(&e.slug, &sealed)?;
        item.set_status(Status::Ready);
        item.serialize()?
    };
    store.write(&e.slug, &text).await?;
    let ingest = store.ingest().await?;
    let sha = approval::body_sha256(&text)?;
    Ok(json!({
        "slug": e.slug,
        "status": "ready",
        "by": by,
        "content_sha256": sha,
        "ingest": ingest,
    }))
}

/// `bounce`: reopen a signed item — strip the seal, back to draft, cascading
/// downhill (nothing stays executable under an unsigned parent). Deliberately
/// unprivileged: a bounce can only destroy an approval, never grant one.
pub async fn bounce_core(store: &dyn WorkStore, actor: Actor, p: &IdParams) -> Result<Value> {
    let corpus = load_corpus(store).await?;
    let e = corpus.resolve(&p.id)?;
    let (class, from) = class_and_status(e)?;
    check_transition(class, actor, from, Status::Draft, None, &[])?;
    let bounced = bounce_subtree(store, &corpus, e).await?;
    let ingest = store.ingest().await?;
    Ok(json!({ "bounced": bounced, "ingest": ingest }))
}

/// `claim`: an execution session takes a ready task — the internal repo and
/// branch are provisioned, the task goes `in-progress`.
pub async fn claim_core(
    store: &dyn WorkStore,
    dir: &Path,
    actor: Actor,
    p: &IdParams,
) -> Result<Value> {
    let corpus = load_corpus(store).await?;
    let e = corpus.resolve(&p.id)?;
    let (class, from) = class_and_status(e)?;
    if class != Class::Task {
        bail!("claim takes a task — {} is a {}", e.slug, class_name(class));
    }
    require_intact(store, &corpus, e, "claim").await?;
    let parent = corpus.parent_of(e).and_then(|pe| pe.item.status());
    check_transition(class, actor, from, Status::InProgress, parent, &[])?;

    let project = project_of(&corpus, e)?;
    let project_stem = bare_stem(&project.slug);
    let repo = crate::repo::ensure_repo(dir, project_stem)?;
    let branch = crate::repo::ensure_branch(&repo, bare_stem(&e.slug))?;

    // Record the repo on the project the first time it's provisioned
    // (frontmatter state — door-written, seal-safe).
    if project.item.fm.get("repo").is_none() {
        let repo_rel = format!(".conduit/repos/{project_stem}.git");
        let text = rewrite(project, |i| i.set("repo", repo_rel.as_str()))?;
        store.write(&project.slug, &text).await?;
    }

    let text = rewrite(e, |i| {
        i.set("branch", branch.as_str());
        i.set("claimed_at", now_rfc3339().as_str());
        i.set_status(Status::InProgress);
    })?;
    store.write(&e.slug, &text).await?;
    let ingest = store.ingest().await?;
    Ok(json!({
        "slug": e.slug,
        "status": "in-progress",
        "repo": repo.to_string_lossy(),
        "branch": branch,
        "hint": format!("git clone -b {branch} {} <workspace>", repo.to_string_lossy()),
        "ingest": ingest,
    }))
}

/// `complete`: the mechanical merge door. Anyone may knock; only the checks
/// decide — seal intact, gate green on the branch, then ONE squash commit
/// onto main, telemetry written, task `done`. The transition runs under
/// `Actor::MergeDoor` regardless of who called: mechanical means the
/// caller's authority is irrelevant.
pub async fn complete_core(
    store: &dyn WorkStore,
    dir: &Path,
    gate_timeout: Duration,
    p: &IdParams,
) -> Result<Value> {
    let corpus = load_corpus(store).await?;
    let e = corpus.resolve(&p.id)?;
    let (class, from) = class_and_status(e)?;
    if class != Class::Task {
        bail!("complete takes a task — {} closes through `close`", e.slug);
    }
    require_intact(store, &corpus, e, "complete").await?;
    let parent = corpus.parent_of(e).and_then(|pe| pe.item.status());
    check_transition(class, Actor::MergeDoor, from, Status::Done, parent, &[])?;

    let project = project_of(&corpus, e)?;
    let repo = crate::repo::repo_path(dir, bare_stem(&project.slug));
    let Some(branch) = e
        .item
        .fm
        .get("branch")
        .and_then(serde_yaml_ng::Value::as_str)
    else {
        bail!("{} has no branch — was it claimed?", e.slug);
    };
    let gate = project
        .item
        .fm
        .get("gate")
        .and_then(serde_yaml_ng::Value::as_str)
        .unwrap_or(DEFAULT_GATE);

    let merged = crate::repo::merge_task(
        &repo,
        branch,
        e.item.title().unwrap_or(&e.slug),
        e.item.id().unwrap_or(&e.slug),
        gate,
        gate_timeout,
        &dir.join(".conduit").join("workspaces"),
    )?;

    let work_ms = e
        .item
        .fm
        .get("claimed_at")
        .and_then(serde_yaml_ng::Value::as_str)
        .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
        .map(|claimed| {
            let elapsed = OffsetDateTime::now_utc() - claimed;
            elapsed.whole_milliseconds().max(0) as u64
        })
        .unwrap_or(0);

    let text = rewrite(e, |i| {
        i.set("merge_commit", merged.merge_commit.as_str());
        i.set("work_ms", serde_yaml_ng::Value::Number(work_ms.into()));
        i.set_status(Status::Done);
    })?;
    store.write(&e.slug, &text).await?;
    let ingest = store.ingest().await?;
    Ok(json!({
        "slug": e.slug,
        "status": "done",
        "merge_commit": merged.merge_commit,
        "work_ms": work_ms,
        "gate": gate,
        "ingest": ingest,
    }))
}

/// `close`: done flows uphill as a precondition — a story/project closes
/// only when every child is terminal (and one landed); a project's close is
/// a human seat.
pub async fn close_core(store: &dyn WorkStore, actor: Actor, p: &IdParams) -> Result<Value> {
    let corpus = load_corpus(store).await?;
    let e = corpus.resolve(&p.id)?;
    let (class, from) = class_and_status(e)?;
    if class == Class::Task {
        bail!("a task is done through the merge door (`complete`), not `close`");
    }
    require_intact(store, &corpus, e, "close").await?;
    let parent = corpus.parent_of(e).and_then(|pe| pe.item.status());
    let children = corpus.children_statuses(e);
    check_transition(class, actor, from, Status::Done, parent, &children)?;
    let text = rewrite(e, |i| i.set_status(Status::Done))?;
    store.write(&e.slug, &text).await?;
    let ingest = store.ingest().await?;
    Ok(json!({ "slug": e.slug, "status": "done", "children": children, "ingest": ingest }))
}

/// `cancel`: abandon an item — cascading downhill so no orphaned child
/// stays claimable under a dead parent. Terminal items never move.
pub async fn cancel_core(store: &dyn WorkStore, actor: Actor, p: &IdParams) -> Result<Value> {
    let corpus = load_corpus(store).await?;
    let e = corpus.resolve(&p.id)?;
    let (class, from) = class_and_status(e)?;
    check_transition(class, actor, from, Status::Cancelled, None, &[])?;
    let mut cancelled = Vec::new();
    for t in subtree(&corpus, e) {
        if t.item.status().is_some_and(|s| !s.is_terminal()) {
            let text = rewrite(t, |i| i.set_status(Status::Cancelled))?;
            store.write(&t.slug, &text).await?;
            cancelled.push(t.slug.clone());
        }
    }
    let ingest = store.ingest().await?;
    Ok(json!({ "cancelled": cancelled, "ingest": ingest }))
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn class_and_status(e: &Loaded) -> Result<(Class, Status)> {
    match (e.item.class(), e.item.status()) {
        (Some(c), Some(s)) => Ok((c, s)),
        _ => bail!("{}: unreadable class/status frontmatter", e.slug),
    }
}

/// Walk task -> story -> project.
fn project_of<'a>(corpus: &'a WorkCorpus, task: &'a Loaded) -> Result<&'a Loaded> {
    let story = corpus
        .parent_of(task)
        .with_context(|| format!("{}: story reference does not resolve", task.slug))?;
    corpus
        .parent_of(story)
        .with_context(|| format!("{}: project reference does not resolve", story.slug))
}

/// The slug's stem without the `work/` prefix — repo and branch naming.
fn bare_stem(slug: &str) -> &str {
    slug.strip_prefix(&format!("{WORK_ROOT}/")).unwrap_or(slug)
}

fn parse_status(s: &str) -> Result<Status> {
    serde_yaml_ng::from_value(serde_yaml_ng::Value::String(s.trim().to_string())).map_err(|_| {
        anyhow::anyhow!(
            "unknown status '{s}' (expected draft, ready, in-progress, done, or cancelled)"
        )
    })
}

/// The terminal door's default signer: `git config user.email`.
fn default_signer() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["config", "user.email"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

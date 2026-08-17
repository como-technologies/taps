//! The work-item doors end to end, against an in-memory store and real
//! internal git repos in a tempdir. The KB transport is the one seam faked;
//! everything else — seal mechanics, lifecycle rules, repo provisioning,
//! the squash merge — is the real code.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

use conduit::item::Item;
use conduit::surface::{
    IdParams, ListParams, NewParams, ShowParams, SignoffParams, bounce_core, cancel_core,
    claim_core, close_core, complete_core, list_core, new_core, show_core, signoff_core,
};
use conduit::work::WorkStore;
use conduit::workitem::{Actor, Class};

// ── The faked seam ─────────────────────────────────────────────────────────

#[derive(Default)]
struct FakeStore {
    pages: Mutex<BTreeMap<String, String>>,
}

#[async_trait]
impl WorkStore for FakeStore {
    async fn ensure_schemas(&self) -> Result<Vec<String>> {
        Ok(vec!["project: registered".into()])
    }
    async fn slugs(&self, type_name: &str) -> Result<Vec<String>> {
        let pages = self.pages.lock().unwrap();
        Ok(pages
            .iter()
            .filter(|(_, text)| {
                Item::parse("x", text)
                    .ok()
                    .and_then(|i| i.class())
                    .map(conduit::item::class_name)
                    == Some(type_name)
            })
            .map(|(slug, _)| slug.clone())
            .collect())
    }
    async fn read(&self, slug: &str) -> Result<String> {
        self.pages
            .lock()
            .unwrap()
            .get(slug)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no page {slug}"))
    }
    async fn write(&self, slug: &str, content: &str) -> Result<()> {
        self.pages
            .lock()
            .unwrap()
            .insert(slug.to_string(), content.to_string());
        Ok(())
    }
    async fn ingest(&self) -> Result<serde_json::Value> {
        Ok(json!({"status": "ok"}))
    }
}

impl FakeStore {
    fn text(&self, slug: &str) -> String {
        self.pages.lock().unwrap().get(slug).unwrap().clone()
    }
    /// Frontmatter-only edit through the model (seal-safe by construction).
    fn edit_fm(&self, slug: &str, f: impl FnOnce(&mut Item)) {
        let text = self.text(slug);
        let mut item = Item::parse(slug, &text).unwrap();
        f(&mut item);
        self.pages
            .lock()
            .unwrap()
            .insert(slug.to_string(), item.serialize().unwrap());
    }
    /// A body edit — exactly what breaks a seal.
    fn tamper_body(&self, slug: &str, from: &str, to: &str) {
        let text = self.text(slug).replace(from, to);
        self.pages.lock().unwrap().insert(slug.to_string(), text);
    }
    fn status_of(&self, slug: &str) -> String {
        let text = self.text(slug);
        let item = Item::parse(slug, &text).unwrap();
        serde_yaml_ng::to_string(&item.status().unwrap())
            .unwrap()
            .trim()
            .to_string()
    }
}

// ── Scenario plumbing ──────────────────────────────────────────────────────

fn new_params(class: Class, title: &str, parent: Option<&str>) -> NewParams {
    NewParams {
        class,
        title: title.into(),
        parent: parent.map(String::from),
        body: Some(format!(
            "## Goal\n\n{title}\n\n## Test set\n\n- unit: it works\n"
        )),
        body_file: None,
        implements: vec![],
    }
}

fn id(s: &str) -> IdParams {
    IdParams { id: s.into() }
}

fn signer(s: &str) -> SignoffParams {
    SignoffParams {
        id: s.into(),
        by: Some("mike@thesandmans.com".into()),
    }
}

/// project → story → task, all draft.
async fn tree(store: &FakeStore) {
    new_core(store, &new_params(Class::Project, "P", None))
        .await
        .unwrap();
    new_core(store, &new_params(Class::Story, "S", Some("project-1")))
        .await
        .unwrap();
    new_core(store, &new_params(Class::Task, "T", Some("story-1")))
        .await
        .unwrap();
}

/// All three signed off, top down, at the human seat.
async fn signed_tree(store: &FakeStore) {
    tree(store).await;
    for slug in ["project-1", "story-1", "task-1"] {
        signoff_core(store, Actor::HumanSeat, &signer(slug))
            .await
            .unwrap();
    }
}

fn git_in(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .envs([
            ("GIT_AUTHOR_NAME", "t"),
            ("GIT_AUTHOR_EMAIL", "t@t"),
            ("GIT_COMMITTER_NAME", "t"),
            ("GIT_COMMITTER_EMAIL", "t@t"),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Stand-in for the execution session: clone the internal repo, commit a
/// change on the task branch, push.
fn push_work(dir: &Path, repo: &str, branch: &str) {
    let ws = dir.join("session-ws");
    git_in(dir, &["clone", "-q", repo, "session-ws"]);
    git_in(&ws, &["checkout", "-q", branch]);
    std::fs::write(ws.join("delivered.txt"), "the change\n").unwrap();
    git_in(&ws, &["add", "."]);
    git_in(&ws, &["commit", "-q", "-m", "wip"]);
    git_in(&ws, &["push", "-q", "origin", branch]);
    std::fs::remove_dir_all(&ws).unwrap();
}

// ── The happy path, whole ──────────────────────────────────────────────────

#[tokio::test]
async fn the_full_loop_lands_one_squash_commit_and_closes_uphill() {
    let store = FakeStore::default();
    let dir = tempfile::TempDir::new().unwrap();
    signed_tree(&store).await;

    // Claim: repo + branch provisioned, clock started.
    let claimed = claim_core(&store, dir.path(), Actor::Harness, &id("task-1"))
        .await
        .unwrap();
    assert_eq!(claimed["status"], "in-progress");
    assert_eq!(claimed["branch"], "work/task-1-t");
    let repo = claimed["repo"].as_str().unwrap().to_string();
    assert!(Path::new(&repo).join("HEAD").exists());
    let task = store.text("work/task-1-t");
    assert!(task.contains("claimed_at:"));
    assert!(
        store.text("work/project-1-p").contains("repo:"),
        "the project records its provisioned repo"
    );

    // The session does the work; the merge door proves the gate and lands
    // ONE commit (the project's gate is set to a trivially-green command —
    // the gate CONTENT is the signed test set's business, not this test's).
    store.edit_fm("work/project-1-p", |i| {
        i.set("gate", "test -f delivered.txt")
    });
    push_work(dir.path(), &repo, "work/task-1-t");
    let done = complete_core(&store, dir.path(), Duration::from_secs(60), &id("task-1"))
        .await
        .unwrap();
    assert_eq!(done["status"], "done");
    let sha = done["merge_commit"].as_str().unwrap();
    assert_eq!(sha.len(), 40);
    let task = store.text("work/task-1-t");
    assert!(task.contains(&format!("merge_commit: {sha}")));
    assert!(task.contains("work_ms:"));

    // The squash commit carries the provenance trailer.
    let log = Command::new("git")
        .args(["log", "-1", "--format=%B", sha])
        .current_dir(&repo)
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&log.stdout).to_string();
    assert!(body.starts_with('T'), "title first: {body}");
    assert!(body.contains("work-item: "), "trailer: {body}");

    // Done flows uphill: harness closes the story, only a human the project.
    close_core(&store, Actor::Harness, &id("story-1"))
        .await
        .unwrap();
    let refused = close_core(&store, Actor::Harness, &id("project-1"))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("human seat"), "{refused}");
    let closed = close_core(&store, Actor::HumanSeat, &id("project-1"))
        .await
        .unwrap();
    assert_eq!(closed["status"], "done");
}

// ── The gates, one by one ──────────────────────────────────────────────────

#[tokio::test]
async fn signoff_flows_downhill_and_is_a_human_seat() {
    let store = FakeStore::default();
    tree(&store).await;

    // Story before project: refused, naming the unsigned parent.
    let err = signoff_core(&store, Actor::HumanSeat, &signer("story-1"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("parent"), "{err}");

    // The harness cannot sign at all.
    signoff_core(&store, Actor::HumanSeat, &signer("project-1"))
        .await
        .unwrap();
    let err = signoff_core(&store, Actor::Harness, &signer("story-1"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("human seat"), "{err}");

    // Top-down succeeds, and the pages carry the seal + ready status —
    // exactly what the schemas' ready-requires-approval invariant expects.
    signoff_core(&store, Actor::HumanSeat, &signer("story-1"))
        .await
        .unwrap();
    let text = store.text("work/story-1-s");
    assert!(text.contains("approval:"));
    assert!(text.contains("content_sha256:"));
    assert!(text.contains("status: ready"));
}

#[tokio::test]
async fn a_tampered_contract_is_bounced_not_worked() {
    let store = FakeStore::default();
    let dir = tempfile::TempDir::new().unwrap();
    signed_tree(&store).await;

    // The body changes after sign-off — by anyone, however it happened.
    store.tamper_body("work/task-1-t", "it works", "it slacks");

    // The claim door refuses AND executes the bounce.
    let err = claim_core(&store, dir.path(), Actor::Harness, &id("task-1"))
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("seal") && msg.contains("bounced"), "{msg}");
    assert_eq!(store.status_of("work/task-1-t"), "draft");
    assert!(
        !store.text("work/task-1-t").contains("approval:"),
        "the broken seal is stripped, never overwritten"
    );

    // Nothing proceeds until a human re-signs.
    let err = claim_core(&store, dir.path(), Actor::Harness, &id("task-1"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("sign-off"), "{err}");
    signoff_core(&store, Actor::HumanSeat, &signer("task-1"))
        .await
        .unwrap();
    claim_core(&store, dir.path(), Actor::Harness, &id("task-1"))
        .await
        .unwrap();
}

#[tokio::test]
async fn bounce_and_cancel_cascade_downhill() {
    let store = FakeStore::default();
    signed_tree(&store).await;

    // Bouncing the story takes its signed task with it; the project stands.
    let report = bounce_core(&store, Actor::Harness, &id("story-1"))
        .await
        .unwrap();
    let bounced: Vec<&str> = report["bounced"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(bounced, vec!["work/story-1-s", "work/task-1-t"]);
    assert_eq!(store.status_of("work/story-1-s"), "draft");
    assert_eq!(store.status_of("work/task-1-t"), "draft");
    assert_eq!(store.status_of("work/project-1-p"), "ready");

    // Cancelling the project sweeps every non-terminal descendant.
    let report = cancel_core(&store, Actor::HumanSeat, &id("project-1"))
        .await
        .unwrap();
    assert_eq!(report["cancelled"].as_array().unwrap().len(), 3);
    for slug in ["work/project-1-p", "work/story-1-s", "work/task-1-t"] {
        assert_eq!(store.status_of(slug), "cancelled");
    }
    // Terminal is terminal.
    let err = bounce_core(&store, Actor::HumanSeat, &id("story-1"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("terminal"), "{err}");
}

#[tokio::test]
async fn close_requires_landed_children_and_the_merge_door_requires_a_claim() {
    let store = FakeStore::default();
    let dir = tempfile::TempDir::new().unwrap();
    signed_tree(&store).await;

    // An open child holds the story's close.
    let err = close_core(&store, Actor::Harness, &id("story-1"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("child"), "{err}");

    // The merge door refuses an unclaimed (ready) task.
    let err = complete_core(&store, dir.path(), Duration::from_secs(60), &id("task-1"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid transition"), "{err}");
}

#[tokio::test]
async fn new_validates_parentage_and_list_show_read_the_tree() {
    let store = FakeStore::default();
    tree(&store).await;

    // A task needs a story, and the parent's class is checked.
    let err = new_core(&store, &new_params(Class::Task, "Orphan", None))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("--parent"), "{err}");
    let err = new_core(&store, &new_params(Class::Task, "Wrong", Some("project-1")))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("hangs under"), "{err}");

    // Duplicate titles are fine: the ref makes the slug — and the handle —
    // unique by construction.
    let again = new_core(&store, &new_params(Class::Project, "P", None))
        .await
        .unwrap();
    assert_eq!(again["handle"], "project-2");
    assert_eq!(again["slug"], "work/project-2-p");

    let listed = list_core(
        &store,
        &ListParams {
            class: Some(Class::Task),
            status: None,
            parent: None,
        },
    )
    .await
    .unwrap();
    let items = listed["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["seal"], "unsealed");
    assert_eq!(items[0]["handle"], "task-1");

    // --parent scopes to the subtree, the parent itself excluded.
    let under = list_core(
        &store,
        &ListParams {
            class: None,
            status: None,
            parent: Some("project-1".into()),
        },
    )
    .await
    .unwrap();
    let slugs: Vec<&str> = under["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["slug"].as_str())
        .collect();
    assert_eq!(slugs, vec!["work/story-1-s", "work/task-1-t"]);

    let shown = show_core(
        &store,
        &ShowParams {
            id: "story-1".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(shown["children"].as_array().unwrap().len(), 1);
    assert!(shown["body"].as_str().unwrap().contains("## Goal"));
}

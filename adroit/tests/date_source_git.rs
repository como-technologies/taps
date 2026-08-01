//! `date_source = git` harness (hardening blitz #4).
//!
//! The oracle (`tests/model.rs`) runs `date_source = filesystem` to stay git-free.
//! This suite exercises the *other* path — the git-derived dates in
//! `src/history.rs` — on real git-backed KB spaces. It drives the binary to
//! create ADRs and `git commit`s through their status changes, then asserts
//! (via the library `query` layer) that under `date_source = git` the created /
//! last-modified dates come from the commits and the structural invariants
//! still hold (`check` clean, statuses intact). The old by_status status
//! *timeline* retired with the layouts (ADR-0020) — lifecycle history is
//! absent going forward, so only the dates are asserted here.

use std::path::Path;
use std::process::Command;

use adroit::adr::Status;
use adroit::config::DateSource;
use adroit::naming::NamingScheme;
use adroit::store::{Store, StoreOptions};
use adroit::{query, view::Severity};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@t.co"]);
    git(dir, &["config", "user.name", "Test"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

/// Scaffold the KB space (wiki.toml + wiki/decisions) — adroit never creates
/// the space itself (ADR-0020).
fn init_space(dir: &Path) {
    std::fs::write(dir.join("wiki.toml"), "name = \"test\"\n").unwrap();
    std::fs::create_dir_all(dir.join("wiki").join("decisions")).unwrap();
}

/// Commit with a **pinned, strictly increasing** commit date. Wall-clock
/// commits flake on a clock-stepping host (NTP under WSL2 stepped the clock
/// backwards mid-gate during the iteration-2 integration merge, reordering
/// `git log` and breaking `created <= last_modified`); pinned dates make the
/// derived dates deterministic.
fn commit(dir: &Path, msg: &str) {
    static COMMIT_DAY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let day = COMMIT_DAY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let date = format!("2026-01-{:02}T00:00:00Z", day.min(28));
    git(dir, &["add", "-A"]);
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .args(["commit", "-q", "-m", msg])
        .output()
        .expect("run git commit");
    assert!(
        out.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Run an `adroit` subcommand and require success, then commit the result.
fn adroit_commit(dir: &Path, args: &[&str], msg: &str) {
    let out = Command::new(env!("CARGO_BIN_EXE_adroit"))
        .arg("--dir")
        .arg(dir)
        .env("EDITOR", "true")
        .env("VISUAL", "true")
        .args(args)
        .output()
        .expect("spawn adroit");
    assert!(
        out.status.success(),
        "adroit {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    commit(dir, msg);
}

/// A read-only store over `dir` reading dates from **git**.
fn git_store(dir: &Path) -> Store {
    Store::open_with(
        dir,
        StoreOptions {
            review_overdue_days: None,
            date_source: DateSource::Git,
            naming: NamingScheme::Sequential,
        },
    )
    .unwrap()
}

fn check_clean(store: &Store) {
    let report = query::check(store).unwrap();
    let errors: Vec<&str> = report
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message.as_str())
        .collect();
    assert!(errors.is_empty(), "check errors under git: {errors:?}");
}

#[test]
fn git_dates_populate_created_and_last_modified() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_space(root);
    init_repo(root);

    adroit_commit(root, &["new", "Alpha", "--no-edit"], "add 0001"); // proposed
    adroit_commit(root, &["set-status", "1", "accepted"], "accept 0001"); // in place

    let store = git_store(root);
    let detail = query::detail(&store, 1).unwrap();

    // The git history yields both dates, in commit order.
    let created = detail
        .summary
        .created
        .clone()
        .expect("git created date populated");
    let modified = detail
        .last_modified
        .clone()
        .expect("git last-modified date populated");
    assert!(
        created <= modified,
        "created {created} must not follow last_modified {modified}"
    );
    // Status still resolves correctly, and the repo is clean under git.
    assert_eq!(detail.summary.status, Status::Accepted);
    check_clean(&store);
}

#[test]
fn supersede_and_invariants_hold_under_git() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_space(root);
    init_repo(root);

    adroit_commit(root, &["new", "Old way", "--no-edit"], "add 0001"); // proposed
    adroit_commit(root, &["new", "New way", "--no-edit"], "add 0002");
    adroit_commit(root, &["set-status", "2", "accepted"], "accept 0002");
    adroit_commit(root, &["supersede", "2", "1"], "supersede 0001 by 0002");

    let store = git_store(root);
    // ADR-1 is Superseded; ADR-2 is Accepted. Both resolved from the pages.
    let d1 = query::detail(&store, 1).unwrap();
    assert_eq!(d1.summary.status, Status::Superseded);
    let d2 = query::detail(&store, 2).unwrap();
    assert_eq!(d2.summary.status, Status::Accepted);

    // No corruption under git: the full set is intact and check is clean.
    let summaries = query::summaries(&store, &query::Filter::default()).unwrap();
    assert_eq!(summaries.len(), 2);
    check_clean(&store);
}

#[test]
fn date_source_git_on_a_non_git_dir_degrades_gracefully() {
    // `date_source = git` against a directory that isn't a git repo must not
    // panic — it falls back to the page's authored `created:` and still works.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_space(root);
    // No `git init` here.
    let out = Command::new(env!("CARGO_BIN_EXE_adroit"))
        .arg("--dir")
        .arg(root)
        .env("EDITOR", "true")
        .args(["new", "Alpha", "--no-edit"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let store = git_store(root);
    let detail = query::detail(&store, 1).unwrap(); // must not panic
    assert_eq!(detail.summary.status, Status::Proposed);
    check_clean(&store);
}

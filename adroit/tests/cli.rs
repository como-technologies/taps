use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Scaffold a KB space (wiki.toml + wiki/decisions) at `root` if absent.
/// Every command operates against a space (ADR-0020), and adroit itself never
/// creates one — the test harness owns the scaffold, mirroring what
/// `llm-wiki spaces create` provisions.
fn space(root: &Path) {
    if !root.join("wiki.toml").is_file() {
        fs::write(root.join("wiki.toml"), "name = \"test\"\n").unwrap();
    }
    fs::create_dir_all(root.join("wiki").join("decisions")).unwrap();
}

/// The corpus directory inside a test space (`<space>/wiki/decisions`).
fn corpus(dir: &TempDir) -> PathBuf {
    dir.path().join("wiki").join("decisions")
}

/// Build a command pointed at an isolated temp KB space (scaffolding it first).
fn adroit(dir: &TempDir) -> Command {
    space(dir.path());
    let mut cmd = Command::cargo_bin("adroit").unwrap();
    cmd.arg("--dir").arg(dir.path());
    // Never block on an editor in tests.
    cmd.env("EDITOR", "true").env("VISUAL", "true");
    // Hermetic AI config. Run in the temp dir so the binary's `dotenvy` load
    // can't discover a developer's repo-root `.env` (e.g. a dogfooding
    // `ADROIT_AI_*`), and drop any inherited AI env. Otherwise the
    // "no provider configured" tests reach a real provider whenever one happens
    // to be set up locally. Tests that want a provider set `ADROIT_AI_FAKE`
    // explicitly, which takes precedence regardless.
    cmd.current_dir(dir.path());
    // Hermetic global config: point XDG at the temp dir so the developer's
    // real ~/.config/adroit/config.yaml can never leak into a test.
    cmd.env("XDG_CONFIG_HOME", dir.path().join("xdg-config"));
    for var in [
        "ADROIT_AI_ENABLED",
        "ADROIT_AI_PROVIDER",
        "ADROIT_AI_MODEL",
        "ADROIT_AI_HOST",
        "ADROIT_ANTHROPIC_KEY",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

/// A minimal, valid hand-written KB decision page for fixture files. The id is
/// a fixed well-formed ULID — identity checks key on the filename/`reference`,
/// never on `id`.
fn page(reference: &str, title: &str, status: &str, extra_fm: &str, body: &str) -> String {
    format!(
        "---\nid: 01HZTESTTESTTESTTESTTEST00\ntitle: {title}\nreference: {reference}\n\
         status: {status}\ncreated: 2026-06-01T00:00:00Z\n{extra_fm}type: decision\n---\n\n{body}"
    )
}

/// Replace only the prose body of an on-disk KB page, keeping its frontmatter
/// intact (a raw `fs::write` of a whole document would destroy the YAML block).
fn write_body(path: &Path, body: &str) {
    let content = fs::read_to_string(path).unwrap();
    let close = content
        .find("\n---\n")
        .expect("closing frontmatter delimiter");
    fs::write(path, format!("{}\n\n{}", &content[..close + 4], body)).unwrap();
}

/// Append prose to an on-disk KB page's body (the body is the document tail,
/// so a plain append keeps the page valid).
fn append_body(path: &Path, extra: &str) {
    let mut content = fs::read_to_string(path).unwrap();
    content.push_str(extra);
    fs::write(path, content).unwrap();
}

/// Recursively collect ADR markdown files (excluding README/template).
fn adr_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                let name = p.file_name().unwrap().to_str().unwrap();
                if !name.eq_ignore_ascii_case("README.md")
                    && !name.eq_ignore_ascii_case("adr-template.md")
                {
                    out.push(p);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// Snapshot every file under `root` (relative path → bytes), including
/// `wiki.toml` / `SUMMARY.md`. Used by the idempotency guards for
/// byte-identical before/after comparisons of the whole space.
fn snapshot(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_path_buf();
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

// ---------------------------------------------------------------------------
// The KB decision page (the one profile — ADR-0020)
// ---------------------------------------------------------------------------

#[test]
fn new_creates_a_kb_decision_page_in_the_decisions_dir() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    let files = adr_files(dir.path());
    assert_eq!(files.len(), 1);
    let p = &files[0];
    assert!(p.parent().unwrap().ends_with("wiki/decisions"));
    assert!(
        p.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("0001-use-postgresql.md")
    );

    // Frontmatter is the source of truth for every machine-owned field; the
    // body is prose only (no H1 / `> State:` banner / `## Status` section).
    let content = fs::read_to_string(p).unwrap();
    assert!(content.starts_with("---\n"), "{content}");
    assert!(content.contains("title: Use PostgreSQL"), "{content}");
    assert!(content.contains("reference: ADR-0001"), "{content}");
    assert!(content.contains("status: proposed"), "{content}");
    assert!(content.contains("type: decision"), "{content}");
    assert!(content.contains("id: "), "{content}");
    assert!(!content.contains("\n# "), "no H1 in the body: {content}");
    assert!(!content.contains("> State:"), "{content}");
    assert!(!content.contains("## Status"), "{content}");
    assert!(content.contains("## Stakeholders"), "{content}");
}

#[test]
fn list_shows_created_adrs() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "First decision", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Second decision", "--no-edit"])
        .assert()
        .success();

    adroit(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("First decision"))
        .stdout(predicate::str::contains("Second decision"));
}

#[test]
fn list_filter_by_status() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Keep proposed", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Make accepted", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-status", "2", "accepted"])
        .assert()
        .success();

    adroit(&dir)
        .args(["list", "--status", "accepted"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Make accepted"))
        .stdout(predicate::str::contains("Keep proposed").not());
}

#[test]
fn status_change_rewrites_frontmatter_in_place() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use Kafka", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();

    // Flat is the only layout: a status change never moves the file.
    let p = corpus(&dir).join("0001-use-kafka.md");
    assert!(p.exists(), "the file must stay put on a status change");
    let content = fs::read_to_string(&p).unwrap();
    assert!(content.contains("status: accepted"), "{content}");
    assert!(!content.contains("status: proposed"), "{content}");
}

#[test]
fn status_getter_prints_lowercase_and_round_trips_into_set_status() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A decision", "--no-edit"])
        .assert()
        .success();

    // `status <ID>` is a getter: just the status word, lowercase (scriptable).
    adroit(&dir)
        .args(["status", "1"])
        .assert()
        .success()
        .stdout("proposed\n");

    // ...and it feeds straight back into `set-status`.
    adroit(&dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();
    adroit(&dir)
        .args(["status", "1"])
        .assert()
        .success()
        .stdout("accepted\n");

    // `-o json` emits a valid JSON string (the typed `Status`), lowercase per
    // the KB `decision` schema enum (ADR-0020) — matching `show`/`list -o json`.
    assert_eq!(
        json_ok(&dir, &["status", "1", "-o", "json"]),
        serde_json::json!("accepted")
    );
}

#[test]
fn supersede_records_both_directions_in_place() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Old way", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "New way", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-status", "2", "accepted"])
        .assert()
        .success();

    adroit(&dir)
        .args(["supersede", "2", "1"])
        .assert()
        .success();

    // The old ADR never moves: its frontmatter gains the status + the ref.
    let old = corpus(&dir).join("0001-old-way.md");
    assert!(old.exists());
    let old_content = fs::read_to_string(&old).unwrap();
    assert!(old_content.contains("status: superseded"), "{old_content}");
    assert!(old_content.contains("superseded_by: 2"), "{old_content}");

    // The new ADR carries the reciprocal body note with a canonical link.
    let new_content = fs::read_to_string(corpus(&dir).join("0002-new-way.md")).unwrap();
    assert!(
        new_content.contains("Supersedes [ADR-0001](./0001-old-way.md)"),
        "{new_content}"
    );
    adroit(&dir).arg("check").assert().success();
}

/// Regression (hardening blitz, model-based oracle — adapted to the KB shape):
/// the reciprocal "Supersedes [..]" note `supersede` writes into the newer ADR
/// must be in the canonical `./` form `relink` produces, so the repo stays
/// link-canonical and a follow-up `relink` is a no-op (the documented
/// invariant).
#[test]
fn supersede_leaves_links_canonical() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Alpha", "--no-edit"])
        .assert()
        .success(); // ADR-0001
    adroit(&dir)
        .args(["new", "Beta", "--no-edit"])
        .assert()
        .success(); // ADR-0002
    adroit(&dir)
        .args(["set-status", "1", "superseded"])
        .assert()
        .success();
    // Supersede ADR-0002 by ADR-0001.
    adroit(&dir)
        .args(["supersede", "1", "2"])
        .assert()
        .success();

    // The reciprocal note's same-dir link is canonical (`./`).
    let new = fs::read_to_string(corpus(&dir).join("0001-alpha.md")).unwrap();
    assert!(
        new.contains("Supersedes [ADR-0002](./0002-beta.md)"),
        "reciprocal note must use the canonical ./ link form, got:\n{new}"
    );

    // And the repo is link-canonical: a relink dry-run rewrites nothing.
    adroit(&dir)
        .args(["relink", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("canonical"));
}

/// Regression (hardening blitz, full-matrix oracle): under the `uuid` naming
/// scheme, a supersession link (`…/{uuid}-{slug}.md`) must resolve back to the
/// ADR whose identity is the bare `{uuid}`. `ref_in_link` previously returned the
/// whole filename stem (`{uuid}-{slug}`), so `adroit check` reported the
/// supersession as "no such ADR exists" and exited non-zero — uuid supersede
/// produced a repo that failed its own validation.
#[test]
fn uuid_scheme_supersede_passes_check() {
    let dir = TempDir::new().unwrap();
    let scheme = ["--naming", "uuid"];
    adroit(&dir)
        .args(scheme)
        .args(["new", "Alpha", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(scheme)
        .args(["new", "Beta", "--no-edit"])
        .assert()
        .success();

    // Recover the two uuids from the filenames (`{uuid}-{slug}.md`).
    let ids: Vec<String> = adr_files(dir.path())
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_str().unwrap();
            name.split('-').next().unwrap().to_string()
        })
        .collect();
    assert_eq!(ids.len(), 2, "expected two ADRs, got {ids:?}");

    adroit(&dir)
        .args(scheme)
        .args(["supersede", &ids[0], &ids[1]])
        .assert()
        .success();

    // The superseded ADR's link must resolve, so `check` passes and `relink` is
    // a no-op (the repo is consistent).
    adroit(&dir).args(scheme).args(["check"]).assert().success();
    adroit(&dir)
        .args(scheme)
        .args(["relink", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("canonical"));
}

/// The KB page profile persists a `reference:` string (ADR-0020), so every
/// naming scheme works end-to-end — identity lives in the filename and the
/// mirrored frontmatter, never in a heading.
#[test]
fn slug_naming_schemes_work_end_to_end() {
    for scheme in ["date", "uuid"] {
        let dir = TempDir::new().unwrap();
        adroit(&dir)
            .args(["--naming", scheme])
            .args(["new", "Hello", "--no-edit"])
            .assert()
            .success();
        adroit(&dir)
            .args(["--naming", scheme])
            .arg("check")
            .assert()
            .success();
    }
}

/// Regression (hardening blitz, #8 check-half): `check` validates frontmatter
/// supersession refs (the YAML `superseded_by:` field). This is the backstop
/// that keeps a dangling pointer from being silent — e.g. when the target ADR
/// is removed out-of-band. (`renumber` itself no longer strands these; it
/// remaps the YAML ref — see `renumber_rewrites_frontmatter_supersession_ref`.)
#[test]
fn check_flags_stranded_supersession() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Alpha", "--no-edit"])
        .assert()
        .success(); // ADR-1
    adroit(&dir)
        .args(["new", "Beta", "--no-edit"])
        .assert()
        .success(); // ADR-2
    adroit(&dir)
        .args(["supersede", "2", "1"])
        .assert()
        .success(); // ADR-1 superseded_by: 2
    adroit(&dir).args(["check"]).assert().success();

    // Remove ADR-2 out-of-band, stranding ADR-1's `superseded_by: 2`. The check
    // rule resolves the bare-number YAML ref against the identity set and must
    // flag it as a broken supersession.
    fs::remove_file(corpus(&dir).join("0002-beta.md")).unwrap();
    adroit(&dir)
        .args(["check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no such ADR exists"));
}

/// Regression (adroit issue 28, portfolio ADR-0006): a KB-resident decision
/// page carries frontmatter keys adroit does not own (`citations:`, …). A
/// `set-status` through the real binary must leave those keys byte-intact —
/// before the fix the rewrite silently destroyed them while exiting 0.
#[test]
fn set_status_preserves_foreign_frontmatter_keys_byte_intact() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt the KB substrate", "--no-edit"])
        .assert()
        .success();

    // Splice KB-owned keys into the YAML block, in adroit's own emit position
    // (end of the block) and style, as a KB tool would leave them.
    let path = corpus(&dir).join("0001-adopt-the-kb-substrate.md");
    let original = fs::read_to_string(&path).unwrap();
    let close = original.rfind("\n---\n").unwrap();
    // `type:` is adroit-owned (stamped on every serialize); the foreign keys
    // are the substrate's own annotations.
    let foreign = "citations:\n- evidence/transcripts/2026-07-20-kb-chat.md@abc123\n";
    let seeded = format!(
        "{}{}{}",
        &original[..close + 1],
        foreign,
        &original[close + 1..]
    );
    fs::write(&path, &seeded).unwrap();

    adroit(&dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();

    // Byte-intact: the rewritten file differs from the seeded input only in
    // the status line — every foreign key survives verbatim.
    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(
        after,
        seeded.replace("status: proposed", "status: accepted")
    );
}

#[test]
fn set_review_sets_and_clears_deadline_byte_stable() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use Redis", "--no-edit"])
        .assert()
        .success();

    let path = corpus(&dir).join("0001-use-redis.md");
    let before = fs::read_to_string(&path).unwrap();

    // Set a deadline: the `review_by:` field lands in frontmatter.
    adroit(&dir)
        .args(["set-review", "1", "2026-07-15"])
        .assert()
        .success()
        .stdout(predicate::str::contains("review deadline to 2026-07-15"));
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("review_by: 2026-07-15"), "{after}");

    // Clearing removes the field and restores the original bytes.
    adroit(&dir)
        .args(["set-review", "1", "--clear"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared"));
    let cleared = fs::read_to_string(&path).unwrap();
    assert!(!cleared.contains("review_by:"));
    assert_eq!(cleared, before);
}

#[test]
fn set_review_rejects_bad_date() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use Redis", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-review", "1", "07/15/2026"])
        .assert()
        .failure();
}

#[test]
fn search_matches_title_and_body() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt Postgres", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Adopt Redis", "--no-edit"])
        .assert()
        .success();

    adroit(&dir)
        .args(["search", "redis"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Adopt Redis"))
        .stdout(predicate::str::contains("Adopt Postgres").not());
}

#[test]
fn index_prints_when_no_summary() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "First", "--no-edit"])
        .assert()
        .success();

    adroit(&dir)
        .arg("index")
        .assert()
        .success()
        .stdout(predicate::str::contains("## Proposed"))
        .stdout(predicate::str::contains("ADR-0001: First"));
}

#[test]
fn index_regenerates_summary_preserving_header() {
    // SUMMARY.md lives beside the corpus (`<space>/wiki/SUMMARY.md`); the ADR
    // links resolve into `./decisions/`.
    let dir = TempDir::new().unwrap();
    space(dir.path());
    let summary = dir.path().join("wiki").join("SUMMARY.md");
    fs::write(
        &summary,
        "# Summary\n\n[Introduction](./README.md)\n\n# Architecture Decision Records\n\n- [ADR Process](./decisions/README.md)\n",
    )
    .unwrap();

    adroit(&dir)
        .args(["new", "Repo Strategy", "--no-edit"])
        .assert()
        .success();
    adroit(&dir).arg("index").assert().success();

    let out = fs::read_to_string(&summary).unwrap();
    assert!(out.contains("# Summary"));
    assert!(out.contains("- [ADR Process](./decisions/README.md)"));
    assert!(out.contains("## Proposed"));
    assert!(
        out.contains("[ADR-0001: Repo Strategy](./decisions/0001-repo-strategy.md)"),
        "{out}"
    );
}

// ---------------------------------------------------------------------------
// `check` — structural CI gate
// ---------------------------------------------------------------------------

#[test]
fn check_passes_on_clean_repo() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "First decision", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Second decision", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-status", "2", "accepted"])
        .assert()
        .success();

    adroit(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("OK: 2 ADRs, no problems"));
}

#[test]
fn check_empty_repo_passes() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("OK: 0 ADRs"));
}

// ---------------------------------------------------------------------------
// `index --check` — SUMMARY.md drift gate
// ---------------------------------------------------------------------------

#[test]
fn index_check_passes_when_in_sync() {
    let dir = TempDir::new().unwrap();
    space(dir.path());
    let summary = dir.path().join("wiki").join("SUMMARY.md");
    fs::write(
        &summary,
        "# Summary\n\n[Introduction](./README.md)\n\n# Architecture Decision Records\n\n- [ADR Process](./decisions/README.md)\n",
    )
    .unwrap();

    adroit(&dir)
        .args(["new", "Repo Strategy", "--no-edit"])
        .assert()
        .success();
    // Write the SUMMARY so it is in sync.
    adroit(&dir).arg("index").assert().success();

    adroit(&dir)
        .args(["index", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SUMMARY.md is up to date"));
}

#[test]
fn index_check_fails_when_out_of_date() {
    let dir = TempDir::new().unwrap();
    space(dir.path());
    let summary = dir.path().join("wiki").join("SUMMARY.md");
    fs::write(
        &summary,
        "# Summary\n\n[Introduction](./README.md)\n\n# Architecture Decision Records\n\n- [ADR Process](./decisions/README.md)\n",
    )
    .unwrap();

    adroit(&dir)
        .args(["new", "Repo Strategy", "--no-edit"])
        .assert()
        .success();
    adroit(&dir).arg("index").assert().success();

    // Change a status without re-indexing: SUMMARY.md is now stale.
    adroit(&dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();

    adroit(&dir)
        .args(["index", "--check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("out of date"));

    // Re-indexing brings it back into sync.
    adroit(&dir).arg("index").assert().success();
    adroit(&dir).args(["index", "--check"]).assert().success();
}

#[test]
fn index_check_no_summary_exits_zero() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Lonely", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["index", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No SUMMARY.md found"));
}

#[test]
fn next_number_is_the_global_max_plus_one() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "One", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Two", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-status", "2", "accepted"])
        .assert()
        .success();
    // Third is 0003 even after 0002's status changed.
    adroit(&dir)
        .args(["new", "Three", "--no-edit"])
        .assert()
        .success();
    assert!(corpus(&dir).join("0003-three.md").exists());
}

#[test]
fn show_displays_adr_details() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use Redis", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["show", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Use Redis"));
}

#[test]
fn show_missing_adr_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir).args(["show", "99"]).assert().failure();
}

#[test]
fn new_empty_title_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "", "--no-edit"])
        .assert()
        .failure();
}

#[test]
fn status_invalid_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Some ADR", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-status", "1", "bogus"])
        .assert()
        .failure();
}

#[test]
fn list_empty_dir_succeeds() {
    let dir = TempDir::new().unwrap();
    adroit(&dir).arg("list").assert().success();
}

#[test]
fn new_then_edit_with_fake_editor() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Editable decision", "--no-edit"])
        .assert()
        .success();
    adroit(&dir).args(["edit", "1"]).assert().success();
}

#[test]
fn review_generates_kickoff_doc() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Cluster Templates", "--no-edit"])
        .assert()
        .success();

    adroit(&dir)
        .args(["review", "1"])
        .assert()
        .success()
        // H1 with the ADR number.
        .stdout(predicate::str::contains("ADR-0001 Review Kickoff"))
        // The ADR title and number appear in the body.
        .stdout(predicate::str::contains("ADR-0001 (Cluster Templates)"))
        // The quorum line (default 3).
        .stdout(predicate::str::contains("3 team members must approve"))
        // The three Key-docs rows.
        .stdout(predicate::str::contains("[Read the ADR]"))
        .stdout(predicate::str::contains("[Read the README](../README.md)"))
        .stdout(predicate::str::contains(
            "[Read the guide](../../guides/adr-review-process.md)",
        ));
}

#[test]
fn review_writes_output_file() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Repo Strategy", "--no-edit"])
        .assert()
        .success();

    let out = dir.path().join("kickoff.md");
    adroit(&dir)
        .args(["review", "1", "--quorum", "5", "--days", "5"])
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("ADR-0001 Review Kickoff"));
    assert!(content.contains("5 team members must approve"));
}

#[test]
fn review_missing_adr_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir).args(["review", "99"]).assert().failure();
}

// ---------------------------------------------------------------------------
// Cross-ADR link integrity (`relink`, `check`)
// ---------------------------------------------------------------------------

#[test]
fn check_warns_on_stale_link_and_relink_repairs_it() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "B", "--no-edit"])
        .assert()
        .success();
    // ADR-0001 links to ADR-0002 at a path that no longer exists (as if the
    // file were reorganized outside adroit, or the link hand-written wrong).
    // The literal target is gone, but ADR-0002 still exists — so this is a
    // STALE link a `relink` heals, NOT a hard error.
    let a = corpus(&dir).join("0001-a.md");
    append_body(&a, "\nSee [ADR-0002](../elsewhere/0002-b.md).\n");

    // check SUCCEEDS — the stale link is a warning, not an error — but reports it.
    adroit(&dir)
        .arg("check")
        .assert()
        .success()
        .stderr(predicate::str::contains("stale link"));

    // relink repairs it to the canonical location.
    adroit(&dir)
        .arg("relink")
        .assert()
        .success()
        .stdout(predicate::str::contains("Relinked"));
    let one = fs::read_to_string(&a).unwrap();
    assert!(one.contains("[ADR-0002](./0002-b.md)"), "got:\n{one}");

    // check is now fully clean.
    adroit(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("no problems"));
}

#[test]
fn check_fails_on_dangling_link_to_unknown_adr() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A", "--no-edit"])
        .assert()
        .success();
    // ADR-0001 links to ADR-0099, which exists nowhere in the repo — a truly
    // dangling link that points at no ADR. This stays a hard error (so genuine
    // breakage still fails CI even though stale links are warnings).
    append_body(
        &corpus(&dir).join("0001-a.md"),
        "\nSee [ADR-0099](./0099-ghost.md).\n",
    );

    adroit(&dir)
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("broken link"));
}

#[test]
fn duplicate_number_fails_check() {
    let dir = TempDir::new().unwrap();
    space(dir.path());
    // Two ADRs share number 0009 — the collision two branches produce on merge.
    fs::write(
        corpus(&dir).join("0009-crossplane.md"),
        page(
            "ADR-0009",
            "Crossplane",
            "proposed",
            "",
            "## Context\n\nc.\n",
        ),
    )
    .unwrap();
    fs::write(
        corpus(&dir).join("0009-dex.md"),
        page("ADR-0009", "Dex", "accepted", "", "## Context\n\nd.\n"),
    )
    .unwrap();

    // The merge-queue gate: a duplicate number is an ERROR (not a warning), so
    // `adroit check` fails — ejecting the second colliding PR from the queue.
    adroit(&dir)
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate number"));
}

// ---------------------------------------------------------------------------
// `adroit renumber`
// ---------------------------------------------------------------------------

#[test]
fn renumber_renames_and_rewrites_inbound_refs() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "B", "--no-edit"])
        .assert()
        .success();
    append_body(
        &corpus(&dir).join("0001-a.md"),
        "\nSee [ADR-0002](./0002-b.md).\n",
    );

    adroit(&dir)
        .args(["renumber", "2", "5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ADR-0002 -> ADR-0005"));

    assert!(corpus(&dir).join("0005-b.md").exists());
    assert!(!corpus(&dir).join("0002-b.md").exists());
    // The renamed page's own persisted `reference:` follows.
    assert!(
        fs::read_to_string(corpus(&dir).join("0005-b.md"))
            .unwrap()
            .contains("reference: ADR-0005")
    );
    let a = fs::read_to_string(corpus(&dir).join("0001-a.md")).unwrap();
    assert!(a.contains("[ADR-0005](./0005-b.md)"), "got: {a}");
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn renumber_resolves_duplicate_with_file_flag() {
    let dir = TempDir::new().unwrap();
    space(dir.path());
    // Duplicate 0009 (different slugs) — the real-world collision.
    let crossplane = corpus(&dir).join("0009-crossplane.md");
    fs::write(
        &crossplane,
        page(
            "ADR-0009",
            "Crossplane",
            "proposed",
            "",
            "## Context\n\nc.\n",
        ),
    )
    .unwrap();
    fs::write(
        corpus(&dir).join("0009-dex.md"),
        page("ADR-0009", "Dex", "accepted", "", "## Context\n\nd.\n"),
    )
    .unwrap();

    // Ambiguous without --file.
    adroit(&dir)
        .args(["renumber", "9", "21"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous"));

    adroit(&dir)
        .args([
            "renumber",
            "9",
            "21",
            "--file",
            crossplane.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(corpus(&dir).join("0021-crossplane.md").exists());
    assert!(
        corpus(&dir).join("0009-dex.md").exists(),
        "the other 0009 is untouched"
    );
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn renumber_rewrites_frontmatter_supersession_ref() {
    // Supersession is a bare-number YAML field (`superseded_by: N`), not a
    // markdown link. Renumbering the *superseding* ADR must retarget that
    // inbound ref so it isn't stranded.
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "First", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Second", "--no-edit"])
        .assert()
        .success();
    // ADR 2 supersedes ADR 1 -> ADR 1's YAML gains `superseded_by: 2`.
    adroit(&dir)
        .args(["supersede", "2", "1"])
        .assert()
        .success();
    let one = corpus(&dir).join("0001-first.md");
    assert!(
        fs::read_to_string(&one)
            .unwrap()
            .contains("superseded_by: 2"),
        "precondition: ADR 1 records the supersession"
    );

    // Renumber the superseding ADR 2 -> 9.
    adroit(&dir).args(["renumber", "2", "9"]).assert().success();

    let one_after = fs::read_to_string(&one).unwrap();
    assert!(
        one_after.contains("superseded_by: 9"),
        "the inbound frontmatter ref must follow the renumber:\n{one_after}"
    );
    assert!(
        !one_after.contains("superseded_by: 2"),
        "the stranded ref must be gone:\n{one_after}"
    );
    // No stranded supersession -> `check` is clean.
    adroit(&dir).arg("check").assert().success();
}

// ---------------------------------------------------------------------------
// Statelessness / idempotency invariant
//
// Guards the design principle (CLAUDE.md "Design principles", book dev/design.md):
// the only state is the filesystem, and every converge-style verb is idempotent —
// re-running it on an unchanged tree is a byte-for-byte no-op.
// ---------------------------------------------------------------------------

#[test]
fn commands_are_idempotent() {
    let dir = TempDir::new().unwrap();
    let run = |args: &[&str]| adroit(&dir).args(args).assert().success();

    // A small repo with a status change, a supersession, a review deadline, and a
    // regenerated index — so relink/index/etc. all have real work to (not) redo.
    run(&["new", "First", "--no-edit"]);
    run(&["new", "Second", "--no-edit"]);
    run(&["new", "Third", "--no-edit"]);
    run(&["new", "Fourth", "--no-edit"]);
    run(&["new", "Fifth", "--no-edit"]);

    // The converge-style verbs: each asserts a desired state. Distinct ADRs per
    // verb so the *first* loop pass below is already a no-op (1 is accepted, 4
    // supersedes 5, 2 has a deadline — none conflict).
    let converge: &[&[&str]] = &[
        &["set-status", "1", "accepted"],
        &["supersede", "4", "5"],
        &["set-review", "2", "2030-01-01"],
        &["index"],
        &["relink"],
    ];
    for argv in converge {
        run(argv);
    }

    // Re-running every converge verb on the now-canonical tree must change
    // nothing — same files, byte-identical contents.
    let before = snapshot(dir.path());
    for argv in converge {
        run(argv);
    }
    let after = snapshot(dir.path());
    assert_eq!(
        before, after,
        "re-running converge-style verbs must be a byte-for-byte no-op"
    );
}

#[test]
fn dry_run_changes_nothing() {
    // `--dry-run` is a true full preview: it must leave the repo byte-identical
    // even **without** `--forge` (the local mutation is gated too, not just the
    // forge side). No forge is configured here, so this isolates the local path.
    let dir = TempDir::new().unwrap();
    let run = |args: &[&str]| adroit(&dir).args(args).assert().success();
    run(&["new", "First", "--no-edit"]);
    run(&["new", "Second", "--no-edit"]);
    run(&["new", "Third", "--no-edit"]);

    let before = snapshot(dir.path());
    let dry: &[&[&str]] = &[
        &["new", "Fourth", "--dry-run"], // allocates no number, opens no editor
        &["set-status", "1", "accepted", "--dry-run"],
        &["supersede", "2", "3", "--dry-run"],
        &["set-review", "1", "2030-01-01", "--dry-run"],
        &["relink", "--dry-run"],
    ];
    for argv in dry {
        run(argv);
    }
    assert_eq!(
        before,
        snapshot(dir.path()),
        "every --dry-run verb must leave the repo byte-for-byte unchanged"
    );

    // And it actually *previews* (not silently no-ops): `new --dry-run` reports
    // the would-be path and creates nothing.
    adroit(&dir)
        .args(["new", "Fourth", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run: would create"));
    // The repo is still three ADRs — `new --dry-run` allocated nothing.
    assert_eq!(before, snapshot(dir.path()));

    // `plan --save --dry-run` (ADR-0008 write path) previews the generated
    // plan + target without splicing it (needs the fake provider to generate).
    adroit(&dir)
        .args(["plan", "1", "--save", "--dry-run"])
        .env("ADROIT_AI_FAKE", "1. A plan that must not land.")
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run"));
    assert_eq!(
        before,
        snapshot(dir.path()),
        "plan --save --dry-run must leave the repo byte-for-byte unchanged"
    );
}

#[test]
fn relink_dry_run_previews_without_writing() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "B", "--no-edit"])
        .assert()
        .success();
    let a = corpus(&dir).join("0001-a.md");
    append_body(&a, "\nSee [ADR-0002](../elsewhere/0002-b.md).\n");
    let before = fs::read_to_string(&a).unwrap();

    adroit(&dir)
        .args(["relink", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Would relink"));
    assert_eq!(
        fs::read_to_string(&a).unwrap(),
        before,
        "dry run must not write"
    );

    adroit(&dir).arg("relink").assert().success();
    assert!(fs::read_to_string(&a).unwrap().contains("./0002-b.md"));
}

// ---------------------------------------------------------------------------
// `adroit config` (show / get / set)
// ---------------------------------------------------------------------------

#[test]
fn config_show_lists_keys_and_sources() {
    let dir = TempDir::new().unwrap();
    let xdg = TempDir::new().unwrap();
    adroit(&dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("SOURCE"))
        .stdout(predicate::str::contains("naming"))
        .stdout(predicate::str::contains("date_source"));
}

#[test]
fn config_get_reflects_flag_and_env() {
    let dir = TempDir::new().unwrap();
    let xdg = TempDir::new().unwrap();
    // A flag override is reflected.
    adroit(&dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["--naming", "uuid", "config", "get", "naming"])
        .assert()
        .success()
        .stdout(predicate::str::contains("uuid"));
    // An env override is reflected.
    adroit(&dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .env("ADROIT_DATE_SOURCE", "git")
        .args(["config", "get", "date_source"])
        .assert()
        .success()
        .stdout(predicate::str::contains("git"));
}

#[test]
fn config_set_writes_config_yaml_and_round_trips() {
    let dir = TempDir::new().unwrap();
    let xdg = TempDir::new().unwrap();
    adroit(&dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["config", "set", "review_overdue_days", "45"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set review_overdue_days = 45"));
    adroit(&dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["config", "get", "review_overdue_days"])
        .assert()
        .success()
        .stdout(predicate::str::contains("45"));
}

#[test]
fn config_set_local_writes_project_dotenv() {
    let adr_dir = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let xdg = TempDir::new().unwrap();
    adroit(&adr_dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .current_dir(cwd.path())
        .args(["config", "set", "naming", "date", "--local"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ADROIT_NAMING=date"));
    let env = fs::read_to_string(cwd.path().join(".env")).unwrap();
    assert!(env.contains("ADROIT_NAMING=date"), "got: {env}");
}

#[test]
fn config_set_rejects_bad_value_and_unknown_key() {
    let dir = TempDir::new().unwrap();
    let xdg = TempDir::new().unwrap();
    adroit(&dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["config", "set", "naming", "sideways"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid"));
    adroit(&dir)
        .env("XDG_CONFIG_HOME", xdg.path())
        .args(["config", "set", "bogus", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown config key"));
}

#[test]
fn dir_flag_overrides_default() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Scoped decision", "--no-edit"])
        .assert()
        .success();
    let alt = TempDir::new().unwrap();
    adroit(&alt).arg("list").assert().success();
}

// ---------------------------------------------------------------------------
// Typed relational links (`adroit link`)
// ---------------------------------------------------------------------------

#[test]
fn link_adds_typed_relation_in_frontmatter() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Base", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Dependent", "--no-edit"])
        .assert()
        .success();

    adroit(&dir)
        .args(["link", "2", "--depends-on", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("depends on"));

    let body = fs::read_to_string(corpus(&dir).join("0002-dependent.md")).unwrap();
    assert!(
        body.contains("depends_on:"),
        "frontmatter records the link: {body}"
    );

    adroit(&dir)
        .args(["show", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Depends on: ADR-0001"));

    // --remove takes it back out.
    adroit(&dir)
        .args(["link", "2", "--depends-on", "1", "--remove"])
        .assert()
        .success();
    let after = fs::read_to_string(corpus(&dir).join("0002-dependent.md")).unwrap();
    assert!(!after.contains("depends_on:"));
}

#[test]
fn link_to_missing_adr_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Base", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["link", "1", "--relates-to", "99"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// No subcommand -> interactive TUI (default build)
// ---------------------------------------------------------------------------

/// With no subcommand and a non-interactive stdin (as in CI / piped contexts —
/// `assert_cmd` runs the child with a non-TTY stdin), adroit must NOT try to
/// seize a real terminal: it prints a short hint and exits 0. This exercises
/// exactly that path so the test can never hang waiting on a terminal.
///
/// The hint differs slightly between a `tui`-enabled build ("requires an
/// interactive terminal") and a no-`tui` build ("built without the `tui`
/// feature"), but both steer the user to the CLI subcommands — assert on that
/// shared cue so the test passes under either feature set.
#[test]
fn no_args_without_tty_prints_hint_and_exits_zero() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("CLI subcommands"));
}

// ── global --dir / env var (regression: --dir must work after a subcommand) ──

#[test]
fn dir_flag_works_after_subcommand() {
    let dir = TempDir::new().unwrap();

    // Seed one ADR (the `adroit` helper passes --dir before the subcommand).
    adroit(&dir)
        .args(["new", "First decision", "--no-edit"])
        .assert()
        .success();

    // The global flag must also be accepted AFTER the subcommand. Build a raw
    // command (the helper already injects --dir, so use it directly here).
    let mut cmd = Command::cargo_bin("adroit").unwrap();
    cmd.args(["list", "--dir", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("First decision"));

    // ...and the short form too.
    let mut cmd = Command::cargo_bin("adroit").unwrap();
    cmd.args(["list", "-d", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("First decision"));
}

#[test]
fn adroit_dir_env_var_sets_directory() {
    let dir = TempDir::new().unwrap();

    adroit(&dir)
        .args(["new", "Env decision", "--no-edit"])
        .assert()
        .success();

    // No --dir flag: the directory comes from the ADROIT_DIR env var.
    let mut cmd = Command::cargo_bin("adroit").unwrap();
    cmd.env("ADROIT_DIR", dir.path().to_str().unwrap())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Env decision"));
}

#[test]
fn non_scaffolding_command_on_a_missing_dir_fails_without_creating_it() {
    let tmp = TempDir::new().unwrap();

    // `check` pointed at a non-existent dir must fail loudly and touch nothing.
    // The old behavior created the dir and printed "OK: 0 ADRs" with exit 0 —
    // a green CI gate against a directory that doesn't exist.
    let missing = tmp.path().join("typo-adrs");
    Command::cargo_bin("adroit")
        .unwrap()
        .arg("--dir")
        .arg(&missing)
        .arg("check")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("does not exist")
                .and(predicate::str::contains("typo-adrs"))
                .and(predicate::str::contains("OK: 0 ADRs").not()),
        );
    assert!(
        !missing.exists(),
        "a non-scaffolding command must not create the missing --dir"
    );

    // Every other read/write verb rides the same guard — spot-check `list`.
    Command::cargo_bin("adroit")
        .unwrap()
        .arg("--dir")
        .arg(&missing)
        .env("EDITOR", "true")
        .env("VISUAL", "true")
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
    assert!(!missing.exists());

    // A --dir that exists but is NOT a KB space (no wiki.toml) is a hard error
    // naming the bootstrap (ADR-0020) — for scaffolding verbs too: `new` may
    // create the decisions/ dir INSIDE a space, never the space itself.
    let plain = tmp.path().join("plain-dir");
    fs::create_dir_all(&plain).unwrap();
    for argv in [vec!["check"], vec!["new", "X", "--no-edit"]] {
        Command::cargo_bin("adroit")
            .unwrap()
            .arg("--dir")
            .arg(&plain)
            .env("EDITOR", "true")
            .env("VISUAL", "true")
            .args(&argv)
            .assert()
            .failure()
            .stderr(
                predicate::str::contains("not a KB space")
                    .and(predicate::str::contains("adroit seed --from")),
            );
    }
    assert!(!plain.join("wiki.toml").exists());
    assert!(!plain.join("wiki").exists());

    // In a space missing its decisions/ dir, a read verb fails without
    // creating it…
    let bare = tmp.path().join("bare-space");
    fs::create_dir_all(&bare).unwrap();
    fs::write(bare.join("wiki.toml"), "name = \"t\"\n").unwrap();
    Command::cargo_bin("adroit")
        .unwrap()
        .arg("--dir")
        .arg(&bare)
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
    assert!(!bare.join("wiki").exists());

    // …while `new` (a scaffolding verb) creates decisions/ inside the space.
    Command::cargo_bin("adroit")
        .unwrap()
        .arg("--dir")
        .arg(&bare)
        .env("EDITOR", "true")
        .env("VISUAL", "true")
        .args(["new", "First decision", "--no-edit"])
        .assert()
        .success();
    assert!(bare.join("wiki/decisions/0001-first-decision.md").exists());
}

// ---------------------------------------------------------------------------
// Naming schemes (date / uuid) end-to-end through the naming seam
// ---------------------------------------------------------------------------

/// A command in the date naming scheme.
fn adroit_date(dir: &TempDir) -> Command {
    let mut cmd = adroit(dir);
    cmd.args(["--naming", "date"]);
    cmd
}

/// The filename stem (no `.md`) of the single ADR in the store.
fn sole_stem(root: &Path) -> String {
    let files = adr_files(root);
    assert_eq!(files.len(), 1, "expected exactly one ADR");
    files[0]
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_suffix(".md")
        .unwrap()
        .to_string()
}

#[test]
fn date_scheme_new_uses_a_date_slug_filename() {
    let dir = TempDir::new().unwrap();
    adroit_date(&dir)
        .args(["new", "Adopt PostgreSQL", "--no-edit"])
        .assert()
        .success();

    let files = adr_files(dir.path());
    assert_eq!(files.len(), 1);
    let p = &files[0];
    assert!(p.parent().unwrap().ends_with("wiki/decisions"));
    let name = p.file_name().unwrap().to_str().unwrap();
    // `YYYYMMDD-<slug>.md` — 8 leading digits then the title slug.
    assert!(name.ends_with("-adopt-postgresql.md"), "got {name}");
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    assert_eq!(digits.len(), 8, "expected an 8-digit date prefix in {name}");

    // Identity lives in the filename + the mirrored `reference:` frontmatter;
    // the body has no heading at all.
    let content = fs::read_to_string(p).unwrap();
    let stem = name.strip_suffix(".md").unwrap();
    assert!(content.contains(&format!("reference: {stem}")), "{content}");
    assert!(!content.contains("\n# "), "{content}");
}

#[test]
fn date_scheme_list_show_status_by_slug() {
    let dir = TempDir::new().unwrap();
    adroit_date(&dir)
        .args(["new", "Adopt PostgreSQL", "--no-edit"])
        .assert()
        .success();
    let slug = sole_stem(dir.path());

    // The list row shows the date slug as the identifier.
    adroit_date(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(&slug))
        .stdout(predicate::str::contains("Adopt PostgreSQL"));

    // `show <slug>` resolves through the naming seam.
    adroit_date(&dir)
        .args(["show", &slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("Adopt PostgreSQL"));

    // `set-status <slug> accepted` rewrites the frontmatter in place — the
    // file keeps its slug and never moves.
    adroit_date(&dir)
        .args(["set-status", &slug, "accepted"])
        .assert()
        .success();
    assert_eq!(sole_stem(dir.path()), slug);
    let content = fs::read_to_string(&adr_files(dir.path())[0]).unwrap();
    assert!(content.contains("status: accepted"), "{content}");
}

#[test]
fn date_scheme_set_review_by_slug() {
    let dir = TempDir::new().unwrap();
    adroit_date(&dir)
        .args(["new", "Adopt PostgreSQL", "--no-edit"])
        .assert()
        .success();
    let slug = sole_stem(dir.path());
    let path = adr_files(dir.path())[0].clone();

    adroit_date(&dir)
        .args(["set-review", &slug, "2026-12-31"])
        .assert()
        .success();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("review_by: 2026-12-31"), "got: {content}");
}

#[test]
fn uuid_scheme_check_flags_a_duplicate_identifier() {
    let dir = TempDir::new().unwrap();
    let scheme = ["--naming", "uuid"];
    adroit(&dir)
        .args(scheme)
        .args(["new", "Adopt PostgreSQL", "--no-edit"])
        .assert()
        .success();
    let p = adr_files(dir.path())[0].clone();
    let stem = sole_stem(dir.path());
    let uuid: String = stem.chars().take_while(|c| *c != '-').collect();

    // A second file with the same uuid but a different slug parses to the same
    // identity — `check` must flag the duplicate even though the scheme has no
    // number.
    fs::copy(&p, corpus(&dir).join(format!("{uuid}-other-slug.md"))).unwrap();

    adroit(&dir)
        .args(scheme)
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate identifier"));
}

#[test]
fn date_scheme_rejects_numeric_only_commands() {
    let dir = TempDir::new().unwrap();
    adroit_date(&dir)
        .args(["new", "Adopt PostgreSQL", "--no-edit"])
        .assert()
        .success();

    // renumber / review are number-shaped and don't apply to a non-numeric
    // scheme — they bail with a clear message, not a confusing "not found".
    adroit_date(&dir)
        .args(["renumber", "1", "2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires a numeric naming scheme"));
    adroit_date(&dir)
        .args(["review", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires a numeric naming scheme"));
}

#[test]
fn date_scheme_supersede_by_slug() {
    let dir = TempDir::new().unwrap();
    adroit_date(&dir)
        .args(["new", "Old approach", "--no-edit"])
        .assert()
        .success();
    adroit_date(&dir)
        .args(["new", "New approach", "--no-edit"])
        .assert()
        .success();
    let slug_of = |needle: &str| {
        adr_files(dir.path())
            .into_iter()
            .find(|p| p.to_str().unwrap().contains(needle))
            .map(|p| p.file_stem().unwrap().to_str().unwrap().to_string())
            .unwrap()
    };
    let old_slug = slug_of("old-approach");
    let new_slug = slug_of("new-approach");

    // Supersede by slug: the old ADR's frontmatter records the slug ref in
    // place; the new ADR gets a reciprocal "Supersedes [<old-slug>]" note.
    adroit_date(&dir)
        .args(["supersede", &new_slug, &old_slug])
        .assert()
        .success();

    let old = corpus(&dir).join(format!("{old_slug}.md"));
    assert!(old.exists(), "the old ADR never moves");
    let old_body = fs::read_to_string(&old).unwrap();
    assert!(old_body.contains("status: superseded"), "{old_body}");
    assert!(
        old_body.contains(&format!("superseded_by: {new_slug}")),
        "{old_body}"
    );
    let new_body = fs::read_to_string(corpus(&dir).join(format!("{new_slug}.md"))).unwrap();
    assert!(new_body.contains(&format!("Supersedes [{old_slug}]")));

    // The repo stays consistent (links resolve, no broken supersession refs).
    adroit_date(&dir).arg("check").assert().success();
}

#[test]
fn uuid_scheme_new_and_show_by_prefix() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["--naming", "uuid", "new", "Adopt PostgreSQL", "--no-edit"])
        .assert()
        .success();

    let name_stem = sole_stem(dir.path());
    // `<26-char-ulid>-<slug>` — the page id is the identity, the slug is for
    // humans (the `uuid` scheme derives its reference from the ULID id).
    let uuid: String = name_stem.chars().take_while(|c| *c != '-').collect();
    assert_eq!(
        uuid.len(),
        26,
        "expected a 26-char ulid prefix in {name_stem}"
    );

    // Addressable by a leading prefix of the uuid (what `list`/display shows).
    let prefix = &uuid[..8];
    adroit(&dir)
        .args(["--naming", "uuid", "show", prefix])
        .assert()
        .success()
        .stdout(predicate::str::contains("Adopt PostgreSQL"));
}

// ---------------------------------------------------------------------------
// `adroit seed` — the one-way legacy-corpus bootstrap (ADR-0020)
// ---------------------------------------------------------------------------

#[test]
fn seed_bootstraps_a_legacy_corpus_and_refuses_a_second_run() {
    let dir = TempDir::new().unwrap();
    // A small legacy by_status corpus: H1 identity, `> State:` banner,
    // `## Status` region with `Created:` provenance — the pre-KB shape.
    let legacy = TempDir::new().unwrap();
    fs::create_dir_all(legacy.path().join("accepted")).unwrap();
    fs::create_dir_all(legacy.path().join("proposed")).unwrap();
    fs::write(
        legacy.path().join("accepted/0001-adopt-adrs.md"),
        "# ADR-0001: Adopt ADRs\n\n> State: Accepted\n\n## Status\n\nAccepted\nCreated: 2026-04-10\n\n## Context\n\nWe keep records.\n",
    )
    .unwrap();
    fs::write(
        legacy.path().join("proposed/0002-use-redis.md"),
        "# ADR-0002: Use Redis\n\n## Status\n\nProposed\n\n## Context\n\nCache.\n",
    )
    .unwrap();

    // Dry run first: plans, writes nothing.
    adroit(&dir)
        .args(["seed", "--from"])
        .arg(legacy.path())
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Would seed 2 ADR(s)"));
    assert!(adr_files(dir.path()).is_empty(), "dry run must not write");

    // Apply: KB pages land in wiki/decisions — number → reference, directory →
    // status, `Created:` → `created`, body stripped of H1/banner/status region.
    adroit(&dir)
        .args(["seed", "--from"])
        .arg(legacy.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Seeded 2 ADR(s)"));
    let one = fs::read_to_string(corpus(&dir).join("0001-adopt-adrs.md")).unwrap();
    assert!(one.starts_with("---\n"), "{one}");
    assert!(one.contains("reference: ADR-0001"), "{one}");
    assert!(one.contains("status: accepted"), "{one}");
    assert!(one.contains("created: 2026-04-10T00:00:00Z"), "{one}");
    assert!(!one.contains("# ADR-0001"), "{one}");
    assert!(!one.contains("> State:"), "{one}");
    assert!(!one.contains("## Status"), "{one}");
    assert!(one.contains("## Context"), "{one}");
    adroit(&dir).arg("check").assert().success();

    // One-shot by design: a second seed refuses the now non-empty space.
    adroit(&dir)
        .args(["seed", "--from"])
        .arg(legacy.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("already contains"));
}

// ---------------------------------------------------------------------------
// Forge integration (issue #4)
// ---------------------------------------------------------------------------

#[test]
fn new_without_forge_has_no_references_section() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Plain ADR", "--no-edit"])
        .assert()
        .success();
    let body = fs::read_to_string(corpus(&dir).join("0001-plain-adr.md")).unwrap();
    assert!(
        !body.contains("## References"),
        "bare `new` must not touch forge"
    );
}

#[cfg(not(feature = "forge"))]
#[test]
fn forge_flag_is_absent_without_the_feature() {
    // A no-forge build doesn't expose `--forge` at all (it's `#[cfg]`-gated), so
    // passing it is a hard error, not a silent no-op.
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit", "--forge"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}

#[cfg(not(feature = "forge"))]
#[test]
fn forge_only_commands_are_absent_without_the_feature() {
    // init/auth/sync/notify are `#[cfg(feature = "forge")]` — a no-forge build
    // doesn't have them at all (publish stays — it's offline).
    let dir = TempDir::new().unwrap();
    for sub in ["auth", "init", "sync", "notify", "reconcile"] {
        adroit(&dir)
            .arg(sub)
            .assert()
            .failure()
            .stderr(predicate::str::contains("unrecognized subcommand"));
    }
}

#[cfg(feature = "forge")]
#[test]
fn new_with_forge_dry_run_previews_plan_without_network() {
    // Point config at a temp XDG dir with a github forge block + a fake token,
    // so the adapter constructs but --dry-run returns before any HTTP/git.
    let home = TempDir::new().unwrap();
    let cfgdir = home.path().join("adroit");
    fs::create_dir_all(&cfgdir).unwrap();
    fs::write(
        cfgdir.join("config.yaml"),
        "forge:\n  provider: github\n  repo: owner/repo\n",
    )
    .unwrap();

    let dir = TempDir::new().unwrap();
    space(dir.path());
    let before = snapshot(dir.path());
    let mut cmd = Command::cargo_bin("adroit").unwrap();
    cmd.env("EDITOR", "true")
        .env("VISUAL", "true")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path())
        .env("ADROIT_GITHUB_TOKEN", "fake-token")
        .arg("--dir")
        .arg(dir.path())
        .args(["new", "Adopt Postgres", "--no-edit", "--forge", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run: would create"))
        .stdout(predicate::str::contains("Forge plan"))
        .stdout(predicate::str::contains("create issue"));

    // A true dry run creates nothing — not even the local ADR file.
    assert!(
        adr_files(dir.path()).is_empty(),
        "new --dry-run must not write the ADR"
    );
    assert_eq!(
        before,
        snapshot(dir.path()),
        "new --dry-run must leave the space untouched"
    );
}

#[cfg(feature = "forge")]
#[test]
fn init_yes_writes_config_env_template_and_hook() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    let adrs = repo.join("adrs");
    fs::create_dir_all(&adrs).unwrap();
    space(&adrs);
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
    };
    git(&["init", "-q"]);
    git(&["remote", "add", "origin", "git@github.com:acme/widgets.git"]);
    let cfg = repo.join("cfg");

    // `--yes` = full non-interactive setup from the detected remote.
    Command::cargo_bin("adroit")
        .unwrap()
        .current_dir(repo)
        .env("XDG_CONFIG_HOME", &cfg)
        .env("XDG_DATA_HOME", repo.join("data"))
        .env("ADROIT_DIR", &adrs)
        .env("EDITOR", "true")
        .env("VISUAL", "true")
        .args(["init", "--yes"])
        .assert()
        .success();

    let conf = fs::read_to_string(cfg.join("adroit").join("config.yaml")).unwrap();
    assert!(conf.contains("provider: github"), "config:\n{conf}");
    assert!(conf.contains("repo: acme/widgets"), "config:\n{conf}");
    assert!(
        fs::read_to_string(repo.join(".env"))
            .unwrap()
            .contains("ADROIT_DIR=")
    );
    // The repo-local template lands in the corpus dir inside the space.
    assert!(adrs.join("wiki/decisions/adr-template.md").exists());
    let hook = repo.join(".git").join("hooks").join("pre-commit");
    assert!(hook.exists(), "pre-commit hook not installed");
    assert!(fs::read_to_string(&hook).unwrap().contains("adroit check"));
}

// ---------------------------------------------------------------------------
// `-o json` output for the read verbs (agent-consumable CLI)
// ---------------------------------------------------------------------------

/// Run `args` against `dir`, assert success, and parse stdout as JSON.
fn json_ok(dir: &TempDir, args: &[&str]) -> serde_json::Value {
    let out = adroit(dir).args(args).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("`{args:?}` did not emit valid JSON: {e}\n{text}"))
}

#[test]
fn list_json_emits_array_of_summaries() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "First decision", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Second decision", "--no-edit"])
        .assert()
        .success();

    let v = json_ok(&dir, &["list", "-o", "json"]);
    assert_eq!(v.as_array().map(|a| a.len()), Some(2));
    assert_eq!(v[0]["reference"], "ADR-0001");
    assert_eq!(v[0]["title"], "First decision");
    assert_eq!(v[0]["status"], "proposed");
}

#[test]
fn list_json_empty_repo_is_empty_array() {
    let dir = TempDir::new().unwrap();
    let v = json_ok(&dir, &["list", "-o", "json"]);
    assert_eq!(v.as_array().map(|a| a.len()), Some(0));
}

#[test]
fn show_json_emits_detail_object() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Only decision", "--no-edit"])
        .assert()
        .success();
    let v = json_ok(&dir, &["show", "1", "-o", "json"]);
    // AdrDetail flattens the summary to the top level alongside `body`.
    assert_eq!(v["reference"], "ADR-0001");
    assert_eq!(v["title"], "Only decision");
    assert!(v["body"].is_string(), "detail JSON carries the raw body");
    // The status timeline retired with the layouts (ADR-0020): no `history`.
    assert!(v.get("history").is_none(), "{v}");
}

#[test]
fn search_json_emits_matching_array() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt Postgres", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Adopt Redis", "--no-edit"])
        .assert()
        .success();
    let v = json_ok(&dir, &["search", "Postgres", "-o", "json"]);
    assert_eq!(v.as_array().map(|a| a.len()), Some(1));
    assert_eq!(v[0]["title"], "Adopt Postgres");
}

#[test]
fn stats_json_has_totals_and_status_breakdown() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A decision", "--no-edit"])
        .assert()
        .success();
    let v = json_ok(&dir, &["stats", "-o", "json"]);
    assert_eq!(v["total"], 1);
    assert!(v["by_status"].is_array());
}

#[test]
fn graph_json_has_nodes_and_edges() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A decision", "--no-edit"])
        .assert()
        .success();
    let v = json_ok(&dir, &["graph", "-o", "json"]);
    assert_eq!(v["nodes"].as_array().map(|a| a.len()), Some(1));
    assert!(v["edges"].is_array());
}

#[test]
fn check_json_clean_repo_exits_zero() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A decision", "--no-edit"])
        .assert()
        .success();
    let v = json_ok(&dir, &["check", "-o", "json"]);
    assert_eq!(v["checked"], 1);
    assert_eq!(v["problems"].as_array().map(|a| a.len()), Some(0));
}

#[test]
fn check_json_broken_link_emits_json_and_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A decision", "--no-edit"])
        .assert()
        .success();
    // Inject a broken cross-ADR link (ADR-0099 doesn't exist) → Error severity.
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    append_body(&file, "\nSee [ADR-0099](./0099-ghost.md) for context.\n");

    // The CI gate still holds (non-zero exit), but stdout is still valid JSON.
    let out = adroit(&dir)
        .args(["check", "-o", "json"])
        .assert()
        .failure();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("check -o json must emit JSON even on failure: {e}\n{text}"));
    assert!(
        !v["problems"].as_array().unwrap().is_empty(),
        "expected the broken link to be reported as a problem"
    );
}

#[test]
fn read_verbs_default_to_human_output() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "A decision", "--no-edit"])
        .assert()
        .success();
    // No -o flag → human table (header line), not JSON.
    adroit(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Status"))
        .stdout(predicate::str::starts_with("[").not());
}

// ---------------------------------------------------------------------------
// `new --interview` (AI-assisted authoring; FakeProvider via ADROIT_AI_FAKE)
// ---------------------------------------------------------------------------

#[test]
fn new_interview_drafts_body_but_keeps_identity_mechanical() {
    let dir = TempDir::new().unwrap();
    let canned = "## Context and Problem Statement\n\nDrafted by the fake provider.\n\n\
                  ## Decision Outcome\n\nChosen option: **A**.";
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--interview", "--no-edit"])
        .env("ADROIT_AI_FAKE", canned)
        .write_stdin("ctx\ndrivers\noptions\nrisks\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    let file = adr_files(dir.path()).into_iter().next().unwrap();
    let body = fs::read_to_string(&file).unwrap();
    // Identity + status stay mechanical (frontmatter); the AI prose lands under
    // the marker.
    assert!(body.contains("title: Adopt feature flags"), "{body}");
    assert!(body.contains("status: proposed"), "{body}");
    assert!(
        body.contains("<!-- adroit:ai-suggested -->"),
        "AI marker present"
    );
    assert!(
        body.contains("Drafted by the fake provider."),
        "AI prose present"
    );

    // The result is a valid repo and the status getter still works.
    adroit(&dir).arg("check").assert().success();
    adroit(&dir)
        .args(["status", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("proposed"));
}

#[test]
fn new_interview_without_a_provider_keeps_the_plain_template() {
    let dir = TempDir::new().unwrap();
    // No ADROIT_AI_FAKE and no provider configured → degrade gracefully. The
    // wording differs by build (lacking the `ai` feature vs. AI not enabled),
    // but both keep the plain template — assert that shared, on-point phrase.
    adroit(&dir)
        .args(["new", "Some decision", "--interview", "--no-edit"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("could not run")
                .and(predicate::str::contains("plain template")),
        );
    // The ADR still exists and is valid (the plain template).
    adroit(&dir).arg("check").assert().success();
}

// ---------------------------------------------------------------------------
// `adroit plan` (AI implementation plan; read-only; FakeProvider seam)
// ---------------------------------------------------------------------------

#[test]
fn plan_generates_an_implementation_plan_via_fake_provider() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["plan", "1"])
        .env("ADROIT_AI_FAKE", "## Implementation Plan\n\n- [ ] Step one")
        .assert()
        .success()
        .stdout(predicate::str::contains("Implementation Plan"))
        .stdout(predicate::str::contains("Step one"));
    // Read-only: the ADR is untouched and the repo stays valid.
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn plan_without_a_provider_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["plan", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AI feature").or(predicate::str::contains("AI provider")));
}

// ---------------------------------------------------------------------------
// `adroit plan --save` — plan persistence (ADR-0008)
// ---------------------------------------------------------------------------

/// Seed one accepted ADR and persist a fake-generated plan into it.
fn seed_saved_plan(dir: &TempDir, plan: &str) {
    adroit(dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    adroit(dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();
    adroit(dir)
        .args(["plan", "1", "--save"])
        .env("ADROIT_AI_FAKE", plan)
        .assert()
        .success()
        .stdout(predicate::str::contains("Saved implementation plan"));
}

#[test]
fn plan_save_persists_a_marked_implementation_section() {
    let dir = TempDir::new().unwrap();
    seed_saved_plan(&dir, "1. Create the schema.\n2. Add tests.");
    let body = fs::read_to_string(corpus(&dir).join("0001-use-postgresql.md")).unwrap();
    assert!(body.contains("## Implementation"), "{body}");
    assert!(body.contains("<!-- adroit:plan -->"), "{body}");
    assert!(body.contains("<!-- /adroit:plan -->"), "{body}");
    assert!(body.contains("1. Create the schema."), "{body}");
    // The document is still a valid corpus member.
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn plan_reads_the_stored_plan_deterministically_without_a_provider() {
    let dir = TempDir::new().unwrap();
    seed_saved_plan(&dir, "1. Create the schema.\n2. Add tests.");
    let before = snapshot(dir.path());

    // No AI env at all (the `adroit` helper scrubs it): the stored plan comes
    // back, exit 0, and the read indicates it is stored.
    let assert = adroit(&dir).args(["plan", "1"]).assert().success();
    let out1 = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out1.contains("1. Create the schema."), "{out1}");
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("stored"),
        "read must say it's stored: {stderr}"
    );

    // Byte-deterministic: a second read prints identical bytes …
    let assert = adroit(&dir).args(["plan", "1"]).assert().success();
    let out2 = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(out1, out2, "stored reads must be byte-identical");
    // … and reading mutates nothing.
    assert_eq!(before, snapshot(dir.path()), "a stored read must not write");
}

#[test]
fn plan_o_json_carries_the_additive_stored_flag() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    // Fresh generation (nothing persisted): stored=false, shape otherwise as before.
    let assert = adroit(&dir)
        .args(["plan", "1", "-o", "json"])
        .env("ADROIT_AI_FAKE", "1. Step.")
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(v["reference"], "ADR-0001");
    assert_eq!(v["title"], "Use PostgreSQL");
    assert_eq!(v["plan"], "1. Step.");
    assert_eq!(v["stored"], false);

    // After --save, a provider-free read returns the stored plan with stored=true.
    adroit(&dir)
        .args(["plan", "1", "--save"])
        .env("ADROIT_AI_FAKE", "1. Step.")
        .assert()
        .success();
    let assert = adroit(&dir)
        .args(["plan", "1", "-o", "json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(v["reference"], "ADR-0001");
    assert_eq!(v["plan"], "1. Step.");
    assert_eq!(v["stored"], true);
}

#[test]
fn plan_save_refuses_to_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    seed_saved_plan(&dir, "1. Original plan.");
    let before = snapshot(dir.path());
    adroit(&dir)
        .args(["plan", "1", "--save"])
        .env("ADROIT_AI_FAKE", "1. Different plan.")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--force"));
    assert_eq!(
        before,
        snapshot(dir.path()),
        "a refused save must not write"
    );

    // `--force` replaces the stored plan (and only that section).
    adroit(&dir)
        .args(["plan", "1", "--save", "--force"])
        .env("ADROIT_AI_FAKE", "1. Different plan.")
        .assert()
        .success();
    let body = fs::read_to_string(corpus(&dir).join("0001-use-postgresql.md")).unwrap();
    assert!(body.contains("1. Different plan."), "{body}");
    assert!(!body.contains("1. Original plan."), "{body}");
    assert_eq!(body.matches("## Implementation").count(), 1, "{body}");
}

#[test]
fn plan_save_force_with_the_same_plan_is_byte_identical() {
    // The converge property on the write path: re-saving the same plan over
    // itself rewrites nothing (minimal-diff invariant, ADR-0003).
    let dir = TempDir::new().unwrap();
    seed_saved_plan(&dir, "1. Same plan.");
    let before = snapshot(dir.path());
    adroit(&dir)
        .args(["plan", "1", "--save", "--force"])
        .env("ADROIT_AI_FAKE", "1. Same plan.")
        .assert()
        .success();
    assert_eq!(before, snapshot(dir.path()));
}

#[test]
fn plan_regenerate_skips_the_stored_plan_without_writing() {
    let dir = TempDir::new().unwrap();
    seed_saved_plan(&dir, "1. The stored plan.");
    let before = snapshot(dir.path());

    // `--regenerate` is an explicit fresh AI call: stored plan ignored, output
    // not persisted (stored=false in the envelope).
    let assert = adroit(&dir)
        .args(["plan", "1", "--regenerate", "-o", "json"])
        .env("ADROIT_AI_FAKE", "1. A fresh plan.")
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(v["plan"], "1. A fresh plan.");
    assert_eq!(v["stored"], false);
    assert_eq!(before, snapshot(dir.path()), "--regenerate must not write");

    // Without a provider it bails like any generation (the stored plan does
    // not satisfy an explicit regeneration request).
    adroit(&dir)
        .args(["plan", "1", "--regenerate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AI feature").or(predicate::str::contains("AI provider")));
}

#[test]
fn plan_save_refuses_a_hand_written_implementation_section() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    let path = corpus(&dir).join("0001-use-postgresql.md");
    append_body(&path, "\n## Implementation\n\nBy hand.\n");
    let before = snapshot(dir.path());
    adroit(&dir)
        .args(["plan", "1", "--save"])
        .env("ADROIT_AI_FAKE", "1. Step.")
        .assert()
        .failure()
        .stderr(predicate::str::contains("hand-written"));
    assert_eq!(
        before,
        snapshot(dir.path()),
        "unmarked content is never touched"
    );
}

#[test]
fn show_o_json_carries_the_stored_plan() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    // Before a save: the additive field is null.
    let assert = adroit(&dir)
        .args(["show", "1", "-o", "json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert!(v["plan"].is_null(), "{v}");

    adroit(&dir)
        .args(["plan", "1", "--save"])
        .env("ADROIT_AI_FAKE", "1. Create the schema.")
        .assert()
        .success();
    let assert = adroit(&dir)
        .args(["show", "1", "-o", "json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(v["plan"], "1. Create the schema.");
}

#[test]
fn draft_preserves_a_stored_plan() {
    // A stored plan (ADR-0008) is adroit-managed decision content, not prose:
    // the AI body splice (`draft` / `compose` / `import --ai`) replaces the
    // prose around it but must not silently discard it.
    let dir = TempDir::new().unwrap();
    seed_saved_plan(&dir, "1. The persisted plan.");
    adroit(&dir)
        .args(["draft", "1", "--no-edit"])
        .env(
            "ADROIT_AI_FAKE",
            "## Context and Problem Statement\n\nRedrafted.",
        )
        .write_stdin("")
        .assert()
        .success();
    let body = fs::read_to_string(corpus(&dir).join("0001-use-postgresql.md")).unwrap();
    assert!(body.contains("Redrafted."), "{body}");
    assert!(body.contains("1. The persisted plan."), "{body}");
    let assert = adroit(&dir)
        .args(["plan", "1", "-o", "json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(v["plan"], "1. The persisted plan.");
    assert_eq!(v["stored"], true);
}

#[test]
fn plan_save_round_trips_a_plan_with_a_subheading() {
    // The splice rides `Store::set_body` → `frontmatter::serialize`; free-form
    // plan markdown (its own `## ` sub-heading included) must survive verbatim.
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["plan", "1", "--save"])
        .env(
            "ADROIT_AI_FAKE",
            "1. Create the schema.\n\n## Rollout\n\n- [ ] Staging.",
        )
        .assert()
        .success();
    // Provider-free read returns the plan verbatim — sub-heading included.
    let assert = adroit(&dir)
        .args(["plan", "1", "-o", "json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(
        v["plan"],
        "1. Create the schema.\n\n## Rollout\n\n- [ ] Staging."
    );
    assert_eq!(v["stored"], true);
    // The repo stays valid and a second provider-free read is byte-stable.
    adroit(&dir).arg("check").assert().success();
    let before = snapshot(dir.path());
    adroit(&dir).args(["plan", "1"]).assert().success();
    assert_eq!(before, snapshot(dir.path()));
}

// ---------------------------------------------------------------------------
// `adroit lint` (authoring-quality checks; read-only)
// ---------------------------------------------------------------------------

#[test]
fn lint_flags_a_fresh_template_and_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["lint", "1"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("still holds only its prompt"));
}

#[test]
fn lint_json_emits_findings_on_stdout() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    let out = adroit(&dir)
        .args(["lint", "1", "-o", "json"])
        .assert()
        .failure();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(!v.as_array().unwrap().is_empty());
    assert_eq!(v[0]["source"], "mechanical");
}

#[test]
fn lint_passes_a_complete_adr() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    // Overwrite the body with a fully-filled ADR (no placeholders, 2 options,
    // a real downside) — prose only, per the KB page shape.
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    write_body(
        &file,
        "## Stakeholders\n\n- Platform team\n\n## Context and Problem Statement\n\n\
         We ship risky changes and want to decouple deploy from release.\n\n\
         ## Considered Options\n\n1. Feature flags\n2. Long-lived branches\n\n\
         ## Decision Outcome\n\nChosen: feature flags, to decouple deploy from release.\n\n\
         ### Negative Consequences\n\n- Flag debt accumulates and needs periodic cleanup.\n",
    );
    adroit(&dir)
        .args(["lint", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no lint findings"));
}

#[test]
fn lint_ai_without_a_provider_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["lint", "1", "--ai"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AI feature").or(predicate::str::contains("AI provider")));
}

// ---------------------------------------------------------------------------
// Created-date provenance (frontmatter `created:`)
// ---------------------------------------------------------------------------

#[test]
fn new_stamps_created_in_frontmatter() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    let content = fs::read_to_string(&file).unwrap();
    let stamped = content
        .lines()
        .find_map(|l| l.strip_prefix("created: "))
        .expect("a created: frontmatter field");
    // And it parses as the ADR's creation date (same day).
    let v = json_ok(&dir, &["show", "1", "-o", "json"]);
    let created = v["created"].as_str().unwrap();
    assert_eq!(&created[..10], &stamped[..10], "{created} vs {stamped}");
}

#[test]
fn created_is_byte_stable_across_set_status_and_plan_save_without_git() {
    // Run-1 regression (the template-corpus M3 read-path rehearsal): `created` was
    // mtime-derived on a non-git corpus, so `set-status` and `plan --save`
    // rewrites re-stamped it to "now", misleading any consumer treating it
    // as decision provenance. The page persists the date in frontmatter
    // (stamped once by `new`), so rewrites can't move it. The same class of
    // wall-clock fragility flaked the iteration-2 integration gate
    // (`created <= last_modified` under a clock step).
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    // Backdate the file so any mtime-derived fallback is visibly different
    // from "now" — making a regression deterministic, not clock-dependent.
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    let backdated = std::time::SystemTime::now() - std::time::Duration::from_secs(72 * 3600);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(backdated)
        .unwrap();

    let created_of = |dir: &TempDir| -> String {
        let out = adroit(dir)
            .args(["show", "1", "-o", "json"])
            .assert()
            .success();
        let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["created"].as_str().unwrap().to_string()
    };

    let before = created_of(&dir);
    adroit(&dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();
    assert_eq!(created_of(&dir), before, "set-status re-stamped `created`");
    adroit(&dir)
        .args(["plan", "1", "--save"])
        .env("ADROIT_AI_FAKE", "1. Do the thing.")
        .assert()
        .success();
    assert_eq!(created_of(&dir), before, "plan --save re-stamped `created`");
}

#[test]
fn lint_accepts_h2_negative_consequences() {
    // Run-1 regression (iteration-1 full loop): 2 of 11 seeded ADRs failed
    // lint solely because the model wrote `## Negative Consequences` at h2
    // where the template nests `###`. Depth is shape, not substance — both
    // pass.
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt CI", "--no-edit"])
        .assert()
        .success();
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    write_body(
        &file,
        "## Context and Problem Statement\n\nWe need automated builds.\n\n\
         ## Considered Options\n\n1. Jenkins\n2. Forge-native CI\n\n\
         ## Decision Outcome\n\nChosen: Jenkins, because plugins.\n\n\
         ## Positive Consequences\n\n* Faster feedback loops.\n\n\
         ## Negative Consequences\n\n* Initial investment required.\n",
    );
    adroit(&dir).args(["lint", "1"]).assert().success();
}

#[test]
fn lint_warns_on_repeated_sections_without_failing() {
    // Run-1 regression: a duplicated `## Stakeholders` skeleton echo was
    // lint-clean. It now surfaces as a warning finding — visible in human and
    // JSON output — but does not fail the exit (warnings advise).
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt CI", "--no-edit"])
        .assert()
        .success();
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    write_body(
        &file,
        "## Stakeholders\n\n- Platform team\n\n\
         ## Stakeholders\n\n- Platform team\n\n\
         ## Context and Problem Statement\n\nWe need automated builds.\n\n\
         ## Considered Options\n\n1. Jenkins\n2. Forge-native CI\n\n\
         ## Decision Outcome\n\nChosen: Jenkins, because plugins.\n\n\
         ### Negative Consequences\n\n- Initial investment required.\n",
    );
    adroit(&dir).args(["lint", "1"]).assert().success().stdout(
        predicate::str::contains("## Stakeholders").and(predicate::str::contains("warning")),
    );
    let out = adroit(&dir)
        .args(["lint", "1", "-o", "json"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let findings = v.as_array().unwrap();
    assert!(!findings.is_empty());
    assert!(
        findings.iter().all(|f| f["severity"] == "warning"),
        "{text}"
    );
}

// ---------------------------------------------------------------------------
// `adroit summarize` (one-paragraph AI TL;DR; read-only)
// ---------------------------------------------------------------------------

#[test]
fn summarize_prints_the_tldr_via_fake_provider() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["summarize", "1"])
        .env(
            "ADROIT_AI_FAKE",
            "A crisp one-paragraph TL;DR of the decision.",
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("A crisp one-paragraph TL;DR"));
    // Read-only.
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn summarize_without_a_provider_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["summarize", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AI feature").or(predicate::str::contains("AI provider")));
}

// ---------------------------------------------------------------------------
// `adroit compose` (instruction-driven AI body revision; writes the body)
// ---------------------------------------------------------------------------

#[test]
fn compose_revises_the_body_via_fake_provider() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["compose", "1", "expand the context", "--no-edit"])
        .env(
            "ADROIT_AI_FAKE",
            "## Context and Problem Statement\n\nRevised by compose.\n\n## Decision Outcome\n\nUse PostgreSQL.",
        )
        .assert()
        .success();
    // The revised, AI-marked prose landed; the mechanical identity is intact.
    adroit(&dir)
        .args(["show", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Revised by compose"))
        .stdout(predicate::str::contains("ADR-0001"));
    // The revision marks itself AI-suggested, and the repo still validates.
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn compose_requires_an_instruction() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    // Whitespace-only instruction is rejected before any provider call.
    adroit(&dir)
        .args(["compose", "1", "   "])
        .assert()
        .failure()
        .stderr(predicate::str::contains("instruction"));
}

#[test]
fn compose_without_a_provider_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["compose", "1", "tighten it"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AI feature").or(predicate::str::contains("AI provider")));
}

// ---------------------------------------------------------------------------
// `adroit related` / `dedupe` (mechanical TF-IDF similarity; read-only)
// ---------------------------------------------------------------------------

/// Three ADRs (two about databases, one about the frontend) with topical bodies.
fn three_topical_adrs(dir: &TempDir) {
    for t in [
        "Adopt PostgreSQL datastore",
        "Use Redis cache database",
        "Pick Vue frontend UI",
    ] {
        adroit(dir).args(["new", t, "--no-edit"]).assert().success();
    }
    for f in adr_files(dir.path()) {
        let name = f.file_name().unwrap().to_str().unwrap().to_string();
        let extra = if name.contains("postgresql") {
            "relational postgresql database storage sql persistence datastore"
        } else if name.contains("redis") {
            "redis caching database in-memory storage lookups persistence"
        } else {
            "vue react frontend browser dashboard interface components"
        };
        append_body(&f, &format!("\n{extra}\n"));
    }
}

#[test]
fn related_ranks_the_topically_similar_adr_first() {
    let dir = TempDir::new().unwrap();
    three_topical_adrs(&dir);
    let out = adroit(&dir)
        .args(["related", "1", "-o", "json"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let arr = v.as_array().unwrap();
    assert!(!arr.is_empty());
    // The other database ADR (ADR-0002) outranks the frontend one (ADR-0003).
    assert_eq!(v[0]["reference"], "ADR-0002");
    assert!(v[0]["score"].as_f64().unwrap() > 0.0);
}

#[test]
fn dedupe_emits_ranked_json() {
    let dir = TempDir::new().unwrap();
    three_topical_adrs(&dir);
    let out = adroit(&dir)
        .args(["dedupe", "1", "-o", "json"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v[0]["title"].is_string() && v[0]["score"].is_number());
}

#[test]
fn related_on_a_single_adr_is_empty() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Lonely decision", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["related", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No unlinked related ADRs"));
}

// ---------------------------------------------------------------------------
// `adroit ask` (mechanical retrieval + AI answer with citations)
// ---------------------------------------------------------------------------

#[test]
fn ask_answers_with_retrieved_sources_via_fake_provider() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt PostgreSQL datastore", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Use Vue frontend", "--no-edit"])
        .assert()
        .success();
    for f in adr_files(dir.path()) {
        let name = f.file_name().unwrap().to_str().unwrap().to_string();
        let extra = if name.contains("postgresql") {
            "relational postgresql database storage acid durability"
        } else {
            "vue frontend browser dashboard interface"
        };
        append_body(&f, &format!("\n{extra}\n"));
    }
    let out = adroit(&dir)
        .args(["ask", "Which database did we choose?", "-o", "json"])
        .env("ADROIT_AI_FAKE", "PostgreSQL, per the datastore ADR.")
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["answer"], "PostgreSQL, per the datastore ADR.");
    // The database ADR is among the retrieved sources.
    let sources = v["sources"].as_array().unwrap();
    assert!(sources.iter().any(|s| s == "ADR-0001"));
}

#[test]
fn ask_without_a_provider_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["ask", "anything?"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AI feature").or(predicate::str::contains("AI provider")));
}

// ---------------------------------------------------------------------------
// Help model: -h == --help (concise); --help-all is the full reference
// ---------------------------------------------------------------------------

#[test]
fn h_and_help_are_identical_and_concise_help_all_is_full() {
    let bin = || Command::cargo_bin("adroit").unwrap();
    let h = bin().arg("-h").output().unwrap();
    let help = bin().arg("--help").output().unwrap();
    assert!(h.status.success() && help.status.success());
    // -h and --help render the exact same (concise) help.
    assert_eq!(h.stdout, help.stdout, "`-h` and `--help` must be identical");

    let concise = String::from_utf8(h.stdout).unwrap();
    assert!(
        concise.contains("Author a decision:"),
        "concise help lists commands grouped by workflow stage"
    );
    assert!(
        !concise.contains("--naming"),
        "concise help must NOT dump the repo-shape options"
    );

    let all = String::from_utf8(bin().arg("--help-all").output().unwrap().stdout).unwrap();
    assert!(
        all.contains("--naming") && all.contains("--date-source"),
        "--help-all lists every option"
    );
}

#[test]
fn subcommand_h_and_help_also_match() {
    let bin = || Command::cargo_bin("adroit").unwrap();
    let h = bin().args(["new", "-h"]).output().unwrap();
    let help = bin().args(["new", "--help"]).output().unwrap();
    assert!(h.status.success() && help.status.success());
    assert_eq!(
        h.stdout, help.stdout,
        "`new -h` and `new --help` must match"
    );
}

// ---------------------------------------------------------------------------
// `new` duplicate-title guard (non-idempotent, but catches the accidental re-run)
// ---------------------------------------------------------------------------

#[test]
fn new_duplicate_title_warns_but_proceeds_non_interactive() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    // Non-interactive (assert_cmd has no TTY): warn + proceed, still allocating 0002.
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success()
        .stderr(predicate::str::contains("already use this title"))
        .stderr(predicate::str::contains("ADR-0001"));
    let v = json_ok(&dir, &["list", "-o", "json"]);
    assert_eq!(v.as_array().map(|a| a.len()), Some(2));
}

#[test]
fn new_force_skips_the_dup_guard() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit", "--force"])
        .assert()
        .success()
        .stderr(predicate::str::contains("already use this title").not());
}

#[test]
fn new_unique_title_has_no_dup_warning() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Use a message queue", "--no-edit"])
        .assert()
        .success()
        .stderr(predicate::str::contains("already use this title").not());
}

#[test]
fn new_interview_keeps_template_when_the_ai_call_fails() {
    let dir = TempDir::new().unwrap();
    // `__ERROR__` makes the fake provider fail (simulating an API credit/network error).
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--interview", "--no-edit"])
        .env("ADROIT_AI_FAKE", "__ERROR__")
        .write_stdin("ctx\ndrivers\noptions\nrisks\n")
        .assert()
        .success() // NOT an error exit — the ADR is created from the template
        .stderr(predicate::str::contains("AI draft failed"));
    // The ADR exists, is valid, and kept the template (no AI marker).
    adroit(&dir).arg("check").assert().success();
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    let body = fs::read_to_string(&file).unwrap();
    assert!(body.contains("title: Adopt feature flags"), "{body}");
    assert!(
        !body.contains("adroit:ai-suggested"),
        "no AI draft on failure"
    );
}

#[test]
fn check_warns_on_duplicate_titles_but_exits_zero() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["new", "Adopt feature flags", "--no-edit", "--force"])
        .assert()
        .success();
    // A warning (not an error): reported on stderr, but `check` still exits 0.
    adroit(&dir)
        .arg("check")
        .assert()
        .success()
        .stderr(predicate::str::contains("duplicate title"))
        .stdout(predicate::str::contains("warning"));
    let v = json_ok(&dir, &["check", "-o", "json"]);
    assert!(
        v["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "duplicate_title" && p["severity"] == "warning")
    );
}

// ---------------------------------------------------------------------------
// `adroit draft <ID>` — AI-complete an existing template ADR
// ---------------------------------------------------------------------------

#[test]
fn draft_fills_an_existing_template_adr_via_fake_provider() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Heal on main", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["draft", "1", "--no-edit"])
        .env(
            "ADROIT_AI_FAKE",
            "## Context and Problem Statement\n\nDrafted by the fake.\n\n\
             ## Decision Outcome\n\nChosen: relink on main.",
        )
        // `draft` runs the same interview as `new --interview` (4 questions).
        .write_stdin("ctx\ndrivers\noptions\nrisks\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("AI-drafted"));
    let file = adr_files(dir.path()).into_iter().next().unwrap();
    let body = fs::read_to_string(&file).unwrap();
    assert!(
        body.contains("title: Heal on main"),
        "identity kept: {body}"
    );
    assert!(body.contains("status: proposed"), "status kept: {body}");
    assert!(body.contains("adroit:ai-suggested"));
    assert!(body.contains("Drafted by the fake."));
    assert!(
        !body.contains("What should drive the choice"),
        "the template's post-Context prompts are replaced by the AI draft"
    );
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn draft_shows_cost_estimate_and_journals_the_raw_draft() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Adopt CQRS", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["draft", "1", "--no-edit"])
        .env(
            "ADROIT_AI_FAKE",
            "## Context and Problem Statement\n\nFake.\n\n## Decision Outcome\n\nChosen: CQRS.",
        )
        .write_stdin("c\nd\no\nr\n")
        .assert()
        .success()
        // the one-line pre-call cost notice …
        .stderr(predicate::str::contains("input tokens"))
        // … and the raw draft is journaled to a `.draft` sidecar.
        .stderr(predicate::str::contains("journaled"));
    // Exactly one `.md.draft` sidecar exists next to the ADR …
    let sidecars = fs::read_dir(corpus(&dir))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "draft"))
        .count();
    assert_eq!(
        sidecars, 1,
        "the raw draft is journaled to one .draft sidecar"
    );
    // … and the store ignores it (not an ADR): the repo still validates.
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn draft_without_a_provider_errors() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "X", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["draft", "1", "--no-edit"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AI feature").or(predicate::str::contains("AI provider")));
}

// ---- import (assessment → seed ADRs; issue #18 ingest seam) ----

/// A minimal `assessments`-shaped export: one domain, two named practices.
const ASSESSMENT_JSON: &str = r#"{
  "name": "Cloud Maturity",
  "domains": [
    { "name": "Security",
      "context": "Domain security context.",
      "practices": [
        { "name": "Secrets management",
          "context": "Secrets are committed to git today.",
          "value": "Leaked credentials are a top breach vector.",
          "risk": "A leak forces a painful rotation.",
          "effort": "M",
          "questions": [ {"text": "Are secrets stored outside source control?", "polarity": "positive"} ] },
        { "name": "Network segmentation",
          "context": "Flat network; one breach reaches everything.",
          "value": "Limits blast radius.",
          "risk": "Lateral movement on compromise." } ] }
  ]
}"#;

fn write_assessment(dir: &TempDir) -> PathBuf {
    let p = dir.path().join("assessment.json");
    fs::write(&p, ASSESSMENT_JSON).unwrap();
    p
}

#[test]
fn import_seeds_proposed_adrs_from_an_assessment() {
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .assert()
        .success()
        .stderr(predicate::str::contains("Seeded 2 proposed ADR(s)"));

    // One ADR per named practice.
    let files = adr_files(dir.path());
    assert_eq!(
        files.len(),
        2,
        "expected 2 seeded ADRs, got {}",
        files.len()
    );

    // The secrets ADR carries the marker, the practice context, the value driver,
    // the recorded signal, and the provenance note.
    let secrets = files
        .iter()
        .find(|p| p.to_string_lossy().contains("secrets-management"))
        .expect("secrets ADR present");
    let body = fs::read_to_string(secrets).unwrap();
    assert!(body.contains("adroit:seeded-from-assessment"));
    assert!(body.contains("Secrets are committed to git today."));
    assert!(body.contains("**Why it matters:** Leaked credentials are a top breach vector."));
    assert!(body.contains("**Estimated effort:** M"));
    assert!(body.contains("- Are secrets stored outside source control?"));
    assert!(body.contains("Seeded from assessment \"Cloud Maturity\""));

    // Identity/status stay mechanical: the seed is Proposed, and the repo is valid.
    adroit(&dir)
        .args(["status", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("proposed"));
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn import_is_rerunnable_and_skips_existing() {
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .assert()
        .success();
    // A second import adds nothing — both practices already have an ADR.
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .assert()
        .success()
        .stderr(predicate::str::contains("2 skipped"));
    assert_eq!(adr_files(dir.path()).len(), 2);
}

#[test]
fn import_dry_run_writes_nothing() {
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("would seed"))
        .stderr(predicate::str::contains("Would seed 2"));
    assert!(
        adr_files(dir.path()).is_empty(),
        "dry-run must not write any ADRs"
    );
}

#[test]
fn import_accepts_the_bundled_example_assessments() {
    // Guard the `examples/` files against rot: every format parses and seeds 4 ADRs.
    for file in [
        "examples/assessment.json",
        "examples/assessment.yaml",
        "examples/assessment.toml",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
        let dir = TempDir::new().unwrap();
        adroit(&dir)
            .args(["import", "--from-assessment"])
            .arg(&path)
            .arg("--dry-run")
            .assert()
            .success()
            .stderr(predicate::str::contains("Would seed 4"));
    }
}

#[test]
fn import_errors_clearly_on_a_missing_file() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["import", "--from-assessment", "/no/such/assessment.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reading assessment export"));
}

#[test]
fn import_reads_a_toml_export() {
    let dir = TempDir::new().unwrap();
    let export = dir.path().join("assessment.toml");
    fs::write(
        &export,
        r#"
name = "Cloud Maturity"
[[domains]]
name = "Security"
  [[domains.practices]]
  name = "Secrets management"
  context = "Secrets are committed to git today."
  value = "breach vector"
  risk = "painful rotation"
"#,
    )
    .unwrap();
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .assert()
        .success()
        .stderr(predicate::str::contains("Seeded 1 proposed ADR(s)"));
    let files = adr_files(dir.path());
    assert_eq!(files.len(), 1);
    assert!(
        fs::read_to_string(&files[0])
            .unwrap()
            .contains("Secrets are committed to git today.")
    );
}

#[test]
fn import_errors_clearly_on_malformed_input() {
    let dir = TempDir::new().unwrap();
    let bad = dir.path().join("bad.json");
    fs::write(&bad, "{ this is not valid json").unwrap();
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&bad)
        .assert()
        .failure()
        .stderr(predicate::str::contains("parsing assessment JSON"));
    // A clean error, not a panic, and nothing written.
    assert!(adr_files(dir.path()).is_empty());
}

#[test]
fn import_ai_fleshes_out_seeds_with_the_fake_provider() {
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .arg("--ai")
        .env("ADROIT_AI_FAKE", "A fleshed-out proposal body.")
        .assert()
        .success()
        .stderr(predicate::str::contains("Seeded 2 proposed ADR(s)"));
    let files = adr_files(dir.path());
    assert_eq!(files.len(), 2);
    // The AI pass marks the body and replaces the prose; status stays mechanical.
    let body = fs::read_to_string(&files[0]).unwrap();
    assert!(
        body.contains("adroit:ai-suggested"),
        "import --ai should mark the body AI-suggested"
    );
    assert!(body.contains("A fleshed-out proposal body."));
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn import_ai_degrades_to_mechanical_without_a_provider() {
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    adroit(&dir)
        .env_remove("ADROIT_AI_FAKE") // hermetic: no provider at all
        .args(["import", "--from-assessment"])
        .arg(&export)
        .arg("--ai")
        .assert()
        .success()
        .stderr(predicate::str::contains("had no AI provider"))
        .stderr(predicate::str::contains("Seeded 2"));
    // The mechanical seed still landed: seeded marker present, AI marker absent.
    let secrets = adr_files(dir.path())
        .into_iter()
        .find(|p| p.to_string_lossy().contains("secrets-management"))
        .unwrap();
    let body = fs::read_to_string(secrets).unwrap();
    assert!(body.contains("adroit:seeded-from-assessment"));
    assert!(!body.contains("adroit:ai-suggested"));
}

#[test]
fn import_ai_surfaces_sanitizer_drop_telemetry() {
    // Run-3 wart 1 (iteration-1 learnings; iteration-3 run-3 loop-summary): on
    // a fresh corpus the bracket-placeholder rule had zero survivors, but the
    // drops were SILENT — from the artifacts alone "the model emitted no
    // placeholder" was indistinguishable from "the sanitizer ate it". The fix:
    // count drops per rule and surface them.
    //
    // The model output here is run-3's confirmed shape — the run-2 ADR-0010
    // wart that run-3 re-exercised: a body closing with a horizontal rule and a
    // novel "[Insert …]" bracket placeholder (which the sanitizer drops, along
    // with the rule it orphans = 1 residue). Driven through the FAKE provider,
    // applied to BOTH seeded practices, so the run-level telemetry aggregates.
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    let canned = "## Decision Outcome\n\n\
                  Chosen: adopt the practice, because it addresses the drivers.\n\n\
                  ## Implementation Notes\n\n\
                  1. Establish the baseline.\n\
                  2. Roll it out incrementally.\n\n\
                  ---\n\n\
                  [Insert implementation plan or other details as needed]";

    // Human run: the stderr telemetry line names each non-zero rule.
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .arg("--ai")
        .env("ADROIT_AI_FAKE", canned)
        .assert()
        .success()
        // Two seeds × (1 bracket placeholder + 1 orphaned rule) = 2 + 2.
        .stderr(predicate::str::contains(
            "sanitized: 2 bracket-placeholder, 2 residue",
        ))
        .stderr(predicate::str::contains("Seeded 2 proposed ADR(s)"));

    // The wart's safety property still holds: no placeholder reaches the corpus.
    for f in adr_files(dir.path()) {
        let body = fs::read_to_string(&f).unwrap();
        assert!(
            !body.contains("[Insert implementation plan"),
            "placeholder leaked into {}: {body}",
            f.display()
        );
        assert!(
            !body.trim_end().ends_with("---"),
            "orphaned rule leaked into {}: {body}",
            f.display()
        );
    }

    // Machine run: the same drops carried in `-o json` under `sanitized`, with
    // only the non-zero rules present (house zeros-omitted convention).
    let dir2 = TempDir::new().unwrap();
    let export2 = write_assessment(&dir2);
    let out = adroit(&dir2)
        .args(["import", "--from-assessment"])
        .arg(&export2)
        .args(["--ai", "-o", "json"])
        .env("ADROIT_AI_FAKE", canned)
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("import --ai -o json invalid: {e}\n{text}"));
    assert_eq!(v["sanitized"]["bracket_placeholder"], 2, "{v}");
    assert_eq!(v["sanitized"]["residue"], 2, "{v}");
    // Rules that didn't fire are omitted entirely (not serialized as 0).
    assert!(v["sanitized"].get("skeleton_echo").is_none(), "{v}");
    assert!(v["sanitized"].get("identity_echo").is_none(), "{v}");
    assert!(v["sanitized"].get("marker_echo").is_none(), "{v}");
    assert_eq!(v["seeded"].as_array().unwrap().len(), 2);
}

#[test]
fn import_ai_omits_sanitized_when_nothing_dropped() {
    // A clean draft (nothing for the sanitizer to strip) carries NO `sanitized`
    // field and prints no telemetry line — the additive field stays absent, so
    // the legacy `-o json` shape is byte-for-byte unchanged for clean runs, and
    // the artifacts honestly read "the model emitted nothing bad".
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    let out = adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .args(["--ai", "-o", "json"])
        .env("ADROIT_AI_FAKE", "A clean, well-formed proposal body.")
        .assert()
        .success()
        // No telemetry line when nothing dropped.
        .stderr(predicate::str::contains("sanitized:").not());
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v.get("sanitized").is_none(), "clean run must omit it: {v}");
}

// ---- import -o json (machine seed summary) + the golden contract fixture ----

/// The vendored cross-repo contract fixture (see its comment header): the
/// `assessments` app's real-exporter golden, regenerated THERE via `just golden`.
fn golden_assessment() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden-assessment.yaml")
}

#[test]
fn import_golden_assessment_contract() {
    // The cross-repo contract pin: importing the assessments app's golden export
    // must keep seeding exactly this backlog. If the assessments export contract
    // drifts (renamed fields, restructured nesting), the counts/titles here
    // change and this test fails adroit's CI instead of silently seeding junk.
    let dir = TempDir::new().unwrap();
    let golden = golden_assessment();

    // Dry-run, human: the exact seed summary (count + title + provenance).
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&golden)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "would seed   Continuous Integration",
        ))
        .stderr(predicate::str::contains(
            "Would seed 1 proposed ADR(s) from assessment \"Release Readiness Sample\".",
        ));

    // Dry-run, JSON: the machine summary, pinned field-for-field.
    let out = adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&golden)
        .args(["--dry-run", "-o", "json"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("import -o json did not emit valid JSON: {e}\n{text}"));
    assert_eq!(v["assessment"], "Release Readiness Sample");
    assert_eq!(v["dry_run"], true);
    assert_eq!(
        v["seeded"],
        serde_json::json!([{
            "reference": null,
            "title": "Continuous Integration",
            "status": "proposed",
            "domain": "Delivery",
        }]),
        "the golden seed summary drifted: {v}"
    );
    assert_eq!(v["skipped"], serde_json::json!([]));
    assert!(adr_files(dir.path()).is_empty(), "dry-run must not write");

    // Wet run: the seed lands with the golden's content mapped into the body
    // (context → problem statement, value/risk → drivers, questions → signals).
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&golden)
        .assert()
        .success()
        .stderr(predicate::str::contains("Seeded 1 proposed ADR(s)"));
    let files = adr_files(dir.path());
    assert_eq!(files.len(), 1);
    let body = fs::read_to_string(&files[0]).unwrap();
    // Identity is frontmatter now (KB page), not an H1.
    assert!(body.contains("reference: ADR-0001"));
    assert!(body.contains("title: Continuous Integration"));
    assert!(body.contains("Every change is built and tested automatically"));
    assert!(body.contains("**Why it matters:** Defects surface minutes after they are introduced"));
    assert!(body.contains("**Risk if unaddressed:** Broken builds discovered at release time"));
    assert!(body.contains("**Estimated effort:** medium"));
    assert!(body.contains("- Does every change build and run the test suite in CI before merge?"));
    assert!(body.contains("- Are releases routinely delayed by manual verification?"));
    assert!(body.contains(
        "Seeded from assessment \"Release Readiness Sample\" — domain \"Delivery\" → practice \"Continuous Integration\"."
    ));
    adroit(&dir).arg("check").assert().success();
}

#[test]
fn import_json_emits_a_machine_seed_summary() {
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    // `json_ok` parses the WHOLE stdout, so the human `seeded <path>` lines must
    // be absent — stdout is pure JSON, human notes go to stderr.
    let v = json_ok(
        &dir,
        &[
            "import",
            "--from-assessment",
            export.to_str().unwrap(),
            "-o",
            "json",
        ],
    );
    assert_eq!(v["dry_run"], false);
    assert_eq!(v["assessment"], "Cloud Maturity");
    assert!(
        v["source"].as_str().unwrap().ends_with("assessment.json"),
        "source carries the export path: {v}"
    );
    let seeded = v["seeded"].as_array().unwrap();
    assert_eq!(seeded.len(), 2);
    assert_eq!(seeded[0]["reference"], "ADR-0001");
    assert_eq!(seeded[0]["title"], "Secrets management");
    assert_eq!(seeded[0]["status"], "proposed");
    assert_eq!(seeded[0]["domain"], "Security");
    assert_eq!(seeded[1]["reference"], "ADR-0002");
    assert_eq!(seeded[1]["title"], "Network segmentation");
    assert_eq!(v["skipped"], serde_json::json!([]));
    // The writes really happened — the summary reports, it doesn't just preview.
    assert_eq!(adr_files(dir.path()).len(), 2);
}

#[test]
fn import_json_reports_skipped_titles() {
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .assert()
        .success();
    // Re-import: everything dedupes — seeded empty, the skipped titles listed.
    let v = json_ok(
        &dir,
        &[
            "import",
            "--from-assessment",
            export.to_str().unwrap(),
            "-o",
            "json",
        ],
    );
    assert_eq!(v["seeded"], serde_json::json!([]));
    assert_eq!(
        v["skipped"],
        serde_json::json!(["Secrets management", "Network segmentation"])
    );
}

#[test]
fn import_json_with_no_named_practices_is_still_valid_json() {
    // The "nothing to seed" early return must keep stdout machine-parseable.
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("empty.json");
    fs::write(&p, r#"{"name":"Empty","domains":[]}"#).unwrap();
    let v = json_ok(
        &dir,
        &[
            "import",
            "--from-assessment",
            p.to_str().unwrap(),
            "-o",
            "json",
        ],
    );
    assert_eq!(v["assessment"], "Empty");
    assert_eq!(v["seeded"], serde_json::json!([]));
    assert_eq!(v["skipped"], serde_json::json!([]));
}

#[test]
fn import_ai_json_keeps_stdout_pure_with_the_fake_provider() {
    // `--ai` chatter (token estimates, provider notes) goes to stderr; the JSON
    // summary alone is stdout, so the assessments seam-check can pipe it.
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    let out = adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .args(["--ai", "-o", "json"])
        .env("ADROIT_AI_FAKE", "A fleshed-out proposal body.")
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("import --ai -o json stdout is not pure JSON: {e}\n{text}"));
    assert_eq!(v["seeded"].as_array().unwrap().len(), 2);
    // The AI pass really ran on the seeded ADRs.
    let body = fs::read_to_string(&adr_files(dir.path())[0]).unwrap();
    assert!(body.contains("adroit:ai-suggested"));
}

#[test]
fn plan_save_succeeds_after_an_ai_import_drafted_an_implementation_outline() {
    // The Adopt-slice regression, found in the M5 ollama dogfood rehearsal:
    // `import --ai` on a small model drafted a body carrying its own `# ` H1
    // and a bare `## Implementation` outline — the latter read as hand-written
    // and blocked `plan --save` forever (the import instruction even *asked*
    // for an implementation outline). The draft sanitizer drops the H1 and
    // retitles the section to `## Implementation notes`, so the seeded backlog
    // stays plan-ready and the slice runs end to end.
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    let canned = "# ADR-0001: Secrets management\n\n\
        ## Context and Problem Statement\n\nReal context.\n\n\
        ## Considered Options\n\n### Option 1: Vault\n\nManaged secrets.\n\n\
        ### Option 2: SOPS\n\nIn-repo encryption.\n\n\
        ## Decision Outcome\n\nChosen: Vault.\n\n\
        ### Negative Consequences\n\n- New infrastructure to run.\n\n\
        ## Implementation\n\n1. Stand up Vault.\n";
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .arg("--ai")
        .env("ADROIT_AI_FAKE", canned)
        .assert()
        .success();
    let body = fs::read_to_string(&adr_files(dir.path())[0]).unwrap();
    // No H1 at all — a KB page body is prose sections only; the model's echoed
    // identity heading is dropped by the sanitizer.
    assert_eq!(
        body.lines().filter(|l| l.starts_with("# ")).count(),
        0,
        "{body}"
    );
    // The model's outline no longer squats on the plan-managed heading.
    assert!(body.contains("## Implementation notes"), "{body}");
    assert!(body.contains("1. Stand up Vault."), "{body}");
    // lint counts the two `###`-recorded options (no false "two options"
    // finding) — the seeded draft passes CI-grade lint as-is.
    adroit(&dir).args(["lint", "1"]).assert().success();
    // … and the slice continues: accept → plan --save → the stored read.
    adroit(&dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();
    adroit(&dir)
        .args(["plan", "1", "--save"])
        .env("ADROIT_AI_FAKE", "1. Roll out per environment.")
        .assert()
        .success();
    let v = json_ok(&dir, &["plan", "1", "-o", "json"]);
    assert_eq!(v["stored"], serde_json::json!(true));
    assert_eq!(v["plan"], serde_json::json!("1. Roll out per environment."));
}

#[test]
fn import_ai_fleshes_out_seeds_against_live_ollama() {
    // Env-gated LIVE check (skipped in CI): with a local ollama serving
    // llama3.2, `import --ai` must produce model-fleshed (not mechanical-prompt)
    // bodies. Run via: ADROIT_LIVE_OLLAMA=1 cargo test import_ai_fleshes_out_seeds_against_live_ollama
    if std::env::var_os("ADROIT_LIVE_OLLAMA").is_none() {
        eprintln!("skipping live ollama check — set ADROIT_LIVE_OLLAMA=1 to run it");
        return;
    }
    let dir = TempDir::new().unwrap();
    let export = write_assessment(&dir);
    adroit(&dir)
        .args(["import", "--from-assessment"])
        .arg(&export)
        .args(["--ai", "-o", "json"])
        .env("ADROIT_AI_ENABLED", "true")
        .env("ADROIT_AI_PROVIDER", "ollama")
        .env("ADROIT_AI_MODEL", "llama3.2")
        .timeout(std::time::Duration::from_secs(600))
        .assert()
        .success();
    let files = adr_files(dir.path());
    assert_eq!(files.len(), 2);
    for f in &files {
        let body = fs::read_to_string(f).unwrap();
        assert!(
            body.contains("adroit:ai-suggested"),
            "{} lacks the AI marker — the live flesh-out did not run",
            f.display()
        );
        assert!(
            !body.contains("_List the options you actually weighed"),
            "{} still carries the mechanical authoring prompt",
            f.display()
        );
    }
}

// ---- plan -o json (structured plan artifact; #18 emit seam) ----

#[test]
fn plan_emits_a_structured_json_envelope() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    // The fake provider returns canned text; -o json wraps it with the ADR identity.
    adroit(&dir)
        .args(["plan", "1", "-o", "json"])
        .env("ADROIT_AI_FAKE", "1. Create the schema.\n2. Add tests.")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"reference\": \"ADR-0001\""))
        .stdout(predicate::str::contains("\"title\": \"Use PostgreSQL\""))
        .stdout(predicate::str::contains("\"plan\":"))
        .stdout(predicate::str::contains("Create the schema."));
}

#[test]
fn publish_to_renders_a_generator_tree() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();
    adroit(&dir)
        .args(["set-status", "1", "accepted"])
        .assert()
        .success();

    // `--to mkdocs` renders the MkDocs shape (nav config + docs/ page).
    let out = TempDir::new().unwrap();
    adroit(&dir)
        .args(["publish", "--to", "mkdocs", "--out"])
        .arg(out.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("mkdocs target"));
    assert!(out.path().join("mkdocs.yml").is_file());
    assert!(out.path().join("docs/0001-use-postgresql.md").is_file());

    // No `--to` defaults to the static-dir target (back-compat).
    let out2 = TempDir::new().unwrap();
    adroit(&dir)
        .args(["publish", "--out"])
        .arg(out2.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("static target"));
    assert!(out2.path().join("index.md").is_file());
    assert!(out2.path().join("0001-use-postgresql.md").is_file());
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_server_handshakes_lists_tools_and_runs_a_read_verb() {
    let dir = TempDir::new().unwrap();
    adroit(&dir)
        .args(["new", "Use PostgreSQL", "--no-edit"])
        .assert()
        .success();

    // initialize → notification (no reply) → tools/list → tools/call list.
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list","arguments":{}}}"#,
    ]
    .join("\n");

    let assert = adroit(&dir)
        .arg("mcp")
        .write_stdin(input)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // Handshake + a read verb exposed as a tool + the seeded ADR returned as JSON
    // from the in-process `tools/call`. A write verb is never a tool.
    assert!(stdout.contains("\"serverInfo\""), "{stdout}");
    assert!(stdout.contains("\"name\":\"list\""), "{stdout}");
    assert!(stdout.contains("Use PostgreSQL"), "{stdout}");
    assert!(!stdout.contains("\"name\":\"set-status\""), "{stdout}");
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_projected_tools_expose_no_escalating_flags() {
    // End-to-end pin of the read-only conformance (ADR-0005/0006/0007): over
    // the real binary, no projected tool may carry a flag that mutates the
    // repo, the forge, or the filesystem — and `publish` (a filesystem write)
    // must not be projected at all.
    let dir = TempDir::new().unwrap();
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ]
    .join("\n");

    let assert = adroit(&dir)
        .arg("mcp")
        .write_stdin(input)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    let mut checked = 0;
    for line in stdout.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let Some(tools) = v["result"]["tools"].as_array() else {
            continue;
        };
        assert!(!tools.is_empty(), "{stdout}");
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert_ne!(
                name, "publish",
                "publish writes an output tree and must not be projected"
            );
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            for flag in [
                "forge",
                "yes",
                "dry_run",
                "out",
                "save",
                "force",
                "regenerate",
            ] {
                assert!(
                    !props.contains_key(flag),
                    "MCP tool `{name}` leaks escalating flag `{flag}`: {tool}"
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no tools/list response found: {stdout}");
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_allow_write_authors_and_transitions_over_the_protocol() {
    // ADR-0021 end-to-end: over the real binary with `--allow-write`, an
    // MCP-only client scaffolds a decision (`new`, editor force-suppressed),
    // transitions it (`set-status`), and the corpus reflects both — while the
    // write tools announce themselves destructive and the forbidden control
    // surface (forge / interview / force) never appears in a schema.
    let dir = TempDir::new().unwrap();
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"new","arguments":{"title":"Adopt event sourcing"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set-status","arguments":{"id":"1","status":"accepted"}}}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list","arguments":{"status":"accepted"}}}"#,
    ]
    .join("\n");

    let assert = adroit(&dir)
        .args(["mcp", "--allow-write"])
        .write_stdin(input)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // The write slice is projected, annotated, and flag-clean.
    let mut saw_new = false;
    for line in stdout.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let Some(tools) = v["result"]["tools"].as_array() else {
            continue;
        };
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            for banned in ["forge", "yes", "out", "interview", "no_edit", "force"] {
                assert!(
                    !props.contains_key(banned),
                    "write-mode tool `{name}` leaks `{banned}`: {tool}"
                );
            }
            if name == "set-status" {
                assert!(
                    !props.contains_key("quorum"),
                    "set-status leaks its forge quorum parameter: {tool}"
                );
            }
            if name == "new" {
                saw_new = true;
                assert_eq!(tool["annotations"]["destructiveHint"], true, "{tool}");
            }
            assert_ne!(name, "draft", "draft is interactive — never projected");
        }
    }
    assert!(saw_new, "write mode projects `new`: {stdout}");

    // The calls really landed: the scaffolded decision is accepted, read back
    // over the same protocol (the list JSON rides inside MCP text content,
    // pretty-printed and escaped).
    assert!(stdout.contains("Adopt event sourcing"), "{stdout}");
    assert!(stdout.contains(r#"\"status\": \"accepted\""#), "{stdout}");

    // And on disk: frontmatter rewritten in place by the subprocess verbs.
    adroit(&dir)
        .args(["status", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("accepted"));
}

/// Regression (wave-3, found seeding the real portfolio corpus): a legacy
/// corpus carries (a) cross-ADR links written for the by_status layout and
/// (b) links to book pages outside the corpus. Seed heals (a) in the same
/// pass, and `check` treats (b) as an advisory external link — never an
/// error — because a seeded ephemeral space can't resolve them.
#[test]
fn seed_heals_by_status_links_and_check_tolerates_external_links() {
    let dir = TempDir::new().unwrap();
    let legacy = TempDir::new().unwrap();
    fs::create_dir_all(legacy.path().join("accepted")).unwrap();
    fs::create_dir_all(legacy.path().join("superseded")).unwrap();
    fs::write(
        legacy.path().join("superseded/0001-old-way.md"),
        "# ADR-0001: Old way\n\n## Status\n\nSuperseded by [ADR-0002](../accepted/0002-new-way.md)\n\n## Context\n\nSee [the book](../../kb-spec.md).\n",
    )
    .unwrap();
    fs::write(
        legacy.path().join("accepted/0002-new-way.md"),
        "# ADR-0002: New way\n\n## Status\n\nAccepted\n\nSupersedes [ADR-0001](../superseded/0001-old-way.md)\n\n## Context\n\nReplaces [ADR-0001](../superseded/0001-old-way.md) in prose too.\n",
    )
    .unwrap();

    adroit(&dir)
        .args(["seed", "--from"])
        .arg(legacy.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Healed"));

    // The by_status-era PROSE link now points at the flat sibling (the
    // status-region links folded into frontmatter refs, not body links).
    let two = fs::read_to_string(corpus(&dir).join("0002-new-way.md")).unwrap();
    assert!(
        two.contains("](./0001-old-way.md)") || two.contains("](0001-old-way.md)"),
        "{two}"
    );
    assert!(!two.contains("../superseded/"), "{two}");

    // check: exit 0 — the external book link is an advisory warning, not an
    // error; the healed ADR links are clean.
    adroit(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 warning(s)"));
    let out = adroit(&dir)
        .args(["check", "-o", "json"])
        .assert()
        .success();
    let json = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        json.contains("external link [../../kb-spec.md]"),
        "expected the external-link advisory in check -o json: {json}"
    );
}

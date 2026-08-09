//! CLI end-to-end tests — ported from `tests-integration/engine/`.
//!
//! Every test runs the real binary against a fresh fixture environment
//! (see `helpers::WikiEnv`), mirroring the Python `wiki_env` fixture.

use std::fs;

use super::helpers::{WikiEnv, commit_all, stderr, stdout};

// ── confidence (test_confidence.py) ───────────────────────────────────────────

#[test]
fn high_confidence_ranks_first() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["search", "mixture experts compute"]);
    let results = data["results"].as_array().unwrap();
    // Mirror the Python skip: the ranking claim needs at least two hits.
    if results.len() < 2 {
        return;
    }
    let first = results[0]["confidence"].as_f64().unwrap();
    let second = results[1]["confidence"].as_f64().unwrap_or(0.0);
    assert!(first >= second, "first: {first}, second: {second}");
}

// ── config (test_config.py) ───────────────────────────────────────────────────

#[test]
fn config_list_global() {
    let env = WikiEnv::new();
    env.run(&["admin", "config", "list"]);
}

#[test]
fn config_get_graph_format() {
    let env = WikiEnv::new();
    env.run(&["admin", "config", "get", "graph.format"]);
}

// ── content (test_content.py) ─────────────────────────────────────────────────

#[test]
fn content_read_by_slug() {
    let env = WikiEnv::new();
    let out = env.run(&["content", "read", "concepts/mixture-of-experts"]);
    assert!(stdout(&out).contains("Mixture of Experts"));
}

#[test]
fn content_read_cross_wiki_uri() {
    let env = WikiEnv::new();
    env.run(&[
        "content",
        "read",
        "wiki://notes/concepts/attention-mechanism",
    ]);
}

// ── export (test_export.py) ───────────────────────────────────────────────────

#[test]
fn export_llms_txt() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out_path = env.tmp().join("export-llms.txt");
    env.run(&[
        "export",
        "--path",
        out_path.to_str().unwrap(),
        "--wiki",
        "research",
    ]);
    let text = fs::read_to_string(&out_path).unwrap();
    assert!(text.contains("Mixture of Experts"));
}

#[test]
fn export_json() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out_path = env.tmp().join("export.json");
    env.run(&[
        "export",
        "--path",
        out_path.to_str().unwrap(),
        "--format",
        "json",
        "--wiki",
        "research",
    ]);
    let data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&out_path).unwrap()).unwrap();
    assert!(data.is_array());
}

// ── graph (test_graph.py) ─────────────────────────────────────────────────────

fn rebuild_both(env: &WikiEnv) {
    env.rebuild("research");
    env.rebuild("notes");
}

#[test]
fn graph_mermaid() {
    let env = WikiEnv::new();
    rebuild_both(&env);
    let out = env.run(&["graph"]);
    assert!(stdout(&out).to_lowercase().contains("graph"));
}

#[test]
fn graph_dot() {
    let env = WikiEnv::new();
    rebuild_both(&env);
    let out = env.run(&["graph", "--format", "dot"]);
    assert!(stdout(&out).contains("digraph"));
}

#[test]
fn graph_llms() {
    let env = WikiEnv::new();
    rebuild_both(&env);
    let out = env.run(&["graph", "--format", "llms"]);
    assert!(!stdout(&out).trim().is_empty());
}

#[test]
fn graph_type_filter() {
    let env = WikiEnv::new();
    rebuild_both(&env);
    env.run(&["graph", "--type", "concept"]);
}

#[test]
fn graph_root_depth() {
    let env = WikiEnv::new();
    rebuild_both(&env);
    env.run(&[
        "graph",
        "--root",
        "concepts/mixture-of-experts",
        "--depth",
        "2",
    ]);
}

#[test]
fn graph_cross_wiki() {
    let env = WikiEnv::new();
    rebuild_both(&env);
    let out = env.run(&["graph", "--cross-wiki"]);
    assert!(!stdout(&out).trim().is_empty());
}

// ── history (test_history.py) ─────────────────────────────────────────────────

#[test]
fn history_returns_commits() {
    let env = WikiEnv::new();
    let out = env.run(&["history", "concepts/mixture-of-experts"]);
    assert!(!stdout(&out).trim().is_empty());
}

#[test]
fn history_json_has_entries() {
    let env = WikiEnv::new();
    let data = env.json(&["history", "concepts/mixture-of-experts"]);
    assert!(!data["entries"].as_array().unwrap().is_empty());
}

// ── incremental (test_incremental.py) ─────────────────────────────────────────

#[test]
fn incremental_ingest_reports_result() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let modified = env.research_wiki.join("concepts/scaling-laws.md");
    let mut text = fs::read_to_string(&modified).unwrap();
    text.push('\n');
    fs::write(&modified, text).unwrap();
    let data = env.json(&["ingest", "concepts/scaling-laws.md"]);
    // file was modified (not in git yet), ingest reports it
    assert!(data.get("pages_validated").is_some() || data.get("unchanged_count").is_some());
}

// ── index (test_index.py) ─────────────────────────────────────────────────────

#[test]
fn index_rebuild_research() {
    let env = WikiEnv::new();
    let out = env.run(&["admin", "index", "rebuild", "--wiki", "research"]);
    assert!(stdout(&out).contains("Indexed"));
}

#[test]
fn index_status_research() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run(&["admin", "index", "status", "--wiki", "research"]);
    assert!(stdout(&out).contains("research"));
}

#[test]
fn index_rebuild_notes() {
    let env = WikiEnv::new();
    let out = env.run(&["admin", "index", "rebuild", "--wiki", "notes"]);
    assert!(stdout(&out).contains("Indexed"));
}

// ── ingest (test_ingest.py) ───────────────────────────────────────────────────

#[test]
fn ingest_dry_run_inbox() {
    let env = WikiEnv::new();
    let data = env.json(&["ingest", "inbox/", "--dry-run"]);
    assert!(data["pages_validated"].as_u64().unwrap() > 0);
}

#[test]
fn ingest_single_file_dry_run() {
    let env = WikiEnv::new();
    let data = env.json(&[
        "ingest",
        "inbox/01-paper-switch-transformer.md",
        "--dry-run",
    ]);
    assert_eq!(data["pages_validated"].as_u64().unwrap(), 1);
}

#[test]
fn ingest_real_file() {
    let env = WikiEnv::new();
    let src = env.inbox.join("01-paper-switch-transformer.md");
    let dst = env.inbox.join("test-ingest.md");
    fs::copy(&src, &dst).unwrap();
    let out = env.run(&["ingest", "inbox/test-ingest.md"]);
    assert!(stdout(&out).contains("Ingested"));
}

#[test]
fn ingest_redact_removes_secret() {
    let env = WikiEnv::new();
    let src = env.inbox.join("03-note-with-secrets.md");
    let dst = env.inbox.join("secrets-test.md");
    fs::copy(&src, &dst).unwrap();
    env.run(&["ingest", "inbox/secrets-test.md", "--redact"]);
    let content = fs::read_to_string(&dst).unwrap();
    assert!(!content.contains("sk-ant-api03"));
    assert!(content.contains("REDACTED"));
}

// ── lint (test_lint.py) ───────────────────────────────────────────────────────

fn lint_findings(env: &WikiEnv, args: &[&str]) -> Vec<serde_json::Value> {
    let mut full = vec!["lint"];
    full.extend(args);
    full.extend(["--format", "json"]);
    let out = env.run_unchecked(&full);
    let data: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    data["findings"].as_array().unwrap().clone()
}

#[test]
fn lint_all_rules() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run_unchecked(&["lint"]);
    let combined = format!("{}{}", stdout(&out), stderr(&out)).to_lowercase();
    assert!(combined.contains("error") || combined.contains("warning"));
}

#[test]
fn lint_broken_link_rule() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run_unchecked(&["lint", "--rules", "broken-link"]);
    assert!(stdout(&out).contains("broken-link"));
}

#[test]
fn lint_orphan_rule() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run_unchecked(&["lint", "--rules", "orphan"]);
    assert!(stdout(&out).contains("orphan"));
}

#[test]
fn lint_json_has_findings_array() {
    let env = WikiEnv::new();
    env.rebuild("research");
    lint_findings(&env, &[]); // panics if `findings` is not an array
}

#[test]
fn lint_broken_link_finds_dead_ref() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let findings = lint_findings(&env, &["--rules", "broken-link"]);
    let broken: Vec<_> = findings
        .iter()
        .filter(|f| f["rule"] == "broken-link")
        .collect();
    assert!(!broken.is_empty());
}

#[test]
fn lint_broken_link_detects_commonmark_inline() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let findings = lint_findings(&env, &["--rules", "broken-link"]);
    assert!(
        findings
            .iter()
            .filter(|f| f["rule"] == "broken-link")
            .any(|f| f["message"]
                .as_str()
                .unwrap()
                .contains("also-does-not-exist"))
    );
}

#[test]
fn lint_broken_link_ignores_valid_link() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let findings = lint_findings(&env, &["--rules", "broken-link"]);
    assert!(
        !findings
            .iter()
            .filter(|f| f["rule"] == "broken-link")
            .any(|f| f["message"]
                .as_str()
                .unwrap()
                .contains("mixture-of-experts"))
    );
}

#[test]
fn lint_orphan_finds_orphan_concept() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let findings = lint_findings(&env, &["--rules", "orphan"]);
    assert!(
        findings
            .iter()
            .any(|f| f["slug"] == "concepts/orphan-concept")
    );
}

#[test]
fn lint_structural_rules_run() {
    let env = WikiEnv::new();
    env.rebuild("research");
    for rule in ["articulation-point", "bridge", "periphery"] {
        let out = env.run_unchecked(&["lint", "--rules", rule]);
        let code = out.status.code().unwrap();
        assert!(code == 0 || code == 1, "rule {rule} exited with {code}");
    }
}

// ── list (test_list.py) ───────────────────────────────────────────────────────

#[test]
fn list_returns_pages() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run(&["list"]);
    assert!(stdout(&out).contains("concept"));
}

#[test]
fn list_json_has_pages() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["list"]);
    assert!(!data["pages"].as_array().unwrap().is_empty());
}

#[test]
fn list_type_filter() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run(&["list", "--type", "concept"]);
    assert!(stdout(&out).contains("concept"));
}

#[test]
fn list_json_type_filter() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["list", "--type", "concept"]);
    for page in data["pages"].as_array().unwrap() {
        assert_eq!(page["type"], "concept");
    }
}

#[test]
fn list_pagination() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run(&["list", "--page", "1", "--page-size", "2"]);
    assert!(stdout(&out).contains("Page 1"));
}

// ── logs (test_logs.py) ───────────────────────────────────────────────────────

fn seed_log(env: &WikiEnv) {
    let logs = env.config.parent().unwrap().join("logs");
    fs::create_dir_all(&logs).unwrap();
    fs::write(
        logs.join("2000-01-01.log"),
        "line1\nline2\nline3\nline4\nline5\n",
    )
    .unwrap();
}

#[test]
fn logs_list_returns_files() {
    let env = WikiEnv::new();
    seed_log(&env);
    let out = env.run(&["admin", "logs", "list"]);
    assert!(stdout(&out).contains("2000-01-01"));
}

#[test]
fn logs_tail_returns_output() {
    let env = WikiEnv::new();
    seed_log(&env);
    let out = env.run(&["admin", "logs", "tail"]);
    assert!(stdout(&out).contains("line"));
}

#[test]
fn logs_tail_n_lines() {
    let env = WikiEnv::new();
    seed_log(&env);
    let out = env.run(&["admin", "logs", "tail", "--lines", "3"]);
    let text = stdout(&out);
    let lines: Vec<&str> = text.trim().lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 3);
    assert!(text.contains("line3"));
}

#[test]
fn logs_clear() {
    let env = WikiEnv::new();
    seed_log(&env);
    let out = env.run(&["admin", "logs", "clear"]);
    assert!(stdout(&out).contains("removed 1"));
}

#[test]
fn logs_list_empty_after_clear() {
    let env = WikiEnv::new();
    seed_log(&env);
    env.run(&["admin", "logs", "clear"]);
    let out = env.run(&["admin", "logs", "list"]);
    assert!(stdout(&out).to_lowercase().contains("no log"));
}

// ── page id (test_page_id.py) ─────────────────────────────────────────────────
//
// Stable page identity — id declaration, resolution, and move survival.
// Pages are created at runtime (not in the shared fixtures) so the
// zero-change guarantee for id-free wikis stays covered by the other tests.

const ULID_B: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const ULID_C: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";
const ULID_MISSING: &str = "01CFGH0000000000000000ZZZZ";

fn write_page(env: &WikiEnv, rel_path: &str, title: &str, page_id: Option<&str>, body: &str) {
    let path = env.research_wiki.join(rel_path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let id_line = page_id.map(|id| format!("id: {id}\n")).unwrap_or_default();
    fs::write(
        &path,
        format!("---\ntitle: \"{title}\"\n{id_line}type: concept\nstatus: active\n---\n\n{body}"),
    )
    .unwrap();
}

fn commit_and_rebuild(env: &WikiEnv, message: &str) {
    commit_all(&env.research, message);
    env.rebuild("research");
}

/// broken-link findings for pages this suite creates (the shared fixture
/// wiki intentionally contains unrelated broken links).
fn broken_links(env: &WikiEnv) -> Vec<serde_json::Value> {
    lint_findings(env, &["--wiki", "research", "--rules", "broken-link"])
        .into_iter()
        .filter(|f| {
            let slug = f["slug"].as_str().unwrap();
            f["rule"] == "broken-link"
                && (slug.starts_with("decisions/") || slug.starts_with("guides/"))
        })
        .collect()
}

/// The acceptance test: move a page, its id links keep resolving.
#[test]
fn id_link_survives_move() {
    let env = WikiEnv::new();
    write_page(
        &env,
        "decisions/target.md",
        "Target",
        Some(ULID_B),
        "Body.\n",
    );
    write_page(
        &env,
        "decisions/linker.md",
        "Linker",
        None,
        &format!("See [[{ULID_B}]].\n"),
    );
    commit_and_rebuild(&env, "add id-linked pages");
    assert!(broken_links(&env).is_empty());

    // Move the target to a new directory and name
    fs::create_dir_all(env.research_wiki.join("guides")).unwrap();
    fs::rename(
        env.research_wiki.join("decisions/target.md"),
        env.research_wiki.join("guides/target-moved.md"),
    )
    .unwrap();
    commit_and_rebuild(&env, "move target");

    assert!(
        broken_links(&env).is_empty(),
        "id link must survive the move"
    );

    // And the id still reads the moved page
    let out = env.run(&["content", "read", ULID_B, "--wiki", "research"]);
    assert!(stdout(&out).contains("Target"));
}

/// Slug link to a moved page dangles; id link to a moved page does not.
#[test]
fn mixed_slug_and_id_links_after_move() {
    let env = WikiEnv::new();
    write_page(&env, "decisions/by-slug.md", "BySlug", None, "Body.\n");
    write_page(&env, "decisions/by-id.md", "ById", Some(ULID_C), "Body.\n");
    write_page(
        &env,
        "decisions/linker.md",
        "Linker",
        None,
        &format!("See [[decisions/by-slug]] and [[{ULID_C}]].\n"),
    );
    commit_and_rebuild(&env, "add mixed-link pages");
    assert!(broken_links(&env).is_empty());

    fs::rename(
        env.research_wiki.join("decisions/by-slug.md"),
        env.research_wiki.join("decisions/by-slug-moved.md"),
    )
    .unwrap();
    fs::rename(
        env.research_wiki.join("decisions/by-id.md"),
        env.research_wiki.join("decisions/by-id-moved.md"),
    )
    .unwrap();
    commit_and_rebuild(&env, "move both");

    let broken = broken_links(&env);
    assert_eq!(
        broken.len(),
        1,
        "exactly the slug link must dangle: {broken:?}"
    );
    assert!(
        broken[0]["message"]
            .as_str()
            .unwrap()
            .contains("decisions/by-slug")
    );
}

#[test]
fn duplicate_id_is_error() {
    let env = WikiEnv::new();
    write_page(&env, "decisions/x.md", "X", Some(ULID_B), "Body.\n");
    write_page(&env, "decisions/y.md", "Y", Some(ULID_B), "Body.\n");
    commit_and_rebuild(&env, "add duplicate ids");

    let findings = lint_findings(&env, &["--wiki", "research", "--rules", "duplicate-id"]);
    let dups: Vec<_> = findings
        .iter()
        .filter(|f| f["rule"] == "duplicate-id")
        .collect();
    assert_eq!(dups.len(), 2);
    assert!(dups.iter().all(|f| f["severity"] == "error"));
}

#[test]
fn id_format_warning() {
    let env = WikiEnv::new();
    write_page(
        &env,
        "decisions/bad.md",
        "Bad",
        Some("not-a-ulid"),
        "Body.\n",
    );
    commit_and_rebuild(&env, "add malformed id");

    let findings = lint_findings(&env, &["--wiki", "research", "--rules", "id-format"]);
    let matching: Vec<_> = findings
        .iter()
        .filter(|f| f["rule"] == "id-format")
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0]["severity"], "warning");
}

#[test]
fn unknown_id_is_broken_link() {
    let env = WikiEnv::new();
    write_page(
        &env,
        "decisions/dangling.md",
        "Dangling",
        None,
        &format!("See [[{ULID_MISSING}]].\n"),
    );
    commit_and_rebuild(&env, "add dangling id link");

    assert!(
        broken_links(&env)
            .iter()
            .any(|f| f["message"].as_str().unwrap().contains(ULID_MISSING))
    );
}

#[test]
fn content_new_with_id_flag() {
    let env = WikiEnv::new();
    let out = env.run(&[
        "content",
        "new",
        "decisions/fresh",
        "--id",
        "--wiki",
        "research",
    ]);
    let text = stdout(&out);
    assert!(text.contains("(id: "), "unexpected output: {text}");
    let ulid = text
        .split("(id: ")
        .nth(1)
        .unwrap()
        .trim()
        .trim_end_matches(')');
    assert_eq!(ulid.len(), 26);

    let content = fs::read_to_string(env.research_wiki.join("decisions/fresh.md")).unwrap();
    assert!(content.contains(&format!("id: {ulid}")));
}

#[test]
fn content_new_rejects_invalid_id() {
    let env = WikiEnv::new();
    let out = env.run_unchecked(&[
        "content",
        "new",
        "decisions/nope",
        "--id",
        "not-a-ulid",
        "--wiki",
        "research",
    ]);
    assert!(!out.status.success());
    assert!(format!("{}{}", stdout(&out), stderr(&out)).contains("ULID"));
}

#[test]
fn list_surfaces_id_only_when_declared() {
    let env = WikiEnv::new();
    write_page(
        &env,
        "decisions/tagged.md",
        "Tagged",
        Some(ULID_B),
        "Body.\n",
    );
    commit_and_rebuild(&env, "add tagged page");

    let data = env.json(&["list", "--wiki", "research"]);
    let pages = data["pages"].as_array().unwrap();
    let tagged = pages
        .iter()
        .find(|p| p["slug"] == "decisions/tagged")
        .unwrap();
    assert_eq!(tagged["id"], ULID_B);
    for page in pages {
        if page["slug"] != "decisions/tagged" {
            assert!(
                page.get("id").is_none(),
                "id must be omitted when absent: {}",
                page["slug"]
            );
        }
    }
}

// ── schema (test_schema.py) ───────────────────────────────────────────────────

const CUSTOM_SCHEMA: &str = r#"{
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "title": "test-custom",
    "type": "object",
    "x-wiki-types": {
        "test-custom": {"label": "Test Custom", "fields": []}
    }
}"#;

#[test]
fn schema_list() {
    let env = WikiEnv::new();
    let out = env.run(&["schema", "list"]);
    assert!(stdout(&out).contains("concept"));
}

#[test]
fn schema_show() {
    let env = WikiEnv::new();
    let out = env.run(&["schema", "show", "concept"]);
    assert!(stdout(&out).contains("title"));
}

#[test]
fn schema_validate() {
    let env = WikiEnv::new();
    env.run(&["schema", "validate"]);
}

#[test]
fn schema_add_and_remove() {
    let env = WikiEnv::new();
    let schema_file = env.tmp().join("test-custom.json");
    fs::write(&schema_file, CUSTOM_SCHEMA).unwrap();

    let out = env.run(&[
        "admin",
        "schema",
        "add",
        "test-custom",
        schema_file.to_str().unwrap(),
    ]);
    assert!(stdout(&out).contains("copied"));

    let out = env.run(&["schema", "list"]);
    assert!(stdout(&out).contains("test-custom"));

    let out = env.run(&["admin", "schema", "remove", "test-custom", "--delete"]);
    assert!(stdout(&out).contains("schema file deleted: true"));

    let out = env.run(&["schema", "list"]);
    assert!(!stdout(&out).contains("test-custom"));
}

// ── search (test_search.py) ───────────────────────────────────────────────────

#[test]
fn search_basic_returns_results() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run(&["search", "mixture of experts"]);
    assert!(stdout(&out).to_lowercase().contains("mixture"));
}

#[test]
fn search_type_filter() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run(&["search", "routing", "--type", "concept"]);
    let text = stdout(&out);
    assert!(text.to_lowercase().contains("concept") || !text.trim().is_empty());
}

#[test]
fn search_cross_wiki() {
    let env = WikiEnv::new();
    env.rebuild("research");
    env.rebuild("notes");
    env.run(&["search", "attention", "--cross-wiki"]);
}

#[test]
fn search_json_has_results() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["search", "transformer"]);
    assert!(!data["results"].as_array().unwrap().is_empty());
}

// ── wikis (test_spaces.py) ───────────────────────────────────────────────────

#[test]
fn admin_list_returns_both_wikis() {
    let env = WikiEnv::new();
    let out = env.run(&["admin", "list"]);
    let text = stdout(&out);
    assert!(text.contains("research"));
    assert!(text.contains("notes"));
}

#[test]
fn admin_list_shows_default_marker() {
    let env = WikiEnv::new();
    let out = env.run(&["admin", "list"]);
    assert!(stdout(&out).contains("* research"));
}

#[test]
fn admin_list_json_has_research_entry() {
    let env = WikiEnv::new();
    let data = env.json(&["admin", "list"]);
    let names: Vec<&str> = data
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"research"));
}

#[test]
fn admin_set_default() {
    let env = WikiEnv::new();
    env.run(&["admin", "set-default", "notes"]);
    let out = env.run(&["admin", "list"]);
    assert!(stdout(&out).contains("* notes"));
    env.run(&["admin", "set-default", "research"]);
    let out = env.run(&["admin", "list"]);
    assert!(stdout(&out).contains("* research"));
}

#[test]
fn admin_register_creates_entry() {
    let env = WikiEnv::new();
    let register_dir = env.tmp().join("wikis").join("register-test");
    fs::create_dir_all(register_dir.join("content")).unwrap();

    let out = env.run(&[
        "admin",
        "register",
        "--name",
        "register-test",
        "--content-root",
        "content",
        "--description",
        "integration test wiki",
        register_dir.to_str().unwrap(),
    ]);
    assert!(stdout(&out).contains("register-test"));
}

#[test]
fn admin_register_creates_wiki_toml() {
    let env = WikiEnv::new();
    let register_dir = env.tmp().join("wikis").join("register-test2");
    fs::create_dir_all(register_dir.join("pages")).unwrap();
    env.run(&[
        "admin",
        "register",
        "--name",
        "register-test2",
        "--content-root",
        "pages",
        register_dir.to_str().unwrap(),
    ]);
    let toml_text = fs::read_to_string(register_dir.join("wiki.toml")).unwrap();
    assert!(toml_text.contains("register-test2"));
    // A non-default content_root is recorded; the default ("content") is not.
    assert!(toml_text.contains("content_root"));
}

#[test]
fn admin_register_creates_dirs() {
    let env = WikiEnv::new();
    let register_dir = env.tmp().join("wikis").join("register-test3");
    fs::create_dir_all(register_dir.join("content")).unwrap();
    env.run(&[
        "admin",
        "register",
        "--name",
        "register-test3",
        "--content-root",
        "content",
        register_dir.to_str().unwrap(),
    ]);
    assert!(register_dir.join("inbox").is_dir());
    assert!(register_dir.join("schemas").is_dir());
}

#[test]
fn admin_remove_unregisters() {
    let env = WikiEnv::new();
    let register_dir = env.tmp().join("wikis").join("to-remove");
    fs::create_dir_all(register_dir.join("content")).unwrap();
    env.run(&[
        "admin",
        "register",
        "--name",
        "to-remove",
        "--content-root",
        "content",
        register_dir.to_str().unwrap(),
    ]);
    let out = env.run(&["admin", "remove", "to-remove", "--delete"]);
    assert!(stdout(&out).contains("Removed"));
    let out = env.run(&["admin", "list"]);
    assert!(!stdout(&out).contains("to-remove"));
}

// ── stats (test_stats.py) ─────────────────────────────────────────────────────

#[test]
fn stats_returns_output() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let out = env.run(&["stats"]);
    assert!(stdout(&out).contains("research"));
}

#[test]
fn stats_json_pages() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["stats"]);
    assert!(data["pages"].as_u64().unwrap() > 0);
}

#[test]
fn stats_json_fields() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["stats"]);
    assert!(data.get("communities").is_some());
    assert!(data.get("diameter").is_some());
    assert!(data.get("radius").is_some());
    assert!(data["center"].is_array());
}

// ── suggest (test_suggest.py) ─────────────────────────────────────────────────

#[test]
fn suggest_returns_results() {
    let env = WikiEnv::new();
    env.rebuild("research");
    env.run(&["suggest", "concepts/mixture-of-experts"]);
}

#[test]
fn suggest_json_is_array() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["suggest", "concepts/mixture-of-experts"]);
    assert!(data.is_array());
}

//! MCP server end-to-end tests — ported from `tests-integration/mcp/`.
//!
//! Spawns `llm-wiki serve` and speaks JSON-RPC 2.0 over stdio, exactly as an
//! MCP client would (the Python suite used the `mcp` client library for the
//! same wire exchange). Duplicated Python assertions over the same tool call
//! are folded into single tests; every distinct behavior is preserved.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

use super::helpers::{BIN, WikiEnv, commit_all};

// Stable slugs/names from the research wiki fixture
const SLUG_MOE: &str = "concepts/mixture-of-experts";
const SLUG_SCALING_LAWS: &str = "concepts/scaling-laws";
const SLUG_ORPHAN: &str = "concepts/orphan-concept";
const SLUG_MISSING: &str = "concepts/does-not-exist-xyz";
const SPACE_NAME: &str = "research";
const SPACE_NOTES: &str = "notes";

/// A minimal MCP client: one `serve` process, JSON-RPC 2.0 over stdio.
struct McpClient {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    /// Spawn the server and perform the initialize handshake.
    fn connect(env: &WikiEnv) -> Self {
        let mut child = Command::new(BIN)
            .arg("--config")
            .arg(&env.config)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut client = McpClient {
            child,
            stdin,
            reader,
            next_id: 0,
        };
        client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "integration-test", "version": "0.0.0"},
            }),
        );
        client.notify("notifications/initialized");
        client
    }

    fn send(&mut self, message: &Value) {
        let mut line = serde_json::to_string(message).unwrap();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({"jsonrpc": "2.0", "method": method}));
    }

    /// Send a request and block until its response arrives, skipping any
    /// server-initiated notifications in between.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let mut line = String::new();
            let n = self.reader.read_line(&mut line).unwrap();
            assert!(n > 0, "server closed stdout waiting for response id={id}");
            if line.trim().is_empty() {
                continue;
            }
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("invalid JSON from server: {e}\nline: {line}"));
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                assert!(
                    message.get("error").is_none(),
                    "request {method} failed: {message}"
                );
                return message["result"].clone();
            }
        }
    }

    /// Call a tool; returns `(is_error, text)` without asserting on errors.
    fn call_raw(&mut self, tool: &str, args: Value) -> (bool, String) {
        let result = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let is_error = result["isError"].as_bool().unwrap_or(false);
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        (is_error, text)
    }

    /// Call a tool; panics if the tool reports an error or returns no content.
    fn call(&mut self, tool: &str, args: Value) -> String {
        let (is_error, text) = self.call_raw(tool, args);
        assert!(!is_error, "call_tool({tool}) returned error: {text}");
        assert!(!text.is_empty(), "call_tool({tool}) returned empty content");
        text
    }

    /// Call a tool and parse its text content as JSON.
    fn call_json(&mut self, tool: &str, args: Value) -> Value {
        let text = self.call(tool, args);
        serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("call_tool({tool}) returned invalid JSON: {e}\n{text}"))
    }

    /// Rebuild the search index for a wiki via the MCP tool.
    fn rebuild(&mut self, wiki: &str) {
        self.call("wiki_admin_index_rebuild", json!({"wiki": wiki}));
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Fresh environment plus a connected MCP session.
fn mcp_env() -> (WikiEnv, McpClient) {
    let env = WikiEnv::new();
    let client = McpClient::connect(&env);
    (env, client)
}

// ── content (test_content.py) ─────────────────────────────────────────────────

#[test]
fn content_read_returns_page_body_with_frontmatter() {
    let (_env, mut mcp) = mcp_env();
    let text = mcp.call("wiki_content_read", json!({"uri": SLUG_MOE}));
    assert!(text.starts_with("---"));
    assert!(text.contains("type:"));
    assert!(text.contains("Mixture of Experts"));
}

#[test]
fn content_read_with_backlinks() {
    let (_env, mut mcp) = mcp_env();
    let text = mcp.call(
        "wiki_content_read",
        json!({"uri": SLUG_MOE, "backlinks": true}),
    );
    assert!(text.contains("---"));
    assert!(text.contains("backlinks"));
}

#[test]
fn content_read_backlinks_include_scaling_laws() {
    let (_env, mut mcp) = mcp_env();
    let text = mcp.call(
        "wiki_content_read",
        json!({"uri": SLUG_SCALING_LAWS, "backlinks": true}),
    );
    assert!(text.contains("mixture-of-experts"));
}

#[test]
fn content_read_via_wiki_uri() {
    let (_env, mut mcp) = mcp_env();
    let text = mcp.call(
        "wiki_content_read",
        json!({"uri": format!("wiki://{SPACE_NAME}/{SLUG_MOE}")}),
    );
    assert!(text.contains("Mixture of Experts"));
}

#[test]
fn resolve_existing_slug() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json("wiki_resolve", json!({"uri": SLUG_MOE}));
    assert_eq!(data["exists"], true);
    assert!(!data["slug"].as_str().unwrap().is_empty());
    assert!(data["path"].as_str().unwrap().ends_with(".md"));
}

#[test]
fn resolve_nonexistent_slug_returns_would_be_path() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json("wiki_resolve", json!({"uri": SLUG_MISSING}));
    assert_eq!(data["exists"], false);
    assert!(data["path"].as_str().unwrap().ends_with(".md"));
}

#[test]
fn content_write_and_read_back() {
    let (_env, mut mcp) = mcp_env();
    let new_data = mcp.call_json(
        "wiki_content_new",
        json!({"uri": "concepts/test-write-target", "wiki": SPACE_NAME}),
    );
    let slug = new_data["slug"].as_str().unwrap().to_string();

    let content = "---\ntitle: Write Test\ntype: page\nstatus: draft\n---\n\nHello write test.\n";
    mcp.call(
        "wiki_content_write",
        json!({"uri": slug, "content": content, "wiki": SPACE_NAME}),
    );

    let text = mcp.call(
        "wiki_content_read",
        json!({"uri": slug, "wiki": SPACE_NAME}),
    );
    assert!(text.contains("Hello write test."));
}

#[test]
fn content_new_creates_page() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json(
        "wiki_content_new",
        json!({"uri": "concepts/test-new-page", "wiki": SPACE_NAME}),
    );
    assert_eq!(data["slug"], "concepts/test-new-page");
    // The transport answers in slugs and uris, never filesystem paths
    // (taps #68) — existence is observable through the same door.
    assert!(data.get("path").is_none(), "{data}");
    let text = mcp.call(
        "wiki_content_read",
        json!({"uri": "concepts/test-new-page", "wiki": SPACE_NAME}),
    );
    assert!(
        text.contains("---"),
        "created page should read back: {text}"
    );
}

#[test]
fn transport_answers_carry_no_appliance_paths() {
    let (env, mut mcp) = mcp_env();
    let root = env.research.to_string_lossy().into_owned();

    let listing = mcp.call("wiki_admin_list", json!({}));
    assert!(
        !listing.contains(&root),
        "admin_list leaks the appliance path: {listing}"
    );

    let created = mcp.call(
        "wiki_content_new",
        json!({"uri": "concepts/leak-probe", "wiki": SPACE_NAME}),
    );
    assert!(!created.contains(&root), "content_new leaks: {created}");

    let written = mcp.call(
        "wiki_content_write",
        json!({
            "uri": "concepts/leak-probe",
            "content": "---\ntitle: Probe\ntype: page\nstatus: draft\n---\n\nBody.\n",
            "wiki": SPACE_NAME,
        }),
    );
    assert!(!written.contains(&root), "content_write leaks: {written}");

    let lint = mcp.call("wiki_lint", json!({"wiki": SPACE_NAME}));
    assert!(!lint.contains(&root), "lint leaks: {lint}");
}

#[test]
fn content_commit_after_write() {
    let (_env, mut mcp) = mcp_env();
    let new_data = mcp.call_json(
        "wiki_content_new",
        json!({"uri": "concepts/test-commit-target", "wiki": SPACE_NAME}),
    );
    let slug = new_data["slug"].as_str().unwrap().to_string();
    let content = "---\ntitle: Commit Test\ntype: page\nstatus: draft\n---\n\nCommit me.\n";
    mcp.call(
        "wiki_content_write",
        json!({"uri": slug, "content": content, "wiki": SPACE_NAME}),
    );

    let data = mcp.call_json(
        "wiki_content_commit",
        json!({"slugs": slug, "message": "test: commit test page", "wiki": SPACE_NAME}),
    );
    let hash = data["commit"].as_str().unwrap();
    assert!(hash.len() > 5, "commit hash should be a valid git SHA");
    assert!(
        data["indexed"].as_u64().unwrap() >= 1,
        "committed page should be indexed"
    );

    let resolved = mcp.call_json("wiki_resolve", json!({"uri": slug, "wiki": SPACE_NAME}));
    assert_eq!(
        resolved["exists"], true,
        "page {slug} should exist after commit"
    );
}

// ── export (test_export.py) ───────────────────────────────────────────────────

#[test]
fn export_llms_txt_pages_written() {
    let (env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let out = env.tmp().join("mcp-export-test.txt");
    let data = mcp.call_json(
        "wiki_export",
        json!({"path": out, "format": "llms-txt", "wiki": SPACE_NAME}),
    );
    assert!(data["pages_written"].as_u64().unwrap() > 0);
}

#[test]
fn export_llms_full_pages_written() {
    let (env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let out = env.tmp().join("mcp-export-full.txt");
    let data = mcp.call_json(
        "wiki_export",
        json!({"path": out, "format": "llms-full", "wiki": SPACE_NAME}),
    );
    assert!(data["pages_written"].as_u64().unwrap() > 0);
}

#[test]
fn export_json_report_fields() {
    let (env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let out = env.tmp().join("mcp-export.json");
    let data = mcp.call_json(
        "wiki_export",
        json!({"path": out, "format": "json", "wiki": SPACE_NAME}),
    );
    assert!(data["path"].is_string());
    assert!(data["bytes"].as_u64().unwrap() > 0);
}

// ── graph (test_graph.py) ─────────────────────────────────────────────────────

fn rebuild_both(mcp: &mut McpClient) {
    mcp.rebuild(SPACE_NAME);
    mcp.rebuild(SPACE_NOTES);
}

#[test]
fn graph_mermaid_output() {
    let (_env, mut mcp) = mcp_env();
    rebuild_both(&mut mcp);
    let text = mcp.call("wiki_graph", json!({})).to_lowercase();
    assert!(
        text.contains("graph lr") || text.contains("graph td") || text.contains("flowchart"),
        "not mermaid: {text}"
    );
}

#[test]
fn graph_dot_output() {
    let (_env, mut mcp) = mcp_env();
    rebuild_both(&mut mcp);
    let text = mcp.call("wiki_graph", json!({"format": "dot"}));
    assert!(text.contains("digraph"));
}

#[test]
fn graph_llms_output() {
    let (_env, mut mcp) = mcp_env();
    rebuild_both(&mut mcp);
    let text = mcp
        .call("wiki_graph", json!({"format": "llms"}))
        .to_lowercase();
    assert!(text.contains("nodes") || text.contains("edges") || text.contains("type groups"));
}

#[test]
fn graph_type_filter() {
    let (_env, mut mcp) = mcp_env();
    rebuild_both(&mut mcp);
    mcp.call("wiki_graph", json!({"type": "concept"}));
}

#[test]
fn graph_root_depth() {
    let (_env, mut mcp) = mcp_env();
    rebuild_both(&mut mcp);
    mcp.call("wiki_graph", json!({"root": SLUG_MOE, "depth": 2}));
}

#[test]
fn graph_cross_wiki() {
    let (_env, mut mcp) = mcp_env();
    rebuild_both(&mut mcp);
    mcp.call("wiki_graph", json!({"cross_wiki": true}));
}

// ── index (test_index.py) ─────────────────────────────────────────────────────

#[test]
fn index_rebuild_returns_pages_indexed() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json("wiki_admin_index_rebuild", json!({"wiki": SPACE_NAME}));
    assert!(data["pages_indexed"].as_u64().unwrap() > 0);
}

#[test]
fn index_status_after_rebuild() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_admin_index_status", json!({"wiki": SPACE_NAME}));
    assert!(data["built"].is_string());
    assert_eq!(data["queryable"], true);
}

// ── ingest (test_ingest.py) ───────────────────────────────────────────────────

#[test]
fn ingest_dry_run_report_fields() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json(
        "wiki_ingest",
        json!({"path": "inbox/01-paper-switch-transformer.md", "dry_run": true}),
    );
    assert!(data["pages_validated"].is_u64());
    assert!(data["warnings"].is_array());
    assert!(data["unchanged_count"].is_u64());
}

#[test]
fn ingest_redact_dry_run() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json(
        "wiki_ingest",
        json!({"path": "inbox/03-note-with-secrets.md", "dry_run": true, "redact": true}),
    );
    assert!(data["pages_validated"].is_u64());
}

// ── lint (test_lint.py) ───────────────────────────────────────────────────────

#[test]
fn lint_rule_filter_has_matching_findings() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    for rule in ["broken-link", "orphan"] {
        let data = mcp.call_json("wiki_lint", json!({"rules": rule}));
        let findings = data["findings"].as_array().unwrap();
        assert!(!findings.is_empty());
        let matching: Vec<_> = findings.iter().filter(|f| f["rule"] == rule).collect();
        assert!(!matching.is_empty(), "no findings with rule={rule}");
        for f in matching {
            assert!(f["slug"].is_string());
        }
    }
}

#[test]
fn lint_returns_findings() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let text = mcp.call("wiki_lint", json!({})).to_lowercase();
    assert!(text.contains("error") || text.contains("warning"));
}

#[test]
fn lint_json_findings_array() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_lint", json!({"format": "json"}));
    assert!(!data["findings"].as_array().unwrap().is_empty());
}

#[test]
fn lint_broken_link_detects_also_does_not_exist() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_lint", json!({"rules": "broken-link"}));
    assert!(
        data["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["rule"] == "broken-link")
            .any(|f| f["message"]
                .as_str()
                .unwrap()
                .contains("also-does-not-exist"))
    );
}

#[test]
fn lint_orphan_finds_orphan_concept() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_lint", json!({"rules": "orphan"}));
    assert!(
        data["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["slug"] == SLUG_ORPHAN)
    );
}

#[test]
fn lint_with_wiki_param() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let text = mcp
        .call("wiki_lint", json!({"wiki": SPACE_NAME}))
        .to_lowercase();
    assert!(text.contains("error") || text.contains("warning"));
}

#[test]
fn lint_findings_have_md_path() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_lint", json!({"rules": "broken-link"}));
    for f in data["findings"].as_array().unwrap() {
        assert!(
            f["path"].as_str().unwrap_or("").ends_with(".md"),
            "finding path not .md: {f}"
        );
    }
}

// ── negative (test_negative.py) ───────────────────────────────────────────────

#[test]
fn read_missing_page_returns_error() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let (is_error, text) = mcp.call_raw(
        "wiki_content_read",
        json!({"uri": SLUG_MISSING, "wiki": SPACE_NAME}),
    );
    assert!(is_error);
    assert!(!text.is_empty());
}

#[test]
fn search_empty_query_returns_valid_response() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let (is_error, text) = mcp.call_raw(
        "wiki_search",
        json!({"query": "", "wiki": SPACE_NAME, "format": "json"}),
    );
    if is_error {
        assert!(!text.is_empty());
    } else {
        let data: Value = serde_json::from_str(&text).unwrap();
        assert!(data["results"].is_array());
    }
}

#[test]
fn lint_missing_page_does_not_crash() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let (_is_error, text) = mcp.call_raw(
        "wiki_lint",
        json!({"uri": SLUG_MISSING, "wiki": SPACE_NAME}),
    );
    assert!(!text.is_empty());
}

#[test]
fn graph_invalid_format_returns_error() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let (is_error, text) = mcp.call_raw(
        "wiki_graph",
        json!({"format": "invalid-format-xyz", "wiki": SPACE_NAME}),
    );
    assert!(is_error);
    assert!(
        text.contains("invalid-format-xyz") || text.to_lowercase().contains("unknown"),
        "unexpected error text: {text}"
    );
}

#[test]
fn resolve_missing_page_does_not_crash() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json(
        "wiki_resolve",
        json!({"uri": SLUG_MISSING, "wiki": SPACE_NAME}),
    );
    assert_eq!(data["exists"], false);
    assert!(data["path"].is_string());
}

// ── page id (test_page_id.py) ─────────────────────────────────────────────────

const ULID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[test]
fn resolve_and_read_by_id() {
    let (env, mut mcp) = mcp_env();
    let page = env.research_wiki.join("concepts/id-page.md");
    std::fs::write(
        &page,
        format!("---\ntitle: \"Id Page\"\nid: {ULID}\ntype: concept\nstatus: active\n---\n\nId page body.\n"),
    )
    .unwrap();
    commit_all(&env.research, "add id page");
    mcp.rebuild(SPACE_NAME);

    let resolved = mcp.call_json("wiki_resolve", json!({"uri": ULID, "wiki": SPACE_NAME}));
    assert_eq!(resolved["slug"], "concepts/id-page");
    assert_eq!(resolved["exists"], true);
    assert_eq!(resolved["id"], ULID);

    let content = mcp.call(
        "wiki_content_read",
        json!({"uri": ULID, "wiki": SPACE_NAME}),
    );
    assert!(content.contains("Id page body"));
}

#[test]
fn resolve_without_id_omits_field() {
    let (_env, mut mcp) = mcp_env();
    let resolved = mcp.call_json("wiki_resolve", json!({"uri": SLUG_MOE, "wiki": SPACE_NAME}));
    assert_eq!(resolved["exists"], true);
    assert!(resolved.get("id").is_none());
}

#[test]
fn content_new_auto_id() {
    let (env, mut mcp) = mcp_env();
    let result = mcp.call_json(
        "wiki_content_new",
        json!({"uri": "concepts/auto-id-page", "wiki": SPACE_NAME, "auto_id": true}),
    );
    let id = result["id"].as_str().unwrap();
    assert_eq!(id.len(), 26);

    let content =
        std::fs::read_to_string(env.research_wiki.join("concepts/auto-id-page.md")).unwrap();
    assert!(content.contains(&format!("id: {id}")));
}

#[test]
fn content_new_rejects_invalid_id() {
    let (_env, mut mcp) = mcp_env();
    let (is_error, text) = mcp.call_raw(
        "wiki_content_new",
        json!({"uri": "concepts/bad-id-page", "wiki": SPACE_NAME, "id": "not-a-ulid"}),
    );
    assert!(is_error);
    assert!(text.contains("ULID"));
}

// ── schema + history (test_schema_history.py) ─────────────────────────────────

#[test]
fn schema_list_contains_concept() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json("wiki_schema", json!({"action": "list", "wiki": SPACE_NAME}));
    assert!(
        data.as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "concept")
    );
}

#[test]
fn schema_show_concept() {
    let (_env, mut mcp) = mcp_env();
    let text = mcp
        .call(
            "wiki_schema",
            json!({"action": "show", "type": "concept", "wiki": SPACE_NAME}),
        )
        .to_lowercase();
    assert!(text.contains("title") || text.contains("summary") || text.contains("confidence"));
}

#[test]
fn history_has_at_least_one_commit() {
    let (_env, mut mcp) = mcp_env();
    let data = mcp.call_json(
        "wiki_history",
        json!({"slug": SLUG_MOE, "wiki": SPACE_NAME, "format": "json"}),
    );
    assert!(!data["entries"].as_array().unwrap().is_empty());
}

// ── search + list (test_search.py) ────────────────────────────────────────────

#[test]
fn search_returns_scored_results() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json(
        "wiki_search",
        json!({"query": "mixture of experts", "format": "json"}),
    );
    let results = data["results"].as_array().unwrap();
    assert!(!results.is_empty());
    for hit in results {
        assert!(hit["slug"].is_string());
        assert!(hit["title"].is_string());
        assert!(hit["score"].is_number());
    }
    assert!(results[0]["score"].as_f64().unwrap() > 0.0);
    assert!(results.iter().any(|r| r["slug"] == SLUG_MOE));
}

#[test]
fn search_type_filter() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json(
        "wiki_search",
        json!({"query": "attention", "type": "concept", "format": "json"}),
    );
    let results = data["results"].as_array().unwrap();
    assert!(!results.is_empty());
    for hit in results {
        assert!(hit["slug"].as_str().unwrap().starts_with("concepts/"));
    }
}

#[test]
fn search_llms_format() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let text = mcp.call(
        "wiki_search",
        json!({"query": "transformer", "format": "llms"}),
    );
    assert!(text.contains("wiki://"));
}

#[test]
fn list_json_pages_match_total() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_list", json!({"format": "json"}));
    let total = data["total"].as_u64().unwrap();
    assert!(total > 0);
    let pages = data["pages"].as_array().unwrap();
    assert_eq!(pages.len() as u64, total);
    for page in pages {
        assert!(page["slug"].is_string());
        assert!(page["title"].is_string());
    }
}

#[test]
fn list_type_filter_returns_concept() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let text = mcp.call("wiki_list", json!({"type": "concept"}));
    assert!(text.contains("concept"));
}

#[test]
fn list_json_type_filter_all_concepts() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_list", json!({"type": "concept", "format": "json"}));
    let pages = data["pages"].as_array().unwrap();
    assert!(!pages.is_empty());
    for page in pages {
        assert_eq!(page["type"], "concept");
    }
}

// ── wikis (test_spaces.py) ───────────────────────────────────────────────────

#[test]
fn admin_list_returns_research() {
    let (_env, mut mcp) = mcp_env();
    let text = mcp.call("wiki_admin_list", json!({}));
    assert!(text.contains(SPACE_NAME));
    let data: Value = serde_json::from_str(&text).unwrap();
    assert!(
        data.as_array()
            .unwrap()
            .iter()
            .any(|w| w["name"] == SPACE_NAME)
    );
}

#[test]
fn admin_set_default_research() {
    let (_env, mut mcp) = mcp_env();
    let text = mcp.call("wiki_admin_set_default", json!({"name": SPACE_NAME}));
    assert!(text.contains(SPACE_NAME));
}

// ── stats (test_stats.py) ─────────────────────────────────────────────────────

#[test]
fn stats_returns_wiki_name() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let text = mcp.call("wiki_stats", json!({}));
    assert!(text.contains(SPACE_NAME));
}

#[test]
fn stats_json_counts() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_stats", json!({"format": "json"}));
    assert!(data["pages"].as_u64().unwrap() > 0);
    assert!(data["orphans"].is_u64());
}

#[test]
fn stats_graph_fields() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_stats", json!({"format": "json"}));
    assert!(data["communities"]["count"].is_u64());
    assert!(data["diameter"].is_null() || data["diameter"].is_number());
    for slug in data["center"].as_array().unwrap() {
        assert!(slug.is_string());
    }
}

// ── structural lint (test_structural.py) ──────────────────────────────────────

#[test]
fn lint_structural_rule_findings_have_slugs() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    for rule in ["articulation-point", "bridge", "periphery"] {
        let data = mcp.call_json("wiki_lint", json!({"rules": rule}));
        for f in data["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["rule"] == rule)
        {
            assert!(!f["slug"].as_str().unwrap().is_empty());
        }
    }
}

#[test]
fn lint_all_rules_includes_structural() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_lint", json!({}));
    let structural = ["articulation-point", "bridge", "periphery"];
    assert!(
        data["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| structural.contains(&f["rule"].as_str().unwrap_or(""))),
        "no structural rules in findings"
    );
}

// ── suggest (test_suggest.py) ─────────────────────────────────────────────────

#[test]
fn suggest_json_results_have_slugs() {
    let (_env, mut mcp) = mcp_env();
    mcp.rebuild(SPACE_NAME);
    let data = mcp.call_json("wiki_suggest", json!({"slug": SLUG_MOE, "format": "json"}));
    let results = data.as_array().unwrap();
    for entry in results {
        assert!(entry["slug"].is_string());
    }
}

// ── schema lifecycle stays live in-process (taps #108) ────────────────────────

const NOTE_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Field note",
  "type": "object",
  "required": ["title", "type"],
  "properties": {
    "title": { "type": "string" },
    "type": { "type": "string" },
    "status": { "type": "string" }
  },
  "x-wiki-types": { "field-note": "A field note" },
  "x-owner": "test-suite"
}"#;

#[test]
fn schema_remove_takes_effect_without_restart() {
    let (_env, mut mcp) = mcp_env();

    // Register a type; the live process must know it immediately…
    let reg = mcp.call_json(
        "wiki_admin_schema_register",
        json!({"type": "field-note", "schema": NOTE_SCHEMA, "wiki": SPACE_NAME}),
    );
    assert_eq!(reg["status"], "registered");
    let shown = mcp.call(
        "wiki_schema",
        json!({"action": "show", "type": "field-note", "wiki": SPACE_NAME}),
    );
    assert!(shown.contains("field-note"));

    // …and after removal it must be gone from the same process, no restart.
    let removed = mcp.call_json(
        "wiki_admin_schema_remove",
        json!({"type": "field-note", "delete": true, "wiki": SPACE_NAME}),
    );
    assert_eq!(removed["dry_run"], false);
    let (is_error, text) = mcp.call_raw(
        "wiki_schema",
        json!({"action": "show", "type": "field-note", "wiki": SPACE_NAME}),
    );
    assert!(
        is_error && text.contains("not registered"),
        "removed type still served by the live process: error={is_error} text={text}"
    );
}

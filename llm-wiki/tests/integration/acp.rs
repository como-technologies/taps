//! ACP server end-to-end tests — ported from `tests-integration/acp/`.
//!
//! Spawns `llm-wiki serve --acp --http :0` (ACP owns stdio; MCP is parked on
//! an ephemeral HTTP port) and speaks NDJSON JSON-RPC 2.0, exactly as the
//! Python suite did. Unlike the Python harness — which ran every exchange in
//! a fresh subprocess and so had to skip `session/load` and the session-cap
//! test — this client keeps one interactive connection per test, so both
//! previously-skipped behaviors are covered here.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::helpers::{BIN, WikiEnv};

/// A minimal ACP client: one `serve --acp` process, NDJSON JSON-RPC over stdio.
struct AcpClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<Value>,
    next_id: u64,
}

impl AcpClient {
    fn connect(env: &WikiEnv) -> Self {
        let mut child = Command::new(BIN)
            .arg("--config")
            .arg(&env.config)
            .args(["serve", "--acp", "--http", ":0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        // Reader thread: forward each NDJSON line as parsed JSON.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(&line)
                    .unwrap_or_else(|e| panic!("ACP line is not valid JSON: {e}\nline: {line}"));
                if tx.send(value).is_err() {
                    break;
                }
            }
        });

        AcpClient {
            child,
            stdin,
            rx,
            next_id: 0,
        }
    }

    /// Send a request; returns its id.
    fn send(&mut self, method: &str, params: Value) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let mut line = serde_json::to_string(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )
        .unwrap();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
        id
    }

    /// Collect messages until the response for `id` arrives. Returns
    /// `(notifications_seen, raw_response)` — the response may carry either
    /// `result` or `error`.
    fn recv_until(&mut self, id: u64) -> (Vec<Value>, Value) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| panic!("timed out waiting for response id={id}: {seen:?}"));
            match self.rx.recv_timeout(remaining) {
                Ok(message) => {
                    if message.get("id").and_then(Value::as_u64) == Some(id) {
                        return (seen, message);
                    }
                    seen.push(message);
                }
                Err(e) => panic!("ACP stream ended waiting for response id={id}: {e}\n{seen:?}"),
            }
        }
    }

    /// Send a request and return its `result`, panicking on JSON-RPC errors.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let (_, response) = self.recv_until(id);
        assert!(
            response.get("error").is_none(),
            "request {method} returned error: {response}"
        );
        response["result"].clone()
    }

    fn initialize(&mut self) -> Value {
        self.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientInfo": {"name": "acp-test", "version": "0.1.0"},
            }),
        )
    }

    /// `session/new` with optional `wiki` meta; returns the session id.
    fn new_session(&mut self, cwd: &std::path::Path, wiki: Option<&str>) -> String {
        let mut params = json!({"cwd": cwd, "mcpServers": []});
        if let Some(wiki) = wiki {
            params["_meta"] = json!({"wiki": wiki});
        }
        let result = self.request("session/new", params);
        result["sessionId"].as_str().unwrap().to_string()
    }

    /// `session/prompt` with a single text block; returns
    /// `(notifications, prompt_result)`.
    fn prompt(&mut self, session_id: &str, text: &str) -> (Vec<Value>, Value) {
        let id = self.send(
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": text}],
            }),
        );
        let (notifications, response) = self.recv_until(id);
        assert!(
            response.get("error").is_none(),
            "session/prompt returned error: {response}"
        );
        (notifications, response["result"].clone())
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Full round-trip: init + session/new (wiki=research) + session/prompt.
fn run_prompt(env: &WikiEnv, text: &str) -> (Vec<Value>, Value) {
    let mut acp = AcpClient::connect(env);
    acp.initialize();
    let sid = acp.new_session(env.tmp(), Some("research"));
    acp.prompt(&sid, text)
}

/// Concatenate streamed `agent_message_chunk` text from session/update
/// notifications.
fn collect_text(notifications: &[Value]) -> String {
    notifications
        .iter()
        .filter(|n| n["method"] == "session/update")
        .filter(|n| n["params"]["update"]["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|n| n["params"]["update"]["content"]["text"].as_str())
        .collect()
}

fn assert_end_turn(result: &Value) {
    assert_eq!(result["stopReason"], "end_turn", "result: {result}");
}

// ── lifecycle (test_lifecycle.py) ─────────────────────────────────────────────

#[test]
fn initialize_returns_agent_name() {
    let env = WikiEnv::new();
    let mut acp = AcpClient::connect(&env);
    let result = acp.initialize();
    assert_eq!(result["agentInfo"]["name"], "llm-wiki");
}

#[test]
fn session_new_returns_session_id() {
    let env = WikiEnv::new();
    let mut acp = AcpClient::connect(&env);
    acp.initialize();
    let sid = acp.new_session(env.tmp(), None);
    assert!(!sid.is_empty());
}

#[test]
fn session_new_with_wiki_meta_returns_session_id() {
    let env = WikiEnv::new();
    let mut acp = AcpClient::connect(&env);
    acp.initialize();
    let sid = acp.new_session(env.tmp(), Some("research"));
    assert!(!sid.is_empty());
}

// Skipped in Python (fresh subprocess per exchange); covered here because the
// client holds one connection.
#[test]
fn session_load_existing_succeeds() {
    let env = WikiEnv::new();
    let mut acp = AcpClient::connect(&env);
    acp.initialize();
    let sid = acp.new_session(env.tmp(), Some("research"));
    acp.request(
        "session/load",
        json!({"sessionId": sid, "cwd": env.tmp(), "mcpServers": []}),
    );
}

#[test]
fn session_load_unknown_returns_error() {
    let env = WikiEnv::new();
    let mut acp = AcpClient::connect(&env);
    acp.initialize();
    let id = acp.send(
        "session/load",
        json!({"sessionId": "session-does-not-exist", "cwd": env.tmp(), "mcpServers": []}),
    );
    let (_, response) = acp.recv_until(id);
    let message = response["error"]["message"].as_str().unwrap();
    assert!(message.to_lowercase().contains("not found"), "{message}");
}

#[test]
fn session_list_returns_at_least_one() {
    let env = WikiEnv::new();
    let mut acp = AcpClient::connect(&env);
    acp.initialize();
    acp.new_session(env.tmp(), None);
    let result = acp.request("session/list", json!({}));
    assert!(!result["sessions"].as_array().unwrap().is_empty());
}

// ── session cap (test_session_cap.py) ─────────────────────────────────────────

#[test]
fn acp_max_sessions_config_readable() {
    let env = WikiEnv::new();
    let out = env.run_unchecked(&["config", "get", "serve.acp_max_sessions"]);
    let code = out.status.code().unwrap();
    assert!(code == 0 || code == 1, "exited with {code}");
}

// Skipped in Python (needed a persistent server); covered here.
#[test]
fn session_cap_enforced() {
    let env = WikiEnv::new();
    env.run(&["config", "set", "serve.acp_max_sessions", "1", "--global"]);
    let mut acp = AcpClient::connect(&env);
    acp.initialize();
    acp.new_session(env.tmp(), None);
    let id = acp.send("session/new", json!({"cwd": env.tmp(), "mcpServers": []}));
    let (_, response) = acp.recv_until(id);
    let message = response["error"]["message"].as_str().unwrap();
    assert!(message.contains("Session limit reached"), "{message}");
}

// ── graph workflow (test_graph.py) ────────────────────────────────────────────

#[test]
fn graph_default_ends_turn() {
    let env = WikiEnv::new();
    let (_, result) = run_prompt(&env, "llm-wiki:graph");
    assert_end_turn(&result);
}

#[test]
fn graph_missing_slug_ends_turn() {
    let env = WikiEnv::new();
    let (_, result) = run_prompt(&env, "llm-wiki:graph zzz-missing-root-slug");
    assert_end_turn(&result);
}

// ── help workflow (test_help.py) ──────────────────────────────────────────────

#[test]
fn help_lists_workflows() {
    let env = WikiEnv::new();
    let (notifications, result) = run_prompt(&env, "llm-wiki:help");
    assert_end_turn(&result);
    assert!(collect_text(&notifications).contains("llm-wiki:research"));
}

#[test]
fn unknown_workflow_reports_available_workflows() {
    let env = WikiEnv::new();
    let (notifications, result) = run_prompt(&env, "llm-wiki:bogus-command");
    assert_end_turn(&result);
    let text = collect_text(&notifications);
    assert!(text.contains("Unknown workflow"), "text: {text}");
    assert!(text.contains("llm-wiki:research"), "text: {text}");
}

// ── ingest workflow (test_ingest.py) ──────────────────────────────────────────

#[test]
fn ingest_default_ends_turn() {
    let env = WikiEnv::new();
    let (_, result) = run_prompt(&env, "llm-wiki:ingest");
    assert_end_turn(&result);
}

#[test]
fn ingest_nonexistent_path_ends_turn() {
    let env = WikiEnv::new();
    let (_, result) = run_prompt(&env, "llm-wiki:ingest /nonexistent-path-xyz");
    assert_end_turn(&result);
}

// ── lint workflow (test_lint.py) ──────────────────────────────────────────────

#[test]
fn lint_all_rules_ends_turn() {
    let env = WikiEnv::new();
    let (_, result) = run_prompt(&env, "llm-wiki:lint");
    assert_end_turn(&result);
}

#[test]
fn lint_orphan_rule_streams_findings() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let (notifications, result) = run_prompt(&env, "llm-wiki:lint orphan");
    assert_end_turn(&result);
    let text = collect_text(&notifications);
    assert!(text.to_lowercase().contains("orphan"), "text: {text}");
}

#[test]
fn lint_comma_separated_rules_ends_turn() {
    let env = WikiEnv::new();
    let (_, result) = run_prompt(&env, "llm-wiki:lint stale,broken-link");
    assert_end_turn(&result);
}

// ── research workflow (test_research.py) ──────────────────────────────────────

#[test]
fn bare_prompt_triggers_research() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let (notifications, result) = run_prompt(&env, "what is mixture of experts?");
    assert_end_turn(&result);
    let text = collect_text(&notifications).to_lowercase();
    assert!(text.contains("searching for"), "text: {text}");
}

#[test]
fn research_explicit_prefix_ends_turn() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let (notifications, result) = run_prompt(&env, "llm-wiki:research scaling laws");
    assert_end_turn(&result);
    assert!(!collect_text(&notifications).is_empty());
}

#[test]
fn research_no_match_ends_turn() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let (_, result) = run_prompt(&env, "llm-wiki:research zzz-no-match-guaranteed-xyz");
    assert_end_turn(&result);
}

// ── use workflow (test_use.py) ────────────────────────────────────────────────

#[test]
fn use_existing_slug_ends_turn() {
    let env = WikiEnv::new();
    env.rebuild("research");
    let data = env.json(&["list", "--wiki", "research"]);
    let slug = data["pages"][0]["slug"].as_str().unwrap();

    let (_, result) = run_prompt(&env, &format!("llm-wiki:use {slug}"));
    assert_end_turn(&result);
}

#[test]
fn use_without_slug_reports_usage() {
    let env = WikiEnv::new();
    let (notifications, result) = run_prompt(&env, "llm-wiki:use");
    assert_end_turn(&result);
    assert!(collect_text(&notifications).contains("Usage"));
}

#[test]
fn use_missing_slug_ends_turn() {
    let env = WikiEnv::new();
    let (_, result) = run_prompt(&env, "llm-wiki:use zzz-missing-slug-xyz");
    assert_end_turn(&result);
}

use super::helpers::setup_wiki;
use llm_wiki::engine::WikiEngine;
use llm_wiki::ops;

// ── Graph ─────────────────────────────────────────────────────────────────────

#[test]
fn graph_build_returns_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::graph_build(
        &engine,
        "test",
        &ops::GraphParams {
            format: Some("mermaid"),
            root: None,
            depth: None,
            type_filter: None,
            relation: None,
            output: None,
            cross_wiki: false,
        },
    )
    .unwrap();
    assert!(result.report.nodes >= 2);
    assert!(result.rendered.contains("graph LR"));
}

#[test]
fn graph_build_dot_format() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::graph_build(
        &engine,
        "test",
        &ops::GraphParams {
            format: Some("dot"),
            root: None,
            depth: None,
            type_filter: None,
            relation: None,
            output: None,
            cross_wiki: false,
        },
    )
    .unwrap();
    assert!(result.rendered.contains("digraph wiki"));
}

// ── taps#98: unrooted graph must never serve a stale snapshot ─────────────────

fn unrooted<'a>() -> ops::GraphParams<'a> {
    ops::GraphParams {
        format: Some("llms"),
        root: None,
        depth: None,
        type_filter: None,
        relation: None,
        output: None,
        cross_wiki: false,
    }
}

#[test]
fn unrooted_graph_survives_process_restarts_and_new_content() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("state").join("config.toml");
    let wiki_path = dir.path().join("fresh");
    llm_wiki::registry::create(&wiki_path, "fresh", None, false, true, &config_path, None).unwrap();

    // Process 1: an empty wiki answers 0 nodes and persists that snapshot.
    {
        let manager = WikiEngine::build(&config_path).unwrap();
        let engine = manager.state.read().unwrap();
        let r = ops::graph_build(&engine, "fresh", &unrooted()).unwrap();
        assert_eq!(r.report.nodes, 0);
    }

    // Content lands and is committed between processes.
    let content_root = wiki_path.join("content");
    std::fs::create_dir_all(content_root.join("concepts")).unwrap();
    std::fs::write(
        content_root.join("concepts/one.md"),
        "---\ntitle: \"One\"\ntype: concept\nstatus: active\nread_when: [testing]\n---\n\nSee [[concepts/two]].\n",
    )
    .unwrap();
    std::fs::write(
        content_root.join("concepts/two.md"),
        "---\ntitle: \"Two\"\ntype: concept\nstatus: active\nread_when: [testing]\n---\n\nTwo.\n",
    )
    .unwrap();
    llm_wiki::git::commit(&wiki_path, "add pages").unwrap();

    // Process 2: a fresh mount must serve the populated graph — not the
    // 0-node snapshot process 1 left behind under a restarted counter key.
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();
    let full = ops::graph_build(&engine, "fresh", &unrooted()).unwrap();
    assert!(
        full.report.nodes >= 2,
        "stale snapshot served: {} nodes",
        full.report.nodes
    );

    // And the unrooted projection contains any rooted one.
    let rooted = ops::graph_build(
        &engine,
        "fresh",
        &ops::GraphParams {
            root: Some("concepts/one".to_string()),
            ..unrooted()
        },
    )
    .unwrap();
    assert!(
        full.report.nodes >= rooted.report.nodes && rooted.report.nodes >= 1,
        "unrooted ({}) must contain rooted ({})",
        full.report.nodes,
        rooted.report.nodes
    );
}

use super::helpers::setup_wiki;
use llm_wiki::engine::WikiEngine;
use llm_wiki::git;
use llm_wiki::ops;
use std::fs;

// ── Search ────────────────────────────────────────────────────────────────────

#[test]
fn search_returns_results() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let results = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "mixture",
            type_filter: None,
            status_filter: None,
            no_excerpt: false,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();
    assert!(!results.results.is_empty());
    assert_eq!(results.results[0].slug, "concepts/moe");
}

#[test]
fn search_type_filter() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let results = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "mixture",
            type_filter: Some("paper"),
            status_filter: None,
            no_excerpt: true,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();
    assert!(results.results.is_empty());
}

// ── List ──────────────────────────────────────────────────────────────────────

#[test]
fn list_returns_pages() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::list(&engine, "test", None, None, 1, None).unwrap();
    assert!(result.total >= 2);
}

#[test]
fn list_type_filter() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::list(&engine, "test", Some("concept"), None, 1, None).unwrap();
    assert!(result.total >= 2);

    let result = ops::list(&engine, "test", Some("paper"), None, 1, None).unwrap();
    assert_eq!(result.total, 0);
}

// ── Facets ────────────────────────────────────────────────────────────────────

#[test]
fn search_facets_type_distribution() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let wiki_path = dir.path().join("test");
    let content_root = wiki_path.join("content");

    // Add a paper page alongside the existing concepts
    fs::create_dir_all(content_root.join("sources")).unwrap();
    fs::write(
        content_root.join("sources/paper-a.md"),
        "---\ntitle: \"MoE Paper\"\ntype: paper\nstatus: active\ntags: [ml]\n---\n\nMixture of Experts paper.\n",
    )
    .unwrap();
    git::commit(&wiki_path, "add paper").unwrap();

    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "mixture",
            type_filter: None,
            status_filter: None,
            no_excerpt: true,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();

    // Type facet should show both concept and paper
    assert!(
        result.facets.r#type.contains_key("concept"),
        "type facet should contain concept, got: {:?}",
        result.facets.r#type
    );
    assert!(
        result.facets.r#type.contains_key("paper"),
        "type facet should contain paper, got: {:?}",
        result.facets.r#type
    );
}

#[test]
fn search_facets_type_unfiltered_when_type_filter_active() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let wiki_path = dir.path().join("test");
    let content_root = wiki_path.join("content");

    fs::create_dir_all(content_root.join("sources")).unwrap();
    fs::write(
        content_root.join("sources/paper-b.md"),
        "---\ntitle: \"Experts Paper\"\ntype: paper\nstatus: active\n---\n\nMixture of Experts scaling.\n",
    )
    .unwrap();
    git::commit(&wiki_path, "add paper").unwrap();

    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    // Search with type filter on concept
    let result = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "experts",
            type_filter: Some("concept"),
            status_filter: None,
            no_excerpt: true,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();

    // Type facet should still show paper (unfiltered)
    assert!(
        result.facets.r#type.contains_key("paper"),
        "type facet should be unfiltered and show paper, got: {:?}",
        result.facets.r#type
    );
}

#[test]
fn search_facets_empty_when_no_results() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "xyznonexistent",
            type_filter: None,
            status_filter: None,
            no_excerpt: true,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();

    assert!(result.results.is_empty());
    assert!(result.facets.r#type.is_empty());
    assert!(result.facets.status.is_empty());
    assert!(result.facets.tags.is_empty());
}

#[test]
fn list_facets_always_present() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::list(&engine, "test", None, None, 1, None).unwrap();

    // Should have type facet with at least "concept"
    assert!(
        result.facets.r#type.contains_key("concept"),
        "list facets should contain concept, got: {:?}",
        result.facets.r#type
    );
    assert!(
        !result.facets.status.is_empty(),
        "list facets should have status distribution"
    );
}

#[test]
fn search_and_list_surface_page_id() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let content_root = dir.path().join("test").join("content");
    fs::write(
        content_root.join("concepts/identified.md"),
        "---\ntitle: \"Identified Unicorn\"\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\ntype: concept\nstatus: active\n---\n\nBody.\n",
    )
    .unwrap();
    git::commit(&dir.path().join("test"), "add identified").unwrap();

    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let results = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "unicorn",
            type_filter: None,
            status_filter: None,
            no_excerpt: true,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();
    let hit = results
        .results
        .iter()
        .find(|r| r.slug == "concepts/identified")
        .expect("page should be found");
    assert_eq!(
        hit.id.map(|u| u.to_string()).as_deref(),
        Some("01ARZ3NDEKTSV4RRFFQ69G5FAV")
    );

    // Pages without an id serialize without the field
    let json = serde_json::to_value(&results.results).unwrap();
    for r in json.as_array().unwrap() {
        if r["slug"] == "concepts/moe" {
            assert!(
                r.get("id").is_none(),
                "id must be omitted from JSON when absent"
            );
        }
    }
}

// ── taps#107: results say what they are ───────────────────────────────────────

#[test]
fn search_results_carry_status_and_filter_by_it() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");

    // A retired page on the same subject as a live one.
    let wiki_path = dir.path().join("test");
    let content_root = wiki_path.join("content");
    std::fs::write(
        content_root.join("concepts/moe-old.md"),
        "---\ntitle: \"MoE (old)\"\ntype: concept\nstatus: archived\nread_when: [testing]\n---\n\nMixture of Experts, retired mechanics.\n",
    )
    .unwrap();
    llm_wiki::git::commit(&wiki_path, "add archived page").unwrap();

    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    // Unfiltered: both pages come back, each carrying its status.
    let all = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "mixture experts",
            type_filter: None,
            status_filter: None,
            no_excerpt: true,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();
    let old = all
        .results
        .iter()
        .find(|r| r.slug == "concepts/moe-old")
        .expect("archived page should appear unfiltered");
    assert_eq!(old.status, "archived");
    let live = all
        .results
        .iter()
        .find(|r| r.slug == "concepts/moe")
        .expect("active page should appear");
    assert_eq!(live.status, "active");

    // Filtered: only the active page remains.
    let active_only = ops::search(
        &engine,
        "test",
        &ops::SearchParams {
            query: "mixture experts",
            type_filter: None,
            status_filter: Some("active"),
            no_excerpt: true,
            top_k: None,
            include_sections: false,
            cross_wiki: false,
        },
    )
    .unwrap();
    assert!(
        active_only.results.iter().all(|r| r.status == "active"),
        "{:?}",
        active_only
            .results
            .iter()
            .map(|r| (&r.slug, &r.status))
            .collect::<Vec<_>>()
    );
    assert!(
        !active_only
            .results
            .iter()
            .any(|r| r.slug == "concepts/moe-old")
    );

    // The llms rendering marks non-active pages inline.
    let rendered = llm_wiki::search::render_search_llms(&all);
    assert!(rendered.contains("[archived]"), "{rendered}");
}

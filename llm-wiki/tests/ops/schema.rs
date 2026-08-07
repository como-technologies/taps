use super::helpers::setup_wiki;
use llm_wiki::engine::WikiEngine;
use llm_wiki::ops;

// ── Schema ────────────────────────────────────────────────────────────────────

#[test]
fn schema_list_returns_types() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let types = ops::schema_list(&engine, "test").unwrap();
    assert!(types.len() >= 15);
    assert!(types.iter().any(|t| t.name == "concept"));
    assert!(types.iter().any(|t| t.name == "skill"));
}

#[test]
fn schema_show_returns_json() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let content = ops::schema_show(&engine, "test", "concept").unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed.get("properties").is_some());
}

#[test]
fn schema_show_unknown_type_errors() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let result = ops::schema_show(&engine, "test", "nonexistent");
    assert!(result.is_err());
}

#[test]
fn schema_show_template_has_frontmatter() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let template = ops::schema_show_template(&engine, "test", "concept").unwrap();
    assert!(template.starts_with("---"));
    assert!(template.contains("type: concept"));
    assert!(template.contains("title:"));
}

#[test]
fn schema_validate_passes_default_schemas() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let issues = ops::schema_validate(&engine, "test", None).unwrap();
    assert!(
        issues.is_empty(),
        "default schemas should validate: {:?}",
        issues
    );
}

#[test]
fn schema_remove_removes_custom_type() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");

    // Write a minimal valid JSON Schema file for the custom type
    let schema_file = dir.path().join("custom-note.json");
    std::fs::write(
        &schema_file,
        r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"custom-note","type":"object","x-wiki-types":{"custom-note":{"label":"Custom Note","fields":[]}}}"#,
    )
    .unwrap();

    // Add the custom type (engine state reflects disk after add via wiki.toml)
    {
        let manager = WikiEngine::build(&config_path).unwrap();
        let engine = manager.state.read().unwrap();
        ops::schema_add(&engine, "test", "custom-note", &schema_file).unwrap();
    }

    // Rebuild engine so the type registry picks up the new type
    let manager = WikiEngine::build(&config_path).unwrap();

    // Verify it was added
    {
        let engine = manager.state.read().unwrap();
        let types = ops::schema_list(&engine, "test").unwrap();
        assert!(
            types.iter().any(|t| t.name == "custom-note"),
            "custom-note must be present after add"
        );
    }

    // Remove the custom type (delete schema file, no page deletion)
    let report = ops::schema_remove(&manager, "test", "custom-note", true, false, false).unwrap();
    assert!(report.schema_file_deleted, "schema file should be deleted");
    assert!(!report.dry_run);
}

#[test]
fn schema_remove_unknown_type_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();

    // Removing a type that was never registered returns Ok with zero counts
    let report =
        ops::schema_remove(&manager, "test", "nonexistent-type", false, false, false).unwrap();
    assert_eq!(report.pages_removed, 0);
    assert_eq!(report.pages_deleted_from_disk, 0);
    assert!(!report.wiki_toml_updated);
    assert!(!report.schema_file_deleted);
}

// ── schema register (taps#65: tools bring their own vocabulary) ───────────────

const ASSESSMENT_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Assessment type",
  "type": "object",
  "required": ["title", "type", "status"],
  "properties": {
    "title": { "type": "string" },
    "type": { "type": "string" },
    "status": { "type": "string" }
  },
  "x-wiki-types": { "assessment": "A published assessment definition" },
  "x-owner": "amaker"
}"#;

#[test]
fn schema_register_new_type_with_owner() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let report = ops::schema_register(
        &engine,
        "test",
        "assessment",
        ASSESSMENT_SCHEMA,
        Some("Body template.\n"),
    )
    .unwrap();
    assert_eq!(report.status, "registered");
    assert_eq!(report.owner.as_deref(), Some("amaker"));

    let space = engine.space("test").unwrap();
    assert!(space.repo_root.join("schemas/assessment.json").is_file());
    assert!(space.repo_root.join("schemas/assessment.md").is_file());
}

#[test]
fn schema_register_identical_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    ops::schema_register(&engine, "test", "assessment", ASSESSMENT_SCHEMA, None).unwrap();
    // Re-mount so the registry knows the new type, as a fresh process would.
    drop(engine);
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let report =
        ops::schema_register(&engine, "test", "assessment", ASSESSMENT_SCHEMA, None).unwrap();
    assert_eq!(report.status, "unchanged");
}

#[test]
fn schema_register_different_content_is_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    ops::schema_register(&engine, "test", "assessment", ASSESSMENT_SCHEMA, None).unwrap();
    drop(engine);
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let altered = ASSESSMENT_SCHEMA.replace("A published assessment definition", "Different");
    let err = ops::schema_register(&engine, "test", "assessment", &altered, None).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("different content"), "got: {msg}");
    assert!(msg.contains("amaker"), "conflict names the owners: {msg}");
}

#[test]
fn schema_register_conflicts_with_engine_class() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    // `concept` is engine-owned and stamped at create — a different schema
    // under that name must be refused, never overwritten.
    let hijack = ASSESSMENT_SCHEMA.replace("assessment", "concept");
    let err = ops::schema_register(&engine, "test", "concept", &hijack, None).unwrap_err();
    assert!(format!("{err}").contains("different content"));
}

#[test]
fn schema_register_rejects_undeclared_type() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let err =
        ops::schema_register(&engine, "test", "other-name", ASSESSMENT_SCHEMA, None).unwrap_err();
    assert!(format!("{err}").contains("x-wiki-types"));
}

#[test]
fn schema_register_rejects_unsafe_type_name() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    for bad in ["../evil", "UPPER", "has space", "", "-leading"] {
        let err = ops::schema_register(&engine, "test", bad, ASSESSMENT_SCHEMA, None).unwrap_err();
        assert!(
            format!("{err}").contains("invalid type name"),
            "'{bad}' should be rejected"
        );
    }
}

#[test]
fn schema_register_rejects_invalid_json_schema() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = setup_wiki(dir.path(), "test");
    let manager = WikiEngine::build(&config_path).unwrap();
    let engine = manager.state.read().unwrap();

    let err =
        ops::schema_register(&engine, "test", "assessment", "not json at all", None).unwrap_err();
    assert!(format!("{err}").contains("not valid JSON"));
}

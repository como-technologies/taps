use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::engine::{EngineState, WikiEngine};
use crate::git;
use crate::markdown;
use crate::search;
use crate::wiki_builder;

/// A registered page type with its schema location and description.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaTypeEntry {
    /// Type identifier (e.g. `"concept"`).
    pub name: String,
    /// Human-readable description of the type.
    pub description: String,
    /// Relative path to the JSON Schema file.
    pub schema_path: String,
}

/// List all registered types in a wiki's type registry.
pub fn schema_list(engine: &EngineState, wiki_name: &str) -> Result<Vec<SchemaTypeEntry>> {
    let wiki = engine.wiki(wiki_name)?;
    Ok(wiki
        .type_registry
        .list_types()
        .into_iter()
        .map(|(name, desc)| SchemaTypeEntry {
            name: name.to_string(),
            description: desc.to_string(),
            schema_path: wiki
                .type_registry
                .schema_path(name)
                .unwrap_or_default()
                .to_string(),
        })
        .collect())
}

/// Levenshtein distance, for near-miss suggestions on unknown type names.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// "type 'X' is not registered", with a did-you-mean when the registry
/// holds a near miss — the registry is right there; suggesting is the
/// standard kindness, for humans and agents alike.
fn unknown_type_error(
    registry: &crate::type_registry::WikiTypeRegistry,
    type_name: &str,
) -> anyhow::Error {
    let suggestion = registry
        .list_types()
        .into_iter()
        .map(|(name, _)| (levenshtein(type_name, name), name))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, name)| format!(" — did you mean '{name}'?"))
        .unwrap_or_default();
    anyhow::anyhow!("type '{type_name}' is not registered{suggestion}")
}

/// Return the raw JSON Schema content for a named type.
pub fn schema_show(engine: &EngineState, wiki_name: &str, type_name: &str) -> Result<String> {
    let wiki = engine.wiki(wiki_name)?;
    let schema_path = wiki
        .type_registry
        .schema_path(type_name)
        .ok_or_else(|| unknown_type_error(&wiki.type_registry, type_name))?;
    let full_path = wiki.repo_root.join(schema_path);

    if full_path.exists() {
        return std::fs::read_to_string(&full_path)
            .with_context(|| format!("failed to read schema: {}", full_path.display()));
    }

    let filename = full_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    crate::default_schemas::default_schemas()
        .get(filename)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "schema file not found on disk and no embedded default for '{type_name}': {}",
                full_path.display()
            )
        })
}

/// Return a frontmatter template for a type, derived from its JSON Schema.
pub fn schema_show_template(
    engine: &EngineState,
    wiki_name: &str,
    type_name: &str,
) -> Result<String> {
    let content = schema_show(engine, wiki_name, type_name)?;
    let schema: serde_json::Value = serde_json::from_str(&content)?;
    Ok(generate_template(&schema, type_name))
}

/// Copy a schema file into the wiki and register the type in `wiki.toml`.
pub fn schema_add(
    engine: &EngineState,
    wiki_name: &str,
    type_name: &str,
    src_path: &Path,
) -> Result<String> {
    let wiki = engine.wiki(wiki_name)?;

    // Validate the schema file
    let content = std::fs::read_to_string(src_path)
        .with_context(|| format!("failed to read: {}", src_path.display()))?;
    let schema_value: serde_json::Value =
        serde_json::from_str(&content).context("file is not valid JSON")?;
    jsonschema::Validator::new(&schema_value)
        .map_err(|e| anyhow::anyhow!("file is not a valid JSON Schema: {e}"))?;

    // Write to schemas/. The validated content is written from memory rather
    // than copied with fs::copy: when the source already IS the destination
    // (e.g. `schema add` pointed at a file inside schemas/), fs::copy would
    // truncate the file to 0 bytes before reading it.
    let filename = src_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid path"))?;
    let schemas_dir = wiki.repo_root.join("schemas");
    std::fs::create_dir_all(&schemas_dir)
        .with_context(|| format!("failed to create {}", schemas_dir.display()))?;
    let dest = schemas_dir.join(filename);
    std::fs::write(&dest, &content)
        .with_context(|| format!("failed to write {}", dest.display()))?;

    // Check if x-wiki-types declares the type
    let has_type = schema_value
        .get("x-wiki-types")
        .and_then(|v| v.as_object())
        .map(|obj| obj.contains_key(type_name))
        .unwrap_or(false);

    let mut msg = format!("copied to {}", dest.display());

    if !has_type {
        // Add wiki.toml override
        let mut wiki_cfg = config::load_wiki(&wiki.repo_root)?;
        wiki_cfg.types.insert(
            type_name.to_string(),
            config::TypeEntry {
                schema: format!("schemas/{}", filename.to_string_lossy()),
                description: format!("Custom type: {type_name}"),
            },
        );
        config::save_wiki(&wiki_cfg, &wiki.repo_root)?;
        msg.push_str(&format!(", added [types.{type_name}] to wiki.toml"));
    }

    msg.push_str(&rebuild_wiki_index(engine, wiki));

    Ok(msg)
}

/// Rebuild the type registry and search index after a schema change.
/// Registering a type changes the union index schema; the existing tantivy
/// index no longer matches it and any later rebuild would fail with "An index
/// exists but the schema does not match". Clear the stale index and rebuild
/// it with the new registry + schema so subsequent commands work. Returns a
/// human-readable outcome note.
fn rebuild_wiki_index(engine: &EngineState, wiki: &crate::engine::WikiContext) -> String {
    match wiki_builder::build_wiki(&wiki.repo_root, &engine.config.index.tokenizer) {
        Ok((new_registry, new_index_schema)) => {
            let index_path = wiki.index_manager.index_path().to_path_buf();
            let search_dir = index_path.join("search-index");
            if search_dir.exists()
                && let Err(e) = std::fs::remove_dir_all(&search_dir)
            {
                return format!(", failed to clear stale index: {e}");
            }
            // Fresh manager: the mounted one still holds a reader opened on
            // the old schema, which must not be reused for the new index.
            let manager = crate::index_manager::WikiIndexManager::new(&wiki.name, &index_path);
            match manager.rebuild(
                &wiki.content_root,
                &wiki.repo_root,
                &new_index_schema,
                &new_registry,
            ) {
                Ok(_) => ", search index rebuilt".to_string(),
                Err(e) => format!(
                    ", stale search index cleared (rebuild failed: {e}); it will be rebuilt on the next command"
                ),
            }
        }
        Err(e) => format!("\nWARNING: index resolution failed: {e}"),
    }
}

/// Report from `schema register`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaRegisterReport {
    /// The registered type name.
    pub type_name: String,
    /// `"registered"` (new) or `"unchanged"` (identical content already present).
    pub status: String,
    /// `x-owner` declared in the schema, if any — ownership as data.
    pub owner: Option<String>,
    /// Human-readable outcome notes (commit, index rebuild).
    pub notes: Vec<String>,
}

/// Register a type schema carried over the transport: validate it, admit it
/// idempotently, never overwrite. Identical content → `unchanged`; same type
/// with different content → a hard named conflict (schema evolution is an
/// explicit future surface, not a silent overwrite).
pub fn schema_register(
    engine: &EngineState,
    wiki_name: &str,
    type_name: &str,
    schema_content: &str,
    template: Option<&str>,
) -> Result<SchemaRegisterReport> {
    let wiki = engine.wiki(wiki_name)?;

    // The type name becomes a filename — keep it slug-shaped.
    if !type_name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || type_name.is_empty()
        || type_name.starts_with('-')
    {
        bail!("invalid type name '{type_name}': lowercase letters, digits, and '-' only");
    }

    let schema_value: serde_json::Value =
        serde_json::from_str(schema_content).context("schema is not valid JSON")?;
    jsonschema::Validator::new(&schema_value)
        .map_err(|e| anyhow::anyhow!("schema is not a valid JSON Schema: {e}"))?;

    // The schema must self-declare the type it defines.
    let declares = schema_value
        .get("x-wiki-types")
        .and_then(|v| v.as_object())
        .map(|obj| obj.contains_key(type_name))
        .unwrap_or(false);
    if !declares {
        bail!("schema does not declare '{type_name}' in x-wiki-types");
    }

    let owner = schema_value
        .get("x-owner")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // Idempotence / conflict: compare against the effective existing schema.
    if wiki.type_registry.is_known(type_name) {
        let existing = schema_show(engine, wiki_name, type_name)?;
        let existing_value: serde_json::Value = serde_json::from_str(&existing)
            .with_context(|| format!("existing schema for '{type_name}' is not valid JSON"))?;
        let template_matches = match template {
            None => true,
            Some(t) => {
                let tmpl_path = wiki
                    .repo_root
                    .join("schemas")
                    .join(format!("{type_name}.md"));
                std::fs::read_to_string(&tmpl_path)
                    .map(|c| c == t)
                    .unwrap_or(false)
            }
        };
        if existing_value == schema_value && template_matches {
            return Ok(SchemaRegisterReport {
                type_name: type_name.to_string(),
                status: "unchanged".to_string(),
                owner,
                notes: vec![],
            });
        }
        let existing_owner = existing_value
            .get("x-owner")
            .and_then(|v| v.as_str())
            .unwrap_or("unowned");
        bail!(
            "type '{type_name}' is already registered with different content \
             (existing owner: {existing_owner}, submitted owner: {}) — \
             refusing to overwrite; schema evolution is an explicit operation, not a re-register",
            owner.as_deref().unwrap_or("unowned")
        );
    }

    // New type: write schema (+ template), commit, rebuild.
    let schemas_dir = wiki.repo_root.join("schemas");
    std::fs::create_dir_all(&schemas_dir)
        .with_context(|| format!("failed to create {}", schemas_dir.display()))?;
    let schema_path = schemas_dir.join(format!("{type_name}.json"));
    std::fs::write(&schema_path, schema_content)
        .with_context(|| format!("failed to write {}", schema_path.display()))?;
    let mut committed_paths = vec![schema_path.clone()];
    if let Some(t) = template {
        let tmpl_path = schemas_dir.join(format!("{type_name}.md"));
        std::fs::write(&tmpl_path, t)
            .with_context(|| format!("failed to write {}", tmpl_path.display()))?;
        committed_paths.push(tmpl_path);
    }

    let mut notes = Vec::new();
    let resolved = wiki.resolved_config(&engine.config);
    if resolved.ingest.auto_commit {
        let path_refs: Vec<&Path> = committed_paths.iter().map(|p| p.as_path()).collect();
        let msg = format!(
            "schema register: {type_name} (owner: {})",
            owner.as_deref().unwrap_or("unowned")
        );
        match git::commit_paths(&wiki.repo_root, &path_refs, &msg) {
            Ok(hash) if !hash.is_empty() => notes.push(format!("committed {hash}")),
            Ok(_) => {}
            Err(e) => notes.push(format!("commit failed: {e}")),
        }
    }

    notes.push(
        rebuild_wiki_index(engine, wiki)
            .trim_start_matches(", ")
            .to_string(),
    );

    Ok(SchemaRegisterReport {
        type_name: type_name.to_string(),
        status: "registered".to_string(),
        owner,
        notes,
    })
}

/// Summary of a `schema remove` operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaRemoveReport {
    /// Number of indexed pages with this type that were removed from the index.
    pub pages_removed: usize,
    /// Number of page files actually deleted from disk.
    pub pages_deleted_from_disk: usize,
    /// True if the `[types.<name>]` entry was removed from `wiki.toml`.
    pub wiki_toml_updated: bool,
    /// True if the schema JSON file was deleted.
    pub schema_file_deleted: bool,
    /// True if this was a dry run (no changes made).
    pub dry_run: bool,
}

/// Remove a type schema and optionally delete its pages.
pub fn schema_remove(
    manager: &WikiEngine,
    wiki_name: &str,
    type_name: &str,
    delete: bool,
    delete_pages: bool,
    dry_run: bool,
) -> Result<SchemaRemoveReport> {
    if type_name == "default" {
        bail!("cannot remove the 'default' type");
    }

    let engine = manager
        .state
        .read()
        .map_err(|_| anyhow::anyhow!("lock poisoned"))?;
    let wiki = engine.wiki(wiki_name)?;

    // Count pages of this type in the index
    let searcher = wiki.index_manager.searcher()?;
    let list_result = search::list(
        &search::ListOptions {
            r#type: Some(type_name.to_string()),
            ..Default::default()
        },
        &searcher,
        wiki_name,
        &wiki.index_schema,
    )?;
    let pages_to_remove = list_result.total;

    if dry_run {
        return Ok(SchemaRemoveReport {
            pages_removed: pages_to_remove,
            pages_deleted_from_disk: if delete_pages { pages_to_remove } else { 0 },
            wiki_toml_updated: wiki
                .type_registry
                .list_types()
                .iter()
                .any(|(n, _)| *n == type_name),
            schema_file_deleted: delete,
            dry_run: true,
        });
    }

    // Remove pages from index
    if pages_to_remove > 0 {
        wiki.index_manager
            .delete_by_type(&wiki.index_schema, type_name)?;
    }

    // Delete page files from disk if requested
    let mut pages_deleted_from_disk = 0;
    if delete_pages && pages_to_remove > 0 {
        for page in &list_result.pages {
            if markdown::delete_page(&page.slug, &wiki.content_root)? {
                pages_deleted_from_disk += 1;
            }
        }
    }

    // Remove wiki.toml override if present
    let mut wiki_toml_updated = false;
    let mut wiki_cfg = config::load_wiki(&wiki.repo_root)?;
    if wiki_cfg.types.remove(type_name).is_some() {
        config::save_wiki(&wiki_cfg, &wiki.repo_root)?;
        wiki_toml_updated = true;
    }

    // Delete schema file if requested
    let mut schema_file_deleted = false;
    if delete && let Some(schema_path) = wiki.type_registry.schema_path(type_name) {
        let full_path = wiki.repo_root.join(schema_path);
        if full_path.exists() {
            // Check if other types use this schema
            let content = std::fs::read_to_string(&full_path).unwrap_or_default();
            if let Ok(schema) = serde_json::from_str::<serde_json::Value>(&content) {
                let wiki_types = schema
                    .get("x-wiki-types")
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.len())
                    .unwrap_or(0);
                if wiki_types <= 1 {
                    std::fs::remove_file(&full_path)?;
                    schema_file_deleted = true;
                }
                // If multiple types use this schema, don't delete
            }
        }
    }

    // Auto-commit if configured and changes were made
    let resolved = wiki.resolved_config(&engine.config);
    let repo_root = wiki.repo_root.clone();
    if resolved.ingest.auto_commit
        && (pages_deleted_from_disk > 0 || wiki_toml_updated || schema_file_deleted)
    {
        let msg = format!(
            "schema remove: {type_name} — {} pages, wiki.toml={wiki_toml_updated}, schema={schema_file_deleted}",
            pages_deleted_from_disk
        );
        let _ = git::commit(&repo_root, &msg);
    }

    Ok(SchemaRemoveReport {
        pages_removed: pages_to_remove,
        pages_deleted_from_disk,
        wiki_toml_updated,
        schema_file_deleted,
        dry_run: false,
    })
}

/// Validate schema files for one type or all types; returns a list of issue strings.
pub fn schema_validate(
    engine: &EngineState,
    wiki_name: &str,
    type_name: Option<&str>,
) -> Result<Vec<String>> {
    let wiki = engine.wiki(wiki_name)?;
    let mut issues = Vec::new();

    if let Some(name) = type_name {
        // Validate single type
        if !wiki.type_registry.is_known(name) {
            return Err(unknown_type_error(&wiki.type_registry, name));
        }
        let schema_path = wiki
            .type_registry
            .schema_path(name)
            .ok_or_else(|| anyhow::anyhow!("no schema path for type '{name}'"))?;
        let full_path = wiki.repo_root.join(schema_path);
        validate_schema_file(&full_path, &mut issues);
    } else {
        // Validate all schemas
        let schemas_dir = wiki.repo_root.join("schemas");
        if schemas_dir.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&schemas_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                validate_schema_file(&entry.path(), &mut issues);
            }
        }
    }

    // Index resolution check
    match wiki_builder::build_wiki(&wiki.repo_root, "en_stem") {
        Ok(_) => {}
        Err(e) => issues.push(format!("index resolution failed: {e}")),
    }

    Ok(issues)
}

fn validate_schema_file(path: &Path, issues: &mut Vec<String>) {
    let filename = path.file_name().unwrap_or_default().to_string_lossy();

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            issues.push(format!("{filename}: cannot read: {e}"));
            return;
        }
    };

    let schema: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            issues.push(format!("{filename}: invalid JSON: {e}"));
            return;
        }
    };

    if let Err(e) = jsonschema::Validator::new(&schema) {
        issues.push(format!("{filename}: invalid JSON Schema: {e}"));
        return;
    }

    if schema.get("x-wiki-types").is_none() {
        issues.push(format!(
            "{filename}: missing x-wiki-types (types won't be discovered)"
        ));
    }
}

fn generate_template(schema: &serde_json::Value, type_name: &str) -> String {
    let required: Vec<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let mut lines = vec!["---".to_string()];

    // Required fields first
    for field in &required {
        if let Some(prop) = properties.get(*field) {
            lines.push(format_template_field(field, prop, type_name));
        }
    }

    // Common optional fields. `confidence` earns its slot because absence
    // opts the page out of staleness tracking; `relates_to` because an
    // orphan birth is the default without it.
    for field in &[
        "summary",
        "status",
        "last_updated",
        "confidence",
        "relates_to",
        "tags",
    ] {
        if !required.contains(field)
            && let Some(prop) = properties.get(*field)
        {
            lines.push(format_template_field(field, prop, type_name));
        }
    }

    lines.push("---".to_string());
    lines.join("\n")
}

fn format_template_field(name: &str, prop: &serde_json::Value, type_name: &str) -> String {
    let prop_type = prop
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("string");

    match prop_type {
        "array" => {
            if name == "read_when" || name == "tags" {
                format!("{name}:\n  - \"\"")
            } else {
                format!("{name}: []")
            }
        }
        "string" => {
            if name == "type" {
                format!("type: {type_name}")
            } else if name == "status" {
                // Status vocabularies differ per class — read the schema's own
                // enum, preferring the authoring contract's born state.
                let from_enum = prop.get("enum").and_then(|e| e.as_array()).and_then(|arr| {
                    let vals: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                    if vals.contains(&"generated") {
                        Some("generated")
                    } else {
                        vals.first().copied()
                    }
                });
                match from_enum {
                    Some(v) => format!("status: {v}"),
                    None => "status: \"\"".to_string(),
                }
            } else if name == "last_updated" {
                format!(
                    "last_updated: \"{}\"",
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
                )
            } else {
                format!("{name}: \"\"")
            }
        }
        "boolean" => format!("{name}: false"),
        "number" => {
            if let Some(default) = prop.get("default") {
                format!("{name}: {default}")
            } else if name == "confidence" {
                // Born-generated pages declare low confidence; a template
                // without a value would opt the page out of staleness tracking.
                format!("{name}: 0.3")
            } else {
                format!("{name}: 0")
            }
        }
        _ => format!("{name}: \"\""),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_schemas_map_has_concept() {
        let schemas = crate::default_schemas::default_schemas();
        assert!(
            schemas.contains_key("concept.json"),
            "embedded concept.json missing"
        );
        let content = schemas["concept.json"];
        assert!(
            content.contains("\"concept\""),
            "concept.json lacks type identifier"
        );
    }
}

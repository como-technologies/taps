use std::sync::Arc;

use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{Map, Value, json};

use super::McpServer;
use super::handlers;
use super::helpers::{ToolResult, err_text};

// ── Schema helpers ────────────────────────────────────────────────────────────

fn schema(props: Value, required: &[&str]) -> Arc<Map<String, Value>> {
    let req: Vec<Value> = required
        .iter()
        .map(|s| Value::String(s.to_string()))
        .collect();
    let obj = json!({
        "type": "object",
        "properties": props,
        "required": req,
    });
    Arc::new(obj.as_object().unwrap().clone())
}

fn str_prop(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}

fn opt_str(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}

fn opt_bool(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn opt_int(desc: &str) -> Value {
    json!({"type": "integer", "description": desc})
}

/// Annotation for tools that only read — never touch the environment.
fn read_only() -> ToolAnnotations {
    ToolAnnotations::new().read_only(true)
}

/// Annotation for admin tools that can delete data.
fn destructive() -> ToolAnnotations {
    ToolAnnotations::new().destructive(true)
}

// ── Tool definitions (26 tools) ───────────────────────────────────────────────

/// Return the complete list of MCP tool definitions for registration.
pub fn tool_list() -> Vec<Tool> {
    vec![
        Tool::new(
            "wiki_admin_create",
            "Admin: initialize a new wiki repository and register it",
            schema(
                json!({
                    "path": str_prop("Path to create the wiki at"),
                    "name": str_prop("Wiki name — used in wiki:// URIs"),
                    "description": opt_str("Optional one-line description"),
                    "force": opt_bool("Update the registry entry if the name already exists"),
                    "set_default": opt_bool("Set as default wiki"),
                    "content_root": opt_str("Content directory relative to repo root (default: \"content\")"),
                }),
                &["path", "name"],
            ),
        ),
        Tool::new(
            "wiki_admin_register",
            "Admin: register an existing wiki repository without creating files",
            schema(
                json!({
                    "path": str_prop("Absolute path to the existing wiki repository"),
                    "name": str_prop("Wiki name — used in wiki:// URIs"),
                    "description": opt_str("Optional one-line description"),
                    "content_root": opt_str("Content directory (overrides wiki.toml; must already exist)"),
                }),
                &["path", "name"],
            ),
        ),
        Tool::new(
            "wiki_admin_list",
            "Admin: list all registered wikis",
            schema(
                json!({
                    "name": opt_str("Wiki name (omit for all)"),
                }),
                &[],
            ),
        )
        .annotate(read_only()),
        Tool::new(
            "wiki_admin_remove",
            "Admin: remove a wiki from the registry, optionally deleting it from disk",
            schema(
                json!({
                    "name": str_prop("Wiki name to remove"),
                    "delete": opt_bool("Also delete the wiki directory from disk"),
                }),
                &["name"],
            ),
        )
        .annotate(destructive()),
        Tool::new(
            "wiki_admin_set_default",
            "Admin: set the default wiki",
            schema(
                json!({
                    "name": str_prop("Wiki name to set as default"),
                }),
                &["name"],
            ),
        ),
        Tool::new(
            "wiki_admin_config",
            "Admin: get or set engine configuration values",
            schema(
                json!({
                    "action": str_prop("Action: get, set, or list"),
                    "key": opt_str("Config key (for get/set)"),
                    "value": opt_str("Config value (for set)"),
                    "global": opt_bool("Write to global config"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["action"],
            ),
        ),
        Tool::new(
            "wiki_admin_schema_register",
            "Admin: register a type schema (idempotent; never overwrites — identical content is a no-op, different content is a named conflict)",
            schema(
                json!({
                    "type": str_prop("Type name (must be declared in the schema's x-wiki-types; x-owner records the owning tool)"),
                    "schema": str_prop("JSON Schema content"),
                    "body_template": opt_str("Markdown body template content (optional)"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["type", "schema"],
            ),
        ),
        Tool::new(
            "wiki_admin_schema_evolve",
            "Admin: evolve a registered type's schema — the explicit upgrade register refuses (same x-owner on both sides; diff reported; index rebuilt; live process remounted)",
            schema(
                json!({
                    "type": str_prop("Type name (must already be registered)"),
                    "schema": str_prop("New JSON Schema content"),
                    "body_template": opt_str("Markdown body template content (optional)"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["type", "schema"],
            ),
        ),
        Tool::new(
            "wiki_admin_schema_remove",
            "Admin: unregister a type and remove its pages from the index",
            schema(
                json!({
                    "type": str_prop("Type name"),
                    "delete": opt_bool("Also delete the schema file"),
                    "delete_pages": opt_bool("Also delete page .md files from disk"),
                    "dry_run": opt_bool("Show what would be done without doing it"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["type"],
            ),
        )
        .annotate(destructive()),
        Tool::new(
            "wiki_content_read",
            "Read full content of a page by slug or URI",
            schema(
                json!({
                    "uri": str_prop("Slug or wiki:// URI"),
                    "no_frontmatter": opt_bool("Strip frontmatter from output"),
                    "list_assets": opt_bool("List co-located assets instead of content"),
                    "backlinks": opt_bool("Include incoming links — pages that link to this page"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["uri"],
            ),
        ),
        Tool::new(
            "wiki_content_write",
            "Write content to a page in the wiki tree",
            schema(
                json!({
                    "uri": str_prop("Slug or wiki:// URI"),
                    "content": str_prop("File content"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["uri", "content"],
            ),
        ),
        Tool::new(
            "wiki_content_new",
            "Create a page or section with scaffolded frontmatter",
            schema(
                json!({
                    "uri": str_prop("Slug or wiki:// URI"),
                    "section": opt_bool("Create a section instead of a page"),
                    "bundle": opt_bool("Create as bundle (folder + index.md)"),
                    "name": opt_str("Page title (default: derived from slug)"),
                    "type": opt_str("Page type (default: page)"),
                    "id": opt_str("Stable page id to assign (must be a ULID)"),
                    "auto_id": opt_bool("Generate a stable page id (ULID)"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["uri"],
            ),
        ),
        Tool::new(
            "wiki_content_commit",
            "Commit pending changes to git",
            schema(
                json!({
                    "slugs": opt_str("Comma-separated page slugs to commit (omit for all)"),
                    "message": opt_str("Commit message"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &[],
            ),
        ),
        Tool::new(
            "wiki_search",
            "Full-text BM25 search, returns ranked results",
            schema(
                json!({
                    "query": str_prop("Search query"),
                    "type": opt_str("Filter by frontmatter type"),
                    "status": opt_str("Filter by frontmatter status — results always carry status; non-current pages (superseded, deprecated, rejected) are history, never current guidance"),
                    "no_excerpt": opt_bool("Omit excerpts — refs only"),
                    "include_sections": opt_bool("Include section index pages"),
                    "top_k": opt_int("Max results"),
                    "wiki": opt_str("Target wiki name"),
                    "cross_wiki": opt_bool("Search across all wikis"),
                    "format": opt_str("Output format: json | llms (default: json)"),
                }),
                &["query"],
            ),
        ),
        Tool::new(
            "wiki_list",
            "Paginated page listing with filters",
            schema(
                json!({
                    "type": opt_str("Filter by frontmatter type"),
                    "status": opt_str("Filter by frontmatter status"),
                    "page": opt_int("Page number, 1-based"),
                    "page_size": opt_int("Results per page"),
                    "wiki": opt_str("Target wiki name"),
                    "format": opt_str("Output format: json | llms (default: json)"),
                }),
                &[],
            ),
        ),
        Tool::new(
            "wiki_ingest",
            "Validate, commit, and index files in the wiki tree",
            schema(
                json!({
                    "path": str_prop("File or folder path, relative to wiki root"),
                    "dry_run": opt_bool("Show what would be created without creating"),
                    "redact": opt_bool("Run redaction pass on file bodies before validation (opt-in; lossy — original values are replaced)"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["path"],
            ),
        ),
        Tool::new(
            "wiki_admin_index_rebuild",
            "Admin: rebuild the tantivy search index",
            schema(
                json!({
                    "wiki": opt_str("Target wiki name"),
                }),
                &[],
            ),
        ),
        Tool::new(
            "wiki_admin_index_status",
            "Admin: inspect index health",
            schema(
                json!({
                    "wiki": opt_str("Target wiki name"),
                }),
                &[],
            ),
        )
        .annotate(read_only()),
        Tool::new(
            "wiki_graph",
            "Generate concept graph, returns GraphReport",
            schema(
                json!({
                    "format": opt_str("Output format: mermaid | dot | llms (default: mermaid)"),
                    "root": opt_str("Subgraph from this node (slug)"),
                    "depth": opt_int("Hop limit from root"),
                    "type": opt_str("Comma-separated page types to include"),
                    "relation": opt_str("Filter edges by relation label"),
                    "output": opt_str("File path for output (default: stdout/return)"),
                    "cross_wiki": opt_bool("Merge all mounted wikis into a unified graph"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &[],
            ),
        ),
        Tool::new(
            "wiki_export",
            "Export the full wiki to a file (llms.txt, llms-full, or json)",
            schema(
                json!({
                    "wiki": str_prop("Target wiki name"),
                    "path": opt_str("Output path (relative to wiki root or absolute; default: llms.txt)"),
                    "format": opt_str("Export format: llms-txt | llms-full | json (default: llms-txt)"),
                    "status": opt_str("Page status filter: active | all (default: active, excludes archived)"),
                }),
                &["wiki"],
            ),
        ),
        Tool::new(
            "wiki_history",
            "Git commit history for a page",
            schema(
                json!({
                    "slug": str_prop("Slug or wiki:// URI"),
                    "limit": opt_int("Max entries to return"),
                    "follow": opt_bool("Track renames (default: from config)"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["slug"],
            ),
        ),
        Tool::new(
            "wiki_stats",
            "Wiki health dashboard — page counts, graph metrics, staleness, structural topology (diameter, radius, center)",
            schema(
                json!({
                    "wiki": opt_str("Target wiki name"),
                }),
                &[],
            ),
        ),
        Tool::new(
            "wiki_suggest",
            "Suggest related pages to link",
            schema(
                json!({
                    "slug": str_prop("Slug or wiki:// URI"),
                    "limit": opt_int("Max suggestions"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["slug"],
            ),
        ),
        Tool::new(
            "wiki_lint",
            "Run deterministic lint rules on the wiki index",
            schema(
                json!({
                    "rules": opt_str("Comma-separated rule names: orphan, broken-link, broken-cross-wiki-link, missing-fields, stale, unknown-type, duplicate-id, id-format, articulation-point, bridge, periphery (omit for all)"),
                    "severity": opt_str("Filter output: error | warning (omit for all)"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &[],
            ),
        ),
        Tool::new(
            "wiki_resolve",
            "Resolve a slug or wiki:// URI to its filesystem path on the server — a diagnostics aid for co-located tooling. Content writes go through wiki_content_write, never directly to disk.",
            schema(
                json!({
                    "uri": str_prop("Slug or wiki:// URI"),
                    "wiki": opt_str("Target wiki name (optional, uses default)"),
                }),
                &["uri"],
            ),
        ),
        Tool::new(
            "wiki_schema",
            "Inspect type schemas (read-only — vocabulary changes go through the wiki_admin_schema_* tools)",
            schema(
                json!({
                    "action": str_prop("Action: list, show, or validate"),
                    "type": opt_str("Type name (for show; optional for validate)"),
                    "template": opt_bool("Return frontmatter template instead of schema (for show)"),
                    "wiki": opt_str("Target wiki name"),
                }),
                &["action"],
            ),
        )
        .annotate(read_only()),
    ]
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

/// Dispatch a tool call by name to the appropriate handler, catching panics.
pub fn call(server: &McpServer, name: &str, args: &Map<String, Value>) -> ToolResult {
    let _span = tracing::info_span!("tool_call", tool = name).entered();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match name {
        "wiki_admin_create" => handlers::handle_admin_create(server, args),
        "wiki_admin_register" => handlers::handle_admin_register(server, args),
        "wiki_admin_list" => handlers::handle_admin_list(server, args),
        "wiki_admin_remove" => handlers::handle_admin_remove(server, args),
        "wiki_admin_set_default" => handlers::handle_admin_set_default(server, args),
        "wiki_admin_config" => handlers::handle_admin_config(server, args),
        "wiki_admin_schema_register" => handlers::handle_admin_schema_register(server, args),
        "wiki_admin_schema_evolve" => handlers::handle_admin_schema_evolve(server, args),
        "wiki_admin_schema_remove" => handlers::handle_admin_schema_remove(server, args),
        "wiki_content_read" => handlers::handle_content_read(server, args),
        "wiki_content_write" => handlers::handle_content_write(server, args),
        "wiki_content_new" => handlers::handle_content_new(server, args),
        "wiki_content_commit" => handlers::handle_content_commit(server, args),
        "wiki_search" => handlers::handle_search(server, args),
        "wiki_list" => handlers::handle_list(server, args),
        "wiki_ingest" => handlers::handle_ingest(server, args),
        "wiki_admin_index_rebuild" => handlers::handle_admin_index_rebuild(server, args),
        "wiki_admin_index_status" => handlers::handle_admin_index_status(server, args),
        "wiki_graph" => handlers::handle_graph(server, args),
        "wiki_history" => handlers::handle_history(server, args),
        "wiki_stats" => handlers::handle_stats(server, args),
        "wiki_lint" => handlers::handle_lint(server, args),
        "wiki_resolve" => handlers::handle_resolve(server, args),
        "wiki_suggest" => handlers::handle_suggest(server, args),
        "wiki_schema" => handlers::handle_schema(server, args),
        "wiki_export" => handlers::handle_export(server, args),
        _ => Err(format!("unknown tool: {name}")),
    }));
    match result {
        Ok(Ok((content, notify_uris))) => {
            let notify_resources_changed = matches!(
                name,
                "wiki_admin_create"
                    | "wiki_admin_register"
                    | "wiki_admin_remove"
                    | "wiki_admin_set_default"
            );
            tracing::debug!(tool = name, "tool call ok");
            ToolResult {
                content,
                is_error: false,
                notify_uris,
                notify_resources_changed,
            }
        }
        Ok(Err(msg)) => {
            tracing::warn!(tool = name, error = %msg, "tool call failed");
            ToolResult {
                content: err_text(msg),
                is_error: true,
                notify_uris: vec![],
                notify_resources_changed: false,
            }
        }
        Err(_) => {
            tracing::error!(tool = name, "tool handler panicked");
            ToolResult {
                content: err_text("internal error: tool panicked".into()),
                is_error: true,
                notify_uris: vec![],
                notify_resources_changed: false,
            }
        }
    }
}

use rmcp::model::Content;
use serde_json::{Map, Value};

use crate::ops;
use crate::slug::{ReadTarget, resolve_read_target};

use super::McpServer;
use super::helpers::*;

// ── Admin ──────────────────────────────────────────────────────────────────────

/// Handle `wiki_admin_create` — create a new wiki repository and register it.
pub fn handle_admin_create(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let path = arg_str_req(args, "path")?;
    let name = arg_str_req(args, "name")?;
    let description = arg_str(args, "description");
    let force = arg_bool(args, "force");
    let set_default = arg_bool(args, "set_default");
    let content_root = arg_str(args, "content_root");

    let config_path = {
        let engine = server.engine();
        engine.config_path.clone()
    };
    let report = ops::admin_create(
        &std::path::PathBuf::from(&path),
        &name,
        description.as_deref(),
        force,
        set_default,
        &config_path,
        Some(&server.manager),
        content_root.as_deref(),
    )
    .map_err(|e| format!("{e}"))?;

    let json = serde_json::to_string_pretty(&serde_json::json!({
        "name": report.name,
        "created": report.created,
        "registered": report.registered,
        "committed": report.committed,
    }))
    .map_err(|e| format!("{e}"))?;
    ok_text(json)
}

/// Handle `wiki_admin_register` — register an existing wiki repository without creating files.
pub fn handle_admin_register(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let path = arg_str_req(args, "path")?;
    let name = arg_str_req(args, "name")?;
    let description = arg_str(args, "description");
    let content_root = arg_str(args, "content_root");

    let config_path = {
        let engine = server.engine();
        engine.config_path.clone()
    };
    let report = ops::admin_register(
        &std::path::PathBuf::from(&path),
        &name,
        description.as_deref(),
        content_root.as_deref(),
        &config_path,
        Some(&server.manager),
    )
    .map_err(|e| format!("{e}"))?;

    let json = serde_json::to_string_pretty(&serde_json::json!({
        "name": report.name,
        "registered": report.registered,
    }))
    .map_err(|e| format!("{e}"))?;
    ok_text(json)
}

/// Handle `wiki_admin_list` — list registered wikis.
///
/// The transport surface is the only topology a client sees: wikis are
/// reached through the door, never as paths, so the listing carries names
/// and descriptions — deployment detail stays on the operator console.
pub fn handle_admin_list(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let engine = server.engine();
    let name = arg_str(args, "name");
    let entries = ops::admin_list(&engine.config, name.as_deref());
    let default = engine.default_wiki_name().to_string();
    let view: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let mut v = serde_json::json!({ "name": e.name });
            if let Some(d) = &e.description {
                v["description"] = serde_json::json!(d);
            }
            if e.name == default {
                v["default"] = serde_json::json!(true);
            }
            v
        })
        .collect();
    let s = serde_json::to_string_pretty(&view).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_admin_remove` — unregister (and optionally delete) a wiki.
pub fn handle_admin_remove(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let name = arg_str_req(args, "name")?;
    let delete = arg_bool(args, "delete");
    let config_path = {
        let engine = server.engine();
        engine.config_path.clone()
    };
    ops::admin_remove(&name, delete, &config_path, Some(&server.manager))
        .map_err(|e| format!("{e}"))?;
    ok_text(format!("Removed wiki \"{name}\""))
}

/// Handle `wiki_admin_set_default` — set the default wiki.
pub fn handle_admin_set_default(
    server: &McpServer,
    args: &Map<String, Value>,
) -> ToolHandlerResult {
    let name = arg_str_req(args, "name")?;
    let config_path = {
        let engine = server.engine();
        engine.config_path.clone()
    };
    ops::admin_set_default(&name, &config_path, Some(&server.manager))
        .map_err(|e| format!("{e}"))?;
    ok_text(format!("Default wiki set to \"{name}\""))
}

// ── Config ────────────────────────────────────────────────────────────────────

/// Handle `wiki_admin_config` — get, set, or list configuration values.
pub fn handle_admin_config(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let action = arg_str_req(args, "action")?;
    let engine = server.engine();
    let config_path = &engine.config_path;

    match action.as_str() {
        "list" => {
            let s = ops::config_list_global(config_path).map_err(|e| format!("{e}"))?;
            ok_text(s)
        }
        "get" => {
            let key = arg_str_req(args, "key")?;
            let val = ops::config_get(config_path, &key).map_err(|e| format!("{e}"))?;
            ok_text(format!("{key}: {val}"))
        }
        "set" => {
            let key = arg_str_req(args, "key")?;
            let value = arg_str_req(args, "value")?;
            let is_global = arg_bool(args, "global");
            let wiki_name = resolve_wiki_name(&engine, args)?;
            let msg = ops::config_set(config_path, &key, &value, is_global, Some(&wiki_name))
                .map_err(|e| format!("{e}"))?;
            ok_text(msg)
        }
        _ => Err(format!("unknown config action: {action}")),
    }
}

// ── Content ───────────────────────────────────────────────────────────────────

/// Handle `wiki_content_read` — read a page or list its co-located assets.
pub fn handle_content_read(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let uri = arg_str_req(args, "uri")?;
    let engine = server.engine();
    let wiki_flag = arg_str(args, "wiki");
    let no_frontmatter = arg_bool(args, "no_frontmatter");
    let list_assets = arg_bool(args, "list_assets");
    let include_backlinks = arg_bool(args, "backlinks");

    match ops::content_read(
        &engine,
        &uri,
        wiki_flag.as_deref(),
        no_frontmatter,
        list_assets,
    )
    .map_err(|e| format!("{e}"))?
    {
        ops::ContentReadResult::Page(content) => {
            if include_backlinks {
                let (entry, slug) = engine
                    .resolve_address(&uri, wiki_flag.as_deref())
                    .map_err(|e| format!("{e}"))?;
                let backlinks = ops::backlinks_for(&engine, &entry.name, slug.as_str())
                    .map_err(|e| format!("{e}"))?;
                let response = serde_json::json!({
                    "content": content,
                    "backlinks": backlinks,
                });
                let s = serde_json::to_string_pretty(&response).map_err(|e| format!("{e}"))?;
                ok_text(s)
            } else {
                ok_text(content)
            }
        }
        ops::ContentReadResult::Assets(assets) => ok_text(assets.join("\n")),
        ops::ContentReadResult::Binary => {
            Err("asset is binary — access it directly from the filesystem".into())
        }
    }
}

/// Handle `wiki_content_write` — write content to a wiki page by slug or URI.
pub fn handle_content_write(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let uri = arg_str_req(args, "uri")?;
    let content = arg_str_req(args, "content")?;
    let engine = server.engine();
    let wiki_flag = arg_str(args, "wiki");

    let result = ops::content_write(&engine, &uri, wiki_flag.as_deref(), &content)
        .map_err(|e| format!("{e}"))?;
    ok_text(format!("Wrote {} bytes to {uri}", result.bytes_written))
}

/// Handle `wiki_content_new` — create a new page or section with scaffolded frontmatter.
pub fn handle_content_new(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let uri = arg_str_req(args, "uri")?;
    let section = arg_bool(args, "section");
    let bundle = arg_bool(args, "bundle");
    let name = arg_str(args, "name");
    let type_ = arg_str(args, "type");
    let id = if arg_bool(args, "auto_id") {
        Some(ulid::Ulid::generate())
    } else if let Some(s) = arg_str(args, "id") {
        Some(ulid::Ulid::from_string(&s).map_err(|e| format!("invalid id (must be a ULID): {e}"))?)
    } else {
        None
    };

    let engine = server.engine();
    let wiki_flag = arg_str(args, "wiki");

    let result = ops::content_new(
        &engine,
        &uri,
        wiki_flag.as_deref(),
        section,
        bundle,
        name.as_deref(),
        type_.as_deref(),
        id,
    )
    .map_err(|e| format!("{e}"))?;
    let mut response = serde_json::json!({
        "uri":       result.uri,
        "slug":      result.slug,
        "bundle":    result.bundle,
    });
    if let Some(id) = result.id {
        response["id"] = serde_json::json!(id.to_string());
    }
    let s = serde_json::to_string_pretty(&response).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_resolve` — resolve a slug or URI to its filesystem path.
pub fn handle_resolve(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let uri = arg_str_req(args, "uri")?;
    let engine = server.engine();
    let wiki_flag = arg_str(args, "wiki");

    let (entry, slug) = engine
        .resolve_address(&uri, wiki_flag.as_deref())
        .map_err(|e| format!("{e}"))?;
    let content_root = engine
        .wiki(&entry.name)
        .map(|s| s.content_root.clone())
        .unwrap_or_else(|_| std::path::PathBuf::from(&entry.path).join("content"));

    let (path, exists, bundle) = match resolve_read_target(slug.as_str(), &content_root) {
        Ok(ReadTarget::Page(p)) => {
            let bundle = p.ends_with("index.md");
            (p, true, bundle)
        }
        _ => {
            let p = content_root.join(format!("{}.md", slug.as_str()));
            (p, false, false)
        }
    };

    let id = engine.wiki(&entry.name).ok().and_then(|wiki| {
        let searcher = wiki.index_manager.searcher().ok()?;
        crate::search::id_for_slug(&searcher, &wiki.index_schema, slug.as_str())
            .ok()
            .flatten()
    });

    let mut response = serde_json::json!({
        "slug":      slug.as_str(),
        "wiki":      entry.name,
        "content_root": content_root,
        "path":      path,
        "exists":    exists,
        "bundle":    bundle,
    });
    if let Some(id) = id {
        response["id"] = serde_json::json!(id.to_string());
    }
    let s = serde_json::to_string_pretty(&response).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_content_commit` — commit pending changes to git.
pub fn handle_content_commit(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;
    let message = arg_str(args, "message");

    let slugs: Vec<String> = arg_str(args, "slugs")
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let all = slugs.is_empty();

    let report = ops::content_commit(
        &engine,
        &server.manager,
        &wiki_name,
        &slugs,
        all,
        message.as_deref(),
    )
    .map_err(|e| format!("{e}"))?;
    let s = serde_json::to_string_pretty(&report).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

// ── Search ────────────────────────────────────────────────────────────────────

/// Handle `wiki_search` — BM25 full-text search across a wiki.
pub fn handle_search(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let query = arg_str_req(args, "query")?;
    let cross_wiki = arg_bool(args, "cross_wiki");
    let format = arg_str(args, "format");
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;

    let results = ops::search(
        &engine,
        &wiki_name,
        &ops::SearchParams {
            query: &query,
            type_filter: arg_str(args, "type").as_deref(),
            no_excerpt: format.as_deref() == Some("llms") || arg_bool(args, "no_excerpt"),
            top_k: arg_usize(args, "top_k"),
            include_sections: arg_bool(args, "include_sections"),
            cross_wiki,
        },
    )
    .map_err(|e| format!("{e}"))?;

    if format.as_deref() == Some("llms") {
        ok_text(crate::search::render_search_llms(&results))
    } else {
        let s = serde_json::to_string_pretty(&results).map_err(|e| format!("{e}"))?;
        ok_text(s)
    }
}

// ── List ──────────────────────────────────────────────────────────────────────

/// Handle `wiki_list` — paginated page listing with optional type/status filters.
pub fn handle_list(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;
    let format = arg_str(args, "format");

    let result = ops::list(
        &engine,
        &wiki_name,
        arg_str(args, "type").as_deref(),
        arg_str(args, "status").as_deref(),
        arg_usize(args, "page").unwrap_or(1),
        arg_usize(args, "page_size"),
    )
    .map_err(|e| format!("{e}"))?;

    if format.as_deref() == Some("llms") {
        ok_text(crate::search::render_list_llms(&result))
    } else {
        let s = serde_json::to_string_pretty(&result).map_err(|e| format!("{e}"))?;
        ok_text(s)
    }
}

// ── Ingest ────────────────────────────────────────────────────────────────────

/// Handle `wiki_ingest` — validate, redact, commit, and index files in the wiki tree.
pub fn handle_ingest(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let path = arg_str_req(args, "path")?;
    let dry_run = arg_bool(args, "dry_run");
    let redact = arg_bool(args, "redact");

    // Read path: ingest (ops handles WikiEngine mutation internally)
    let (report, wiki_name, notify_uris) = {
        let engine = server.engine();
        let wiki_name = resolve_wiki_name(&engine, args)?;

        let report =
            ops::ingest_with_redact(&engine, &server.manager, &path, dry_run, redact, &wiki_name)
                .map_err(|e| format!("{e}"))?;

        let notify_uris = if !dry_run {
            let wiki = engine.wiki(&wiki_name).map_err(|e| format!("{e}"))?;
            let ingest_path = wiki.content_root.join(&path);
            collect_page_uris(&ingest_path, &wiki.content_root, &wiki_name)
        } else {
            vec![]
        };

        (report, wiki_name, notify_uris)
    };

    let _ = wiki_name; // used above for notify_uris
    let s = serde_json::to_string_pretty(&report).map_err(|e| format!("{e}"))?;
    Ok((vec![Content::text(s)], notify_uris))
}

// ── Index ─────────────────────────────────────────────────────────────────────

/// Handle `wiki_admin_index_rebuild` — rebuild the tantivy search index from scratch.
pub fn handle_admin_index_rebuild(
    server: &McpServer,
    args: &Map<String, Value>,
) -> ToolHandlerResult {
    let wiki_name = {
        let engine = server.engine();
        resolve_wiki_name(&engine, args)?
    };

    let report = ops::index_rebuild(&server.manager, &wiki_name).map_err(|e| format!("{e}"))?;

    // Non-fatal: refresh the graph snapshot after index rebuild.
    {
        let engine = server.engine();
        if let Ok(wiki) = engine.wiki(&wiki_name) {
            let current_gen = wiki.index_manager.generation();
            if let Ok(searcher) = wiki.index_manager.searcher() {
                let _ = wiki.graph_cache.rebuild(current_gen, || {
                    crate::graph::build_graph(
                        &searcher,
                        &wiki.index_schema,
                        &crate::graph::GraphFilter::default(),
                        &wiki.type_registry,
                    )
                });
            }
        }
    }

    let s = serde_json::to_string_pretty(&report).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_admin_index_status` — report health and staleness of the search index.
pub fn handle_admin_index_status(
    server: &McpServer,
    args: &Map<String, Value>,
) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;

    let status = ops::index_status(&engine, &wiki_name).map_err(|e| format!("{e}"))?;
    let s = serde_json::to_string_pretty(&status).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

// ── Graph ─────────────────────────────────────────────────────────────────────

/// Handle `wiki_graph` — build and render the concept graph.
pub fn handle_graph(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;

    let result = ops::graph_build(
        &engine,
        &wiki_name,
        &ops::GraphParams {
            format: arg_str(args, "format").as_deref(),
            root: arg_str(args, "root"),
            depth: arg_usize(args, "depth"),
            type_filter: arg_str(args, "type").as_deref(),
            relation: arg_str(args, "relation"),
            output: arg_str(args, "output").as_deref(),
            cross_wiki: arg_bool(args, "cross_wiki"),
        },
    )
    .map_err(|e| format!("{e}"))?;

    ok_text(result.rendered)
}

// ── History ───────────────────────────────────────────────────────────────────

/// Handle `wiki_history` — return git commit history for a page slug.
pub fn handle_history(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let slug = arg_str_req(args, "slug")?;
    let limit = arg_usize(args, "limit");
    let follow = args.get("follow").and_then(|v| v.as_bool());
    let wiki_flag = arg_str(args, "wiki");

    let engine = server.engine();
    let result = ops::history(&engine, &slug, wiki_flag.as_deref(), limit, follow)
        .map_err(|e| format!("{e}"))?;
    let s = serde_json::to_string_pretty(&result).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_stats` — return aggregate health and coverage stats for a wiki.
pub fn handle_stats(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;
    let result = ops::stats(&engine, &wiki_name).map_err(|e| format!("{e}"))?;
    let s = serde_json::to_string_pretty(&result).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_lint` — run deterministic lint rules and return findings.
pub fn handle_lint(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;
    let rules = arg_str(args, "rules");
    let severity = arg_str(args, "severity");
    let mut result = ops::run_lint(&engine, &wiki_name, rules.as_deref(), severity.as_deref())
        .map_err(|e| format!("{e}"))?;
    // Findings address pages by slug; paths cross the transport wiki-relative,
    // never as appliance-absolute filesystem detail.
    if let Ok(wiki) = engine.wiki(&wiki_name) {
        for f in &mut result.findings {
            if let Ok(rel) = std::path::Path::new(&f.path).strip_prefix(&wiki.repo_root) {
                f.path = rel.to_string_lossy().into_owned();
            }
        }
    }
    let s = serde_json::to_string_pretty(&result).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_suggest` — suggest related pages to link from a given slug.
pub fn handle_suggest(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let slug = arg_str_req(args, "slug")?;
    let limit = arg_usize(args, "limit");
    let wiki_flag = arg_str(args, "wiki");
    let engine = server.engine();
    let result =
        ops::suggest(&engine, &slug, wiki_flag.as_deref(), limit).map_err(|e| format!("{e}"))?;
    let s = serde_json::to_string_pretty(&result).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_schema` — read-only: list, show, or validate type schemas.
/// Vocabulary changes go through the `wiki_admin_schema_*` tools.
pub fn handle_schema(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let action = arg_str(args, "action").ok_or("action is required")?;
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;

    match action.as_str() {
        "list" => {
            let entries = ops::schema_list(&engine, &wiki_name).map_err(|e| format!("{e}"))?;
            let s = serde_json::to_string_pretty(&entries).map_err(|e| format!("{e}"))?;
            ok_text(s)
        }
        "show" => {
            let type_name = arg_str(args, "type").ok_or("type is required for show")?;
            let template = args
                .get("template")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if template {
                let tmpl = ops::schema_show_template(&engine, &wiki_name, &type_name)
                    .map_err(|e| format!("{e}"))?;
                ok_text(tmpl)
            } else {
                let content = ops::schema_show(&engine, &wiki_name, &type_name)
                    .map_err(|e| format!("{e}"))?;
                ok_text(content)
            }
        }
        "validate" => {
            let type_name = arg_str(args, "type");
            let issues = ops::schema_validate(&engine, &wiki_name, type_name.as_deref())
                .map_err(|e| format!("{e}"))?;
            if issues.is_empty() {
                ok_text("ok".to_string())
            } else {
                ok_text(issues.join("\n"))
            }
        }
        _ => Err(format!("unknown action: {action}")),
    }
}

/// Handle `wiki_admin_schema_register` — register a type schema idempotently.
pub fn handle_admin_schema_register(
    server: &McpServer,
    args: &Map<String, Value>,
) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;
    let type_name = arg_str(args, "type").ok_or("type is required")?;
    let schema_content = arg_str(args, "schema").ok_or("schema is required")?;
    let body_template = arg_str(args, "body_template");
    let report = ops::schema_register(
        &engine,
        &wiki_name,
        &type_name,
        &schema_content,
        body_template.as_deref(),
    )
    .map_err(|e| format!("{e}"))?;
    // A new type changes the wiki's type registry, and the mounted
    // context is immutable — remount so this live process validates
    // and indexes the new type from here on (a one-shot CLI process
    // never needs this; a long-running serve always does).
    let entry = engine
        .config
        .wikis
        .iter()
        .find(|w| w.name == wiki_name)
        .cloned();
    drop(engine);
    if report.status == "registered"
        && let Some(entry) = entry
    {
        server
            .manager
            .mount_wiki(&entry)
            .map_err(|e| format!("remount after register failed: {e}"))?;
    }
    let s = serde_json::to_string_pretty(&report).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

/// Handle `wiki_admin_schema_remove` — unregister a type and remove its pages
/// from the index.
pub fn handle_admin_schema_remove(
    server: &McpServer,
    args: &Map<String, Value>,
) -> ToolHandlerResult {
    let engine = server.engine();
    let wiki_name = resolve_wiki_name(&engine, args)?;
    let type_name = arg_str(args, "type").ok_or("type is required")?;
    let delete = args
        .get("delete")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let delete_pages = args
        .get("delete_pages")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let dry_run = args
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    drop(engine);
    let report = ops::schema_remove(
        &server.manager,
        &wiki_name,
        &type_name,
        delete,
        delete_pages,
        dry_run,
    )
    .map_err(|e| format!("{e}"))?;
    let s = serde_json::to_string_pretty(&report).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

// ── Export ────────────────────────────────────────────────────────────────────

/// Handle `wiki_export` — export the full wiki to llms.txt, llms-full, or JSON.
pub fn handle_export(server: &McpServer, args: &Map<String, Value>) -> ToolHandlerResult {
    let wiki = arg_str_req(args, "wiki")?;
    let engine = server.engine();

    let format = ops::ExportFormat::parse(arg_str(args, "format").as_deref().unwrap_or("llms-txt"));
    let include_archived = arg_str(args, "status").as_deref() == Some("all");

    let report = ops::export(
        &engine,
        &ops::ExportOptions {
            wiki: wiki.clone(),
            path: arg_str(args, "path"),
            format,
            include_archived,
        },
    )
    .map_err(|e| format!("{e}"))?;

    let s = serde_json::to_string_pretty(&report).map_err(|e| format!("{e}"))?;
    ok_text(s)
}

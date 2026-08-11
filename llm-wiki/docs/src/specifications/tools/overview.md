---
title: "Tool Surface Overview"
summary: "The MCP/ACP/CLI tools — design principle, grouping, and global flags."
read_when:
  - Getting an overview of all available tools
  - Understanding why a tool belongs in the engine vs a skill
status: ready
last_updated: "2025-07-18"
---

# Tool Surface Overview

The engine exposes 25 tools. Every tool is available via MCP
(stdio + HTTP), ACP, and CLI. Same tool surface, three transports.

Tools split into **userland** (working *in* a wiki) and **admin**
(`wiki_admin_*` — operating the machinery). The rule that shapes the
split: no tool mixes risk classes behind an action argument, so a
harness can allowlist the userland surface and the read-only tools
outright while every admin write stays behind a prompt.

## Design Principle

A tool belongs in the engine if and only if it requires **stateful
access** that a skill cannot replicate:

- Filesystem writes into the wiki tree
- Git operations (commit, history)
- Tantivy index queries (search, list, graph traversal)
- Wiki registry mutations

Everything else — workflow orchestration, LLM prompting, multi-step
procedures — belongs in skills, external to the engine (e.g. the
Como authoring kit under `kit/`).

## The 25 Tools

### Admin — wiki registry (5 tools)

| Tool | Description |
|------|-------------|
| `wiki_admin_create` | Create a new wiki repo + register (hot-reloaded if server running) |
| `wiki_admin_register` | Register an existing repo without creating files |
| `wiki_admin_list` | List all registered wikis (read-only) |
| `wiki_admin_remove` | Remove a wiki from the registry (destructive; unmounted if server running) |
| `wiki_admin_set_default` | Set the default wiki (updated immediately if server running) |

References:
- [wiki-administration.md](wiki-administration.md)

### Admin — configuration (1 tool)

`wiki_admin_config` — get, set, or list configuration values (per-wiki
or global).

References:
- [config-management.md](config-management.md)

### Admin — vocabulary (2 tools)

| Tool | Description |
|------|-------------|
| `wiki_admin_schema_register` | Register a type schema idempotently (first-contact door for tools) |
| `wiki_admin_schema_remove` | Unregister a type and remove its pages (destructive) |

References:
- [schema-management.md](schema-management.md)

### Admin — index (2 tools)

| Tool | Description |
|------|-------------|
| `wiki_admin_index_rebuild` | Rebuild tantivy index from committed files |
| `wiki_admin_index_status` | Check index health (read-only) |

References:
- [index.md](index.md)

### Schema introspection (1 tool, read-only)

`wiki_schema` — list, show (± template), or validate type schemas.

References:
- [schema-management.md](schema-management.md)

### Content operations (4 tools)

| Tool | Description |
|------|-------------|
| `wiki_content_read` | Read full page content by slug or `wiki://` URI |
| `wiki_content_write` | Write a file into the wiki tree |
| `wiki_content_new` | Create a page or section with scaffolded frontmatter |
| `wiki_content_commit` | Commit pending changes to git |

References:
- [content-operations.md](content-operations.md)

### Search, graph & health (10 tools)

| Tool | Description |
|------|-------------|
| `wiki_search` | Full-text BM25 search with optional `--type` filter |
| `wiki_list` | Paginated page listing with type/status filters |
| `wiki_ingest` | Validate frontmatter + update index + commit |
| `wiki_graph` | Generate concept graph (Mermaid/DOT) |
| `wiki_history` | Git commit history for a page |
| `wiki_stats` | Wiki health dashboard |
| `wiki_suggest` | Suggest related pages to link |
| `wiki_lint` | Deterministic lint rules over the index |
| `wiki_resolve` | Resolve a slug/URI to its server-side path (diagnostics) |
| `wiki_export` | Export the full wiki (llms.txt / llms-full / JSON) |

References:
- [search.md](search.md)
- [list.md](list.md)
- [ingest.md](ingest.md)
- [graph.md](graph.md)
- [history.md](history.md)
- [stats.md](stats.md)
- [suggest.md](suggest.md)
- [lint.md](lint.md)
- [export.md](export.md)

## Global Flags

All CLI commands accept:

```
--wiki <name>    Target a specific wiki (default: global.default_wiki)
```

All MCP/ACP tools accept an optional `wiki` parameter with the same
semantics.

## CLI-Only Commands

These commands are available via CLI only (no MCP/ACP equivalent).

### Log management

| Command | Description |
|---------|-------------|
| `llm-wiki admin logs tail [--lines N]` | Show recent log entries (default: 50) |
| `llm-wiki admin logs list` | List log files |
| `llm-wiki admin logs clear` | Delete all log files |

### Filesystem watcher

| Command | Description |
|---------|-------------|
| `llm-wiki watch [--wiki <name>]` | Auto-ingest on file save (standalone mode) |

Operates on `$XDG_STATE_HOME/llm-wiki/logs/`. Only useful when `llm-wiki serve`
has file logging enabled (see [server.md](../engine/server.md)).

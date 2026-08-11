---
title: "Schema Management"
summary: "The schema surface, split by risk class — read-only llm-wiki schema / wiki_schema, and the write half under llm-wiki admin schema / wiki_admin_schema_*."
read_when:
  - Understanding how to inspect type schemas
  - Adding or removing a custom type
  - Getting a frontmatter template for a type
  - Validating schema files before ingest
status: ready
last_updated: "2025-07-18"
---

# Schema Management

Introspect and manage the type schemas for a wiki. The surface splits
by **risk class**, not audience — no tool mixes reads with destructive
writes behind an action argument, so a harness can allowlist the read
half outright:

- **Read-only**: `llm-wiki schema` (CLI) and `wiki_schema` (MCP,
  `readOnlyHint`) — `list`, `show` (± `--template`), `validate`.
- **Vocabulary writes**: `llm-wiki admin schema` (CLI) and the
  per-verb `wiki_admin_schema_register` / `wiki_admin_schema_remove`
  MCP tools (remove carries `destructiveHint`).

All operations target a specific wiki (`--wiki <name>` or the default).

```
llm-wiki schema list [--format text|json]                          List registered types
llm-wiki schema show <type> [--format text|json]                   Print JSON Schema
llm-wiki schema show <type> --template                             Print frontmatter template
llm-wiki schema validate [<type>]                                  Validate schemas + index resolution
llm-wiki admin schema register <type> <schema-path> [--template <md>]        Register a tool-owned type (idempotent)
llm-wiki admin schema add <type> <schema-path>                               Register a custom type
llm-wiki admin schema remove <type> [--delete] [--delete-pages] [--dry-run]  Unregister a type
```

## Operations

### list

List all registered types with descriptions.

**CLI:**
```
llm-wiki schema list [--wiki <name>] [--format text|json]
```

**MCP:**
```json
{
  "action": "list",
  "wiki": "<name>"
}
```

**Output (text):** sorted list of type entries:

```
default       Fallback for unrecognized types
article       Editorial source — blog posts, news, essays
concept       Synthesized knowledge — one concept per page
doc           Reference document — specifications, guides, standards
...
```

**Output (json):** array of `{ name, description, schema_path }`.

Sources: discovered from `schemas/*.json` via `x-wiki-types`, merged
with `wiki.toml` `[types.*]` overrides.

### show

Print the JSON Schema for a type.

**CLI:**
```
llm-wiki schema show <type> [--wiki <name>] [--format text|json]
```

**MCP:**
```json
{
  "action": "show",
  "type": "<type>",
  "wiki": "<name>"
}
```

**Output:** the full JSON Schema file content for the given type.

If the type is not registered, returns an error.

### show --template

Print a YAML frontmatter template for a type.

**CLI:**
```
llm-wiki schema show <type> --template [--wiki <name>]
```

**MCP:**
```json
{
  "action": "show",
  "type": "<type>",
  "template": true,
  "wiki": "<name>"
}
```

**Output:** a YAML frontmatter block with all required fields filled
with placeholder values and optional fields commented or omitted:

```yaml
---
title: ""
type: concept
read_when:
  - ""
summary: ""
status: generated
last_updated: "2026-08-10T00:00:00Z"
confidence: 0.5
tags:
  - ""
---
```

`status` comes from the class's own enum (preferring the authoring
contract's born state, `generated`), `last_updated` is RFC 3339 (the
format the schemas demand), and `confidence` appears whenever the class
declares it — an absent confidence would opt the page out of staleness
tracking. Classes that declare `relates_to` get it in the commons too.

For skill type (aliased fields):

```yaml
---
name: ""
description: ""
type: skill
status: generated
last_updated: "2026-08-10T00:00:00Z"
tags:
  - ""
---
```

Template generation reads `required` and `properties` from the JSON
Schema. Required fields are included with empty/default values.
Optional fields may be included as comments or omitted.

### register

Register a type schema idempotently — the door tools use on first
contact. Identical content is a no-op (`unchanged`); different content
under the same name is a named conflict, never an overwrite. The
schema must declare its types in `x-wiki-types`; `x-owner` records the
owning tool (the write boundary the kit enforces).

**CLI:**
```
llm-wiki admin schema register <type> <schema-path> [--template <md-path>] [--wiki <name>]
```

**MCP** (`wiki_admin_schema_register` — content travels in the call,
no server-side paths):
```json
{
  "type": "<type>",
  "schema": "<JSON Schema content>",
  "body_template": "<Markdown template content, optional>",
  "wiki": "<name>"
}
```

**Output:** `{ status: "registered" | "unchanged", ... }`. On a live
server, `registered` remounts the wiki so the new type validates and
indexes immediately.

### add

Register a custom type by copying a schema file into the wiki and
optionally adding a `[types.*]` override to `wiki.toml`. CLI-only —
it reads a path on the server, which a transport client doesn't have;
transport clients use `wiki_admin_schema_register`.

**CLI:**
```
llm-wiki admin schema add <type> <schema-path> [--wiki <name>]
```

**Behavior:**

1. Validate the schema file (valid JSON, valid JSON Schema)
2. Copy it to `<wiki>/schemas/<filename>`
3. If the schema has `x-wiki-types` declaring the type → done
   (auto-discovered on next build)
4. If not → add a `[types.<type>]` entry to `wiki.toml` pointing
   to the copied schema file
5. Run `validate` on the result to confirm index resolution works

**Output:** confirmation of what was done.

### remove

Unregister a type, remove its pages from the index, and optionally
delete page files from disk.

**CLI:**
```
llm-wiki admin schema remove <type> [--delete] [--delete-pages] [--dry-run] [--wiki <name>]
```

**MCP** (`wiki_admin_schema_remove`):
```json
{
  "type": "<type>",
  "delete": true,
  "delete_pages": true,
  "dry_run": true,
  "wiki": "<name>"
}
```

**Behavior:**

1. Cannot remove the `default` type — error
2. Count pages of this type in the index
3. If `--dry-run` → report what would be done, stop
4. Remove pages of this type from the tantivy index
5. If `--delete-pages` → delete the `.md` files from disk
6. If `[types.<type>]` exists in `wiki.toml` → remove the entry
7. If `--delete` → remove the type from `x-wiki-types` in the
   schema file, or delete the schema file entirely if it declares
   only this type

**Output:**
- Pages removed from index: N
- Page files deleted from disk: N (if `--delete-pages`)
- `wiki.toml` entry removed: yes/no
- Schema file modified/deleted: yes/no (if `--delete`)

**Flags:**
- `--delete` — also modify/delete the schema file
- `--delete-pages` — also delete page `.md` files from disk
- `--dry-run` — show what would be done without doing it

### validate

Validate schema files and index resolution.

**CLI:**
```
llm-wiki schema validate [<type>] [--wiki <name>]
```

**MCP:**
```json
{
  "action": "validate",
  "type": "<type>",
  "wiki": "<name>"
}
```

**Behavior:**

- If `<type>` is given → validate that type's schema file only
- If omitted → validate all schema files in `schemas/`

**Checks:**

1. File is valid JSON
2. File is valid JSON Schema (Draft 2020-12)
3. `x-wiki-types` is present and non-empty (warning if missing)
4. Base schema invariant: `default` type requires `title` and `type`
5. `x-index-aliases` targets are valid (no cycles, targets exist
   as properties in some schema)
6. Index resolution: run `build_wiki()` as a dry-run — confirms
   that the full set of schemas produces a valid tantivy schema
   with no field conflicts

Check 6 is the key one — it catches problems that individual schema
validation misses:
- Two schemas classifying the same field differently (text vs keyword)
- Alias targets that don't resolve to any known field
- Missing `default` type after discovery

**Output:** ok or list of errors/warnings per schema file.

## MCP Tool Definitions

```json
{
  "name": "wiki_schema",
  "description": "Inspect type schemas (read-only)",
  "annotations": { "readOnlyHint": true },
  "parameters": {
    "action": "list | show | validate",
    "type": "(for show; optional for validate) type name",
    "template": "(for show) return frontmatter template instead of schema",
    "wiki": "target wiki name (uses default if omitted)"
  }
}
```

```json
{
  "name": "wiki_admin_schema_register",
  "description": "Register a type schema (idempotent)",
  "parameters": {
    "type": "type name (declared in the schema's x-wiki-types)",
    "schema": "JSON Schema content",
    "body_template": "Markdown body template content (optional)",
    "wiki": "target wiki name (uses default if omitted)"
  }
}
```

```json
{
  "name": "wiki_admin_schema_remove",
  "description": "Unregister a type and remove its pages from the index",
  "annotations": { "destructiveHint": true },
  "parameters": {
    "type": "type name",
    "delete": "also delete/modify the schema file",
    "delete_pages": "also delete page files from disk",
    "dry_run": "show what would be done without doing it",
    "wiki": "target wiki name (uses default if omitted)"
  }
}
```

## Relationship to Other Tools

- `wiki_admin_config list` returns wiki identity and settings — not types.
  `wiki_schema list` returns the type registry.
- `wiki_content_new` scaffolds a page with minimal frontmatter.
  `wiki_schema show --template` returns the full type-specific
  template without creating a file.
- `wiki_ingest` validates against the schema. `wiki_schema show`
  lets you inspect what it validates against.
- `wiki_admin_index_rebuild` rebuilds the full index.
  `wiki_admin_schema_remove` removes only pages of a specific type.

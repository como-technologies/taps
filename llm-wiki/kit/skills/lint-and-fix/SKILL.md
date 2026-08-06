---
name: lint-and-fix
description: Run the Como KB lint pass and fix what it finds - broken links, orphans, missing fields, unknown types - without crossing the decision boundary. Use for KB hygiene passes or after a batch of authoring.
---

# Lint and fix

Mechanical hygiene, bounded by ownership. Follow the Como authoring
contract (`docs/guides/como-authoring.md`).

1. **Run**: `wiki_lint` (JSON output). Triage by severity: errors gate,
   warnings advise.
2. **Fix what agents own** — content pages (`guide`, `glossary-entry`,
   `worked-example`). All edits go through the tool surface:
   `wiki_content_read` the page, `wiki_content_write` the corrected
   version — never a filesystem edit.
   - `broken-link`: repoint to the current slug (`wiki_search` finds
     it); if the target truly doesn't exist, either author the missing
     stub or remove the link — say which you chose and why.
   - `orphan`: link the page into its neighborhood via `wiki_suggest`;
     an orphan that genuinely belongs nowhere is a candidate for
     retirement — flag it, don't silently keep or delete it.
   - `missing-fields` / `unknown-type`: fix the frontmatter to the
     schema. If the schema seems wrong, stop and surface it — schemas
     change by human decision, not during a lint pass.
3. **Never touch tool-owned classes** (`decision`, `plan`,
   `measure-report`). Their lint findings belong to their owning
   tools: report them, and where the session has that tool's door
   (for decisions: adroit's MCP server or CLI, `adroit check` /
   `adroit relink`) route the fix through it — no direct edits, ever.
4. **Re-gate**: `wiki_ingest` the touched pages, re-run `wiki_lint`,
   and report before/after error and warning counts. Zero introduced
   errors is the bar; pre-existing warnings you deliberately left are
   listed, not hidden.

A lint pass edits shape, not meaning: if a fix would change what a page
*claims*, that's authoring, not linting — switch skills and say so.

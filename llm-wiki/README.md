# llm-wiki

A headless wiki engine for agents. 23 MCP tools. One Rust binary. No LLM inside.

**Build knowledge that compounds — not answers that evaporate.**

> New to the TAPS suite? Start with the
> [Getting Started guide](https://como-technologies.github.io/taps/getting-started/).

A git-backed Markdown wiki — searchable, typed, graph-linked. Accessible from
the command line, from any MCP-compatible agent, or from any IDE via ACP.

llm-wiki is the **Como KB product** — the knowledge-base substrate of the
Como suite. It ships the engine-owned content-class schemas and zero-flag
admission provisioning in `spaces create` (tools register their own
artifact classes over the transport via `wiki_schema register`), and the
**[Como authoring kit](kit/README.md)** — skills, harness configs, the
[authoring contract](docs/src/guides/como-authoring.md), and a captured
worked example — for pointing an AI harness at a space and authoring
content that lands in the right shape. The KB contract itself is specified
in [docs/src/specifications/como-kb-spec.md](docs/src/specifications/como-kb-spec.md).

---

## The problem with RAG

Most AI knowledge tools retrieve and generate on every query. Each answer is
disposable — nothing is learned, nothing is kept. Ask the same question twice
and the LLM reasons from scratch.

llm-wiki implements a different pattern — the **Dynamic Knowledge Repository**
(DKR), introduced by Andrej Karpathy:

> Process sources at ingest time, not query time. The LLM integrates each
> source into the wiki — updating concept pages, creating source summaries,
> flagging contradictions — and commits the result. Knowledge compounds with
> every addition.

|                         | Traditional RAG       | llm-wiki (DKR)              |
| ----------------------- | --------------------- | --------------------------- |
| When knowledge is built | At query time         | At ingest time              |
| Cross-references        | Ad hoc or missed      | Pre-built, typed graph      |
| Knowledge accumulation  | Resets each query     | Compounds over time         |
| Audit trail             | None                  | Git history per page        |
| Data ownership          | Provider systems      | Your files, your git repo   |

---

## How it works

The engine is pure infrastructure. It manages files, git, full-text search,
and graph structure. The LLM is always external — it calls the engine's tools
via MCP, reads pages, writes pages, and commits knowledge. Intelligence flows
through skills, not the binary.

```
LLM agent
  │
  ├── wiki_list(format: "llms")             → all pages grouped by type
  ├── wiki_search("mixture of experts")     → ranked results + facets
  ├── wiki_content_read("concepts/moe")     → full page + backlinks
  ├── wiki_graph(root: "concepts/moe")      → typed graph in Mermaid/DOT
  ├── wiki_suggest("concepts/moe")          → pages worth linking
  ├── wiki_content_new("concepts/new-page") → scaffold + returns local path
  ├── [write directly to path]              → no MCP round-trip
  └── wiki_ingest(path: "concepts/")        → validate, index, commit
```

A wiki page is a plain Markdown file with typed frontmatter:

```yaml
---
type: concept
title: Mixture of Experts
status: active
confidence: 0.9
tags: [routing, scaling, efficiency]
sources:
  - sources/switch-transformer-2021
  - sources/mixtral-2024
concepts:
  - concepts/sparse-routing
  - concepts/scaling-laws
---

Sparse routing of tokens to expert subnetworks...
```

The engine validates frontmatter against a JSON Schema, extracts typed graph
edges from `sources` and `concepts`, and indexes everything in tantivy. The
graph is live the moment a page is committed. The default schema library
lives in [`schemas/`](schemas/).

---

## Build

llm-wiki is a member of the taps Cargo workspace. From the workspace root:

```bash
cargo build --release -p llm-wiki
```

The binary lands at `target/release/llm-wiki`. Put it on your `PATH`, or
run it in place with `cargo run -p llm-wiki --`.

---

## Quick start

```bash
# Create a wiki space
llm-wiki spaces create ~/wikis/research --name research

# Start the MCP server
llm-wiki serve
```

Connect your agent or editor via its MCP config. The 23 tools are
immediately available.

→ [Getting started guide](docs/src/guides/getting-started.md)

---

## IDE integration via ACP

In addition to MCP, llm-wiki speaks **ACP** (Agent Client Protocol) — a
session-oriented streaming protocol over stdio. Connect from Zed or any
ACP-compatible editor and trigger built-in workflows directly from the IDE
panel:

| Prompt | What runs |
| ------ | --------- |
| `llm-wiki:research <query>` | Search + read top results, stream summaries |
| `llm-wiki:lint [rules]` | Run structural lint rules, stream findings |
| `llm-wiki:graph [root]` | Build and stream the concept graph |
| `llm-wiki:ingest [path]` | Ingest a path, stream the report |
| `llm-wiki:use <slug>` | Stream a page body directly into the IDE |
| `llm-wiki:help` | List all available workflows |

Start with `--acp` alongside `--http` to give ACP exclusive stdio:

```bash
llm-wiki serve --acp --http :18765
```

→ [ACP transport](docs/src/specifications/integrations/acp-transport.md) · [Configuration](docs/src/guides/configuration.md)

---

## What agents can do

| Tool | What it does |
| ---- | ------------ |
| `wiki_search` | BM25 full-text search across one or all wikis, with type/status/tag facets |
| `wiki_list` | Paginated page listing with filters; `format: "llms"` for LLM-readable output |
| `wiki_content_read` | Read a page with optional backlinks |
| `wiki_content_write` | Write a page (validates frontmatter against type schema) |
| `wiki_content_new` | Scaffold a new page; returns local `path` for direct writes |
| `wiki_resolve` | Resolve a slug or `wiki://` URI to its local filesystem path |
| `wiki_ingest` | Validate a path, update the index, commit to git |
| `wiki_graph` | Typed concept graph — Mermaid, DOT, or natural-language `llms` format |
| `wiki_suggest` | Find pages worth linking by tag overlap, graph distance, BM25 similarity |
| `wiki_stats` | Wiki health: page counts, type distribution, staleness, graph density |
| `wiki_lint` | Deterministic quality rules: orphans, broken links, missing fields, stale pages |
| `wiki_export` | Write full wiki to `llms.txt` at wiki root — for ecosystem publishing or audit |
| `wiki_history` | Git commit history for a page, with rename following |
| `wiki_schema` | Show, validate, or template a type schema |
| `wiki_spaces_*` | Create, register, list, remove wiki spaces; supports custom `wiki_root` |

Full tool reference: [`docs/src/specifications/tools/`](docs/src/specifications/tools/)

---

## Stable page ids

By default a page's identity is its path, so moving a file breaks every
inbound link. A page may opt in to a **stable id** — a tool-generated
ULID in frontmatter:

```yaml
---
id: 01ARZ3NDEKTSV4RRFFQ69G5FAV
title: "Use Postgres"
type: doc
---
```

Links and addresses (`[[01ARZ…]]`, `superseded_by: 01ARZ…`,
`wiki_content_read(uri: "01ARZ…")`) resolve by slug first, then by id —
so id links survive `git mv` and existing slug links behave exactly as
before. `content new --id` generates one; `lint` enforces uniqueness
(`duplicate-id`) and shape (`id-format`). Wikis without ids are
unaffected. Full contract:
[`docs/src/specifications/model/page-identity.md`](docs/src/specifications/model/page-identity.md).

---

## Skills

The engine exposes tools. Skills tell agents how to use them.

The [Como authoring kit](kit/README.md) ships the skills, harness configs,
and authoring contract used to point an AI harness at a wiki space. Skills
are plain Markdown files — readable by the LLM, replaceable, forkable.
Write your own for your own workflows. The engine has no opinions about
workflows, LLM providers, or interfaces: every LLM call happens outside
the binary, and nothing is coupled to a specific AI provider.

---

## Technology

The file format is Markdown. The history store is git. Both predate llm-wiki
and will outlive it — your wiki is readable, diffable, and portable with zero
dependency on this tool. The engine itself is a single Rust binary with no
runtime, no database, and nothing to keep running between sessions.

| Component | Technology |
| --------- | ---------- |
| Search | [tantivy](https://crates.io/crates/tantivy) — BM25, Lucene-class performance |
| Git | [git2](https://crates.io/crates/git2) — libgit2 bindings |
| Graph | [petgraph](https://crates.io/crates/petgraph) — typed DiGraph |
| MCP | [rmcp](https://crates.io/crates/rmcp) — stdio + Streamable HTTP |
| ACP | [agent-client-protocol](https://crates.io/crates/agent-client-protocol) |

---

## Documentation

| | |
| - | - |
| [Getting started](docs/src/guides/getting-started.md) | End-to-end walkthrough |
| [Guides](docs/src/guides/README.md) | Configuration, custom types, multi-wiki, lint, graph |
| [Specifications](docs/src/specifications/README.md) | Formal tool and model contracts |
| [Como KB spec](docs/src/specifications/como-kb-spec.md) | The suite's knowledge-base contract |
| [Roadmap](docs/roadmap.md) | What shipped, what's next |
| [Decisions](docs/decisions/README.md) | Architectural decision records |

Schemas for the default page types live in [`schemas/`](schemas/).

---

## Contributing

[Contributing guide](CONTRIBUTING.md)

## Credits

llm-wiki originated as [geronimo-iia/llm-wiki](https://github.com/geronimo-iia/llm-wiki)
by Jerome Guibert, building on Andrej Karpathy's
[LLM Wiki gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
that defined the Dynamic Knowledge Repository pattern. Como maintains this
codebase independently in the taps workspace. Dual MIT/Apache-2.0 licensing
is retained with the upstream copyright — see [LICENSE-MIT](LICENSE-MIT)
and [LICENSE-APACHE](LICENSE-APACHE).

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)

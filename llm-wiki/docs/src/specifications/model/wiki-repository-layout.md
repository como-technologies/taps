---
title: "Wiki Repository Layout"
summary: "The model — a knowledge base has wikis, a wiki has content/ — and how a wiki repository is structured: wiki.toml, schemas/, three content layers, and the two roots."
read_when:
  - Deciding where to put a new file in the wiki repository
  - Understanding the three-layer DKR structure
  - Understanding what llm-wiki admin create does
status: ready
last_updated: "2025-07-17"
---

# Wiki Repository Layout

The vocabulary, in one line: **a knowledge base has one or more wikis,
and a wiki has a `content/` directory of pages.**

- **Knowledge base (KB)** — the appliance: one engine process and its
  registry (`$XDG_CONFIG_HOME/llm-wiki/config.toml`). `KB_URL` points at it.
- **Wiki** — the corpus: one git repository, one `[[wikis]]` registry
  entry, strictly 1:1. `KB_WIKI` names it.
- **`content/`** — the pages directory inside a wiki (the
  `content_root`). Everything the engine indexes, searches, and graphs
  lives here.

A wiki repository is a git repo with a fixed top-level structure:

```
my-wiki/                    ← git root (repository root)
├── README.md               ← for humans (name, description, usage)
├── wiki.toml               ← wiki config + type registry
├── schemas/                ← JSON Schema + body templates per page type
│   ├── base.json
│   ├── concept.json
│   ├── concept.md          ← body template (optional)
│   ├── paper.json
│   ├── paper.md            ← body template (optional)
│   ├── skill.json
│   ├── doc.json
│   ├── doc.md              ← body template (optional)
│   ├── section.json
│   ├── section.md          ← body template (optional)
│   ├── decision.json       ← Como schema library (llm-wiki#14)
│   ├── guide.json
│   ├── glossary-entry.json
│   ├── worked-example.json
│   └── plan.json
├── inbox/                  ← drop zone (human puts files here)
├── evidence/               ← immutable capture layer (originals preserved)
└── content/                ← compiled knowledge (default; configurable via content_root)
```

The content directory name defaults to `content/` but can be changed via `content_root` in `wiki.toml` (e.g. `content_root = "pages"` for repos where pages already live elsewhere). The `inbox/`, `evidence/`, and `schemas/` directories are always named exactly as shown. The name `raw` (the pre-rename name of the capture layer) also stays reserved so existing wikis keep working; `content_root` may not use it.

No hidden directories in the repo. No `schema.md` — `wiki.toml` is the
single source of truth for wiki identity, engine configuration, and the
type registry. See [type-system.md](type-system.md).


## Top-Level Files and Directories

**`wiki.toml`** — wiki identity, engine configuration, and optional
type overrides. The LLM reads it via `wiki_admin_config`.

**`schemas/`** — JSON Schema files (Draft 2020-12) that define
frontmatter per page type. Each schema declares which types it serves
via `x-wiki-types`. The engine discovers types by scanning this
directory — no registration in `wiki.toml` needed for the common case.
Optional `.md` files alongside schemas provide body templates for
`wiki_content_new` (e.g. `concept.md` next to `concept.json`).

**`inbox/`** — human interface. Drop files here for the LLM to process.

**`evidence/`** — immutable capture layer. Originals preserved, never
indexed. Wikis created before the rename use `raw/` for this layer;
both names are reserved.

**`content/`** (or the value of `content_root`) — compiled knowledge. Authors
(human or LLM) write directly here. Everything inside is a page or asset.
The engine indexes it, searches it, and builds the concept graph from it.


## Folder Structure Inside the Wiki Root

The owner's choice. The engine enforces nothing about categories — only
the `inbox/` → `evidence/` → `<content_root>/` flow matters. Epistemic distinctions
are carried by the `type` field, not by folders. See
[epistemic-model.md](epistemic-model.md).


## Roots

**Repository root** — the git repository directory. Contains
`wiki.toml`, `schemas/`, `inbox/`, `evidence/`, and the wiki content directory.
Created by `llm-wiki admin create`.

**Content root** — `<repo>/<content_root>/` (default: `<repo>/content/`).
All page slugs are relative to it. Set via `content_root` in `wiki.toml`.

---
title: "Engine State"
summary: "Where the engine stores its state — global config and wiki registry under XDG config, search indexes and logs under XDG state."
read_when:
  - Understanding where engine state lives on disk
  - Understanding the separation between wiki repo and engine state
  - Diagnosing index or config issues
status: ready
last_updated: "2025-07-17"
---

# Engine State

Engine state lives outside the wiki repository, split along XDG lines:
the registry under config, everything derived under state. It is local
to the machine — never committed, never shared.

```
$XDG_CONFIG_HOME/llm-wiki/
└── config.toml             ← global config + wiki registry

$XDG_STATE_HOME/llm-wiki/
├── indexes/
│   └── <name>/             ← per-wiki index
│       ├── search-index/   ← tantivy files
│       ├── schema.json     ← computed index schema
│       └── state.toml      ← indexed commit, page count, built date
├── snapshots/              ← graph snapshot cache
└── logs/                   ← rotating log files for llm-wiki serve
```

When `--config` / `LLM_WIKI_CONFIG` names a config file explicitly, all
state lives beside that file instead — one flag pins config *and* state,
the hermetic-world trick tests and nested appliances rely on.


## Global Config

`$XDG_CONFIG_HOME/llm-wiki/config.toml` holds the wiki registry (which wikis are
registered and where they live) and global defaults. Created
automatically on the first `llm-wiki admin create`.

See [global-config.md](../model/global-config.md) for the full
key reference.


## Search Indexes

One index per wiki at `$XDG_STATE_HOME/llm-wiki/indexes/<name>/`. The search index
is a derived artifact — rebuildable from committed files at any time
via `llm-wiki admin index rebuild`.

`state.toml` tracks the indexed commit, page count, and build date.
The engine uses it for staleness detection.

See [index-management.md](index-management.md) for staleness, schema
versioning, and auto-recovery.


## Logs

`$XDG_STATE_HOME/llm-wiki/logs/` holds rotating log files written by
`llm-wiki serve`. Created automatically on first use.

See [server.md](server.md) for logging configuration.

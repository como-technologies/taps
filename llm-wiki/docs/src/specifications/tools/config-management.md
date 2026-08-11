---
title: "Config Management Tool"
summary: "get, set, and list configuration values."
read_when:
  - Reading or writing configuration via CLI or MCP
status: ready
last_updated: "2025-07-17"
---

# Config Management

MCP tool: `wiki_admin_config`

| Command                    | Description              |
| -------------------------- | ------------------------ |
| `config get <key>`         | Read a config value      |
| `config set <key> <value>` | Write a config value     |
| `config list`              | List all resolved config |

Operates on `wiki.toml` (per-wiki) or `$XDG_CONFIG_HOME/llm-wiki/config.toml`
(global). For the file formats and full key reference, see
[wiki-toml.md](../model/wiki-toml.md) and
[global-config.md](../model/global-config.md).


## CLI

```
llm-wiki admin config get <key>
llm-wiki admin config set <key> <value>
                [--global]          # write to $XDG_CONFIG_HOME/llm-wiki/config.toml
                [--wiki <name>]     # write to per-wiki wiki.toml
llm-wiki admin config list
             [--global]
             [--wiki <name>]
             [--format <fmt>]    # text | json (default: text)
```

`set` without `--global` writes to the per-wiki `wiki.toml` of the
default wiki (or `--wiki <name>` target). With `--global` it writes to
`$XDG_CONFIG_HOME/llm-wiki/config.toml`.

Global-only keys (`index.*`, `serve.*`, `logging.*`) reject
`--wiki` with an error.

### Examples

```bash
llm-wiki admin config get defaults.search_top_k
llm-wiki admin config set defaults.search_top_k 15 --global
llm-wiki admin config set defaults.page_mode bundle --wiki research
llm-wiki admin config list
llm-wiki admin config list --global --format json
```

### config list output

Text (default):

```
defaults.search_top_k    = 10
defaults.page_mode       = flat
ingest.auto_commit       = true
validation.type_strictness = loose
```

JSON (`--format json`):

```json
{
  "defaults.search_top_k": 10,
  "defaults.page_mode": "flat",
  "ingest.auto_commit": true,
  "validation.type_strictness": "loose"
}
```

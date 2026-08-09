# como-kb-client

The shared KB transport client for taps tools — how a tool finds and
dials the knowledge base. Nothing more: the KB's own model and tool
surface belong to [llm-wiki's docs](../llm-wiki/docs/src/SUMMARY.md)
(start at the
[wiki repository layout](../llm-wiki/docs/src/specifications/model/wiki-repository-layout.md)
for the KB → wiki → `content/` vocabulary).

## The pair

A tool is told where the KB is by two variables:

| Variable  | Meaning |
|-----------|---------|
| `KB_URL`  | the appliance's streamable-HTTP MCP endpoint, e.g. `http://kb:8080/mcp` |
| `KB_WIKI` | the target wiki's name (optional — omitted means the appliance's default wiki) |

`KB_URL` unset or blank means *no KB configured*: tools degrade with a
clear message, never an error.

## The discovery order

Where the pair lives is one suite-wide order, owned here so every tool
answers identically:

1. the process environment,
2. a `.env` in the tool's working directory,
3. the user-level `~/.config/taps/env` — written once when an
   appliance stands up (the Getting Started guide's Step 2), inherited
   by every tool ever after.

Nearest layer wins; loads never override what's already set.

## The surface

- `load_env()` — layer the config files into the process environment.
  Call once at startup, *before* clap parses, so `env =` defaults see
  the files.
- `KbTarget::discover()` — `load_env()` + read the pair. The one call
  sites want. (`KbTarget::from_env()` reads without loading.)
- `KbClient::connect(&target)` — open the MCP session;
  `call` / `call_json` invoke tools, injecting the target wiki as the
  `wiki` argument unless the caller already set one; `close()` ends
  the session cleanly.

The transport surface is the only door: nothing here reads the
engine's registry or touches a wiki's filesystem.

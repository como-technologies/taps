# The Como authoring kit

Everything needed to point an AI harness at a Como KB and author
content that lands in the right shape. The contract the kit implements is
[docs/src/guides/como-authoring.md](../docs/src/guides/como-authoring.md); the
decision behind the model is portfolio ADR-0010 (harness-first), plan of
record portfolio#7; the workspace/appliance posture is taps issue 53.

The engine stays a dumb pipe — no LLM inside. The harness brings the
model; this kit brings the instructions and wiring.

## The posture: workspace and appliance

Spaces live behind a running `llm-wiki serve` — **the appliance** — and
every client reaches them through its transport surface (MCP tools over
stdio or streamable HTTP). The harness runs in an **authoring
workspace**: a thin directory holding only the config in this kit, no
corpus. One workspace session reaches every space its wired appliances
host; spaces are addressed by name per tool call. Nothing but the
engine touches a space's filesystem.

## Layout

```
kit/
  README.md            this file
  workspace/           the authoring workspace template — copy it whole
    .mcp.json          appliance wiring (template: incus-container appliance named "kb")
    CLAUDE.md          the session rules — tools are the only door
    .claude/
      settings.json    deny rules: the harness cannot shell into the appliance
  skills/              Como skills (Claude Code format; the procedures)
    author-guide/
    author-glossary/
    research/
    lint-and-fix/
  starter/             starter content for a fresh space
  claude-desktop/
    README.md          config snippet + current limits (MCP-only harness)
  worked-example/
    session.md         a real captured authoring session, gates and all
```

## Setup — Claude Code

1. Stand up an appliance: `llm-wiki serve` wherever you deploy it —
   terminal process, systemd unit, incus container, k8s pod. The engine
   is topology-agnostic; its registry is `~/.llm-wiki/config.toml` (or
   `$LLM_WIKI_CONFIG`) *where the appliance runs*. The Getting Started
   guide's walked path is an incus container named `kb`.
2. Create the space on the appliance:
   `llm-wiki spaces create <dir> --name <name>` (run on the appliance,
   or call the `wiki_spaces_create` tool once connected). Provisioning
   installs the Como schema library, strict validation, admission
   hooks, and search weights.
3. Make a workspace: copy `kit/workspace/` (the whole directory,
   dotfiles included) to a fresh directory, and `kit/skills/` to
   `<workspace>/.claude/skills/`.
4. Edit `.mcp.json` to reach *your* appliance. The template spawns a
   stdio session into an incus container named `kb` — the dev-mode
   path. Variants:

   ```jsonc
   // local process (dev-mode, no container — no isolation boundary)
   "kb": { "command": "llm-wiki", "args": ["serve"] }

   // team appliance over streamable HTTP (the prod path; real boundary)
   // (appliance side: `llm-wiki serve --http`; default port 8080)
   "kb": { "type": "http", "url": "http://kb.internal:8080/mcp" }
   ```

   The `.claude/settings.json` rules and the `.mcp.json` entry key all
   name the appliance `kb` — keep them in sync if you rename. The
   settings pre-authorize the authoring lane (search, read, write,
   ingest, lint, …) since the engine's gates live server-side; the
   sharp lifecycle tools — space create/remove, `wiki_schema`,
   `wiki_config` — still prompt, because permission rules can't see
   arguments and those can delete or loosen things.
5. Open Claude Code in the workspace and talk. The worked example shows
   a full session.

One appliance per `.mcp.json` entry; a workspace may wire several. Note
each stdio entry spawns its own engine process per session — fine for
one author, but concurrent sessions against one appliance belong on the
HTTP transport (one server, many sessions).

## Setup — Claude Desktop

See [claude-desktop/README.md](claude-desktop/README.md). Content classes
(`guide`, `glossary-entry`, `worked-example`) work over llm-wiki's MCP
server; decisions work through adroit's guarded MCP write slice
(`adroit mcp --allow-write`, its ADR-0021) — destructive-annotated tools
the human approves per call.

## Why ad-hoc corpus checks don't use `spaces create`

Recorded here per portfolio#7 (wave 1): seeding a committed decision
corpus into an ephemeral space for an `adroit check` hand-scaffolds a
two-line `wiki.toml` instead of calling `llm-wiki spaces create`,
deliberately — the corpus check is adroit's, and requiring an llm-wiki
build buys no additional assertion there. Provisioning is exercised
where it matters: this repo's own tests (`tests/spaces.rs`) cover the
schema library, hooks, strictness, and weights on every run, and every
kit session starts with a real `spaces create`. If a corpus check ever
needs admission-hook coverage, that is the moment to revisit — not
before.

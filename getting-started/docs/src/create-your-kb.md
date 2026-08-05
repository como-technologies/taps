# Step 2 — Create your knowledge base

Everything the loop produces — the assessment's findings, the decisions,
the plans, the measurements — lands in one place:
[the knowledge base](../portfolio/knowledge-base.html). Before anything else, you
stand up the space that every later step writes to.

## Create the space

```sh
llm-wiki spaces create ~/myproject-kb --name myproject --set-default
```

One command provisions the whole thing: the Como schema library, strict
validation, admission hooks, and search weights. No flags to remember —
a fresh space is born ready.

## Wire up your harness

The KB's conversational door is your own AI harness, configured from the
shipped **authoring kit**:

```sh
cp  ~/taps/llm-wiki/kit/claude-code/.mcp.json  ~/myproject-kb/
cp  ~/taps/llm-wiki/kit/claude-code/CLAUDE.md  ~/myproject-kb/
mkdir -p ~/myproject-kb/.claude
cp -r ~/taps/llm-wiki/kit/skills ~/myproject-kb/.claude/skills
```

Open Claude Code (or your MCP client — see the kit's Claude Desktop
notes) in `~/myproject-kb` and talk: search the corpus, draft pages, file guidance.
Every write goes through the same validation gates regardless of who — or
what — authored it.

> 🚧 **Unverified.** The exact `.mcp.json` contents may assume `llm-wiki`
> and `adroit` on `PATH` — Step 1's `just install` provides that; the
> walk will confirm the wiring end to end.

## Verify

```sh
llm-wiki spaces list             # your space: registered, default
llm-wiki lint --wiki myproject   # empty is fine; zero errors is the bar
```

Your space is provisioned and validated — a blank canvas. Step 3 gives
it something to react to.

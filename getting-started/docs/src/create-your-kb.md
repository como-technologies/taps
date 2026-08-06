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

No harness on this machine yet? (A Step 0 clean room won't have one.)
Claude Code installs in one line, and its first launch handles a
browserless box gracefully — it prints a login URL; open that on any
machine with a browser, sign in, and paste the code back:

```sh
curl -fsSL https://claude.ai/install.sh | bash
```

Now start your session **in the KB directory** — not the `~/taps`
checkout you just cloned, tempting as that is. The kit you copied is
per-directory config: Claude Code only loads the `.mcp.json`, the
`CLAUDE.md` conventions, and the skills from where it starts.

```sh
cd ~/myproject-kb
claude
```

Then talk: search the corpus, draft pages, file guidance. `/mcp` should
show the `llm-wiki` server connected (it finds `llm-wiki` and `adroit`
on `PATH` — Step 1's `just install` put them there), and every write
goes through the same validation gates regardless of who — or what —
authored it.

## Verify

```sh
llm-wiki spaces list             # your space: registered, default
llm-wiki lint --wiki myproject   # empty is fine; zero errors is the bar
```

Your space is provisioned and validated — a blank canvas. Step 3 gives
it something to react to.

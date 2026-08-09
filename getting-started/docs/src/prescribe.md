# Step 4 — Prescribe

An assessment that stays a scorecard changes nothing. This step turns
Step 3's report into a backlog of **proposed decisions** in your wiki —
decision records your team refines, debates, and accepts. The tool is
[adroit](../portfolio/loop/prescribe.html), and it works the way every
taps tool now does: the KB is the state store, reached over the
appliance's transport. adroit owns two page classes, `decision` and
`plan`; your wiki learns them the moment adroit first writes, and only
adroit writes them.

Prescribing is judgment. Nothing in this step parses the report or
generates decisions from a template — a session (or you, at the
terminal) *reads* the report like any other page and decides what's
worth deciding. adroit just records the judgment and keeps the corpus
honest.

adroit already knows where the KB is — the suite pair you wrote once
in Step 2 (`~/.config/taps/env`), read through the same discovery
order as every taps tool. No other configuration exists: adroit has no
data directory and no filesystem mode. If the KB is down, adroit says
so and does nothing.

## Hand your session the tool

Like amaker in Step 3, the `adroit` binary serves its whole command
surface over MCP (`adroit mcp`) — the agent drives the same verbs a
terminal would, as typed tools. Add it beside `kb` and `amaker`, and
extend the walk's pre-grant to trust it:

```sh
cd ~/kb-workspace
python3 - <<'EOF'
import json
cfg = json.load(open(".mcp.json"))
cfg["mcpServers"]["adroit"] = {"type": "stdio", "command": "adroit", "args": ["mcp"]}
json.dump(cfg, open(".mcp.json", "w"), indent=2)
s = json.load(open(".claude/settings.local.json"))
s["permissions"]["allow"].append("mcp__adroit")
json.dump(s, open(".claude/settings.local.json", "w"), indent=2)
EOF
```

(A stdio server pins its binary for the life of a session — if you
ever rebuild the suite mid-walk, reconnect `/mcp` or restart the
session so the door serves the new tool.)

## Seed the backlog from the report

Paste this into your **workspace session**:

```text
Read the assessment report page in the knowledge base (list pages of
type assessment-report, then read it). From its gaps, propose the few
decisions worth making — quality over coverage. Draft each as a full
decision record: context and problem statement, at least two considered
options, a decision outcome, and honest negative consequences. Create
each with the adroit tools, linking it to the report page with relates.
Leave every decision proposed — accepting is my seat, not yours. Finish
with adroit's list so I can see the backlog.
```

Watch the first `new` land: adroit registers its two schemas on first
contact (`registered`, then `unchanged` ever after), allocates the
reference (`ADR-0001` — max existing in the wiki + 1), and every page
enters through the same admission gates as everything else. The
`relates` link is a real graph edge from each decision back to the
report — provenance the engine's lint keeps honest, not import
metadata.

## Refine — this part is you

Work the backlog until each record says what you mean. All the doors
sit on the same store:

```sh
adroit list                 # the backlog, reference-sorted
adroit show 1               # read before you sign
adroit lint 1               # authoring-quality gate — mechanical, no AI
adroit edit 1 --body-file revised.md   # replace a proposed body wholesale
```

Or stay in conversation — the session refines through the same `edit`
tool, and every rewrite preserves what it doesn't own: frontmatter
fields, foreign keys, anything another tool hung on the page. `edit`
works on **proposed** decisions only; a decided record is history, and
history gets superseded, not rewritten.

## Accept what you mean to do

```sh
adroit set-status 1 accepted
adroit check                # the corpus stays sound
```

The lifecycle is deliberately narrow: a proposal is decided
(`accepted` / `rejected`), an accepted decision can age out
(`deprecated`), and replacement is one atomic verb —
`adroit supersede <new> <old>` links both sides' frontmatter in a
single stroke and lands as a single commit in your wiki's history.
Terminal states don't come back: reopening a question is a *new*
decision that `--relates` to the old one.

Accepted decisions are the contract with the next stage: Step 5 reads
**only** accepted records. Everything still `proposed` waits, visible
but inert.

## Verify

Ask your workspace session:

```text
What have we decided so far? Answer from accepted decisions only, and
cite pages.
```

It searches and reads adroit's pages with the ordinary wiki tools — no
adroit dependency; ownership is a *write* boundary. Then check the
corpus and the vocabulary from the outside:

```sh
adroit check    # supersession integrity, duplicate refs/ids — exits non-zero on errors
incus exec kb -- su - kb -c 'llm-wiki schema list --wiki myproject'
```

`decision` and `plan` now sit beside amaker's classes, each carrying
`x-owner: adroit` — the wiki learned them at first contact. Your
backlog is real, your acceptances are recorded, and the loop moves on:
Step 5 turns accepted decisions into adopted change.

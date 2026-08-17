# Step 5 — Adopt

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

Accepted decisions become landed, priced change. The engine is
[conduit](../portfolio/loop/adopt.html), and it works the way the rest
of the suite now does, taken to its conclusion: **work items are pages
in your wiki** — `project`, `story`, `task`, three classes conduit owns
and your wiki learns at first contact — and the only way their state
ever changes is through conduit's doors. Humans gate **intent**: you
sign off what should happen and you close what finished. Nobody —
human or harness — reviews their way onto `main`: a task lands only
through a **mechanical merge door** that verifies the signed contract
and runs the project's gate, then writes exactly one squash commit.

There is no forge in this step. Each project owns an **internal git
repository** that conduit provisions inside your execution workspace
(`.conduit/repos/`); work lands there, and the pages carry the
telemetry (`merge_commit`, `work_ms`) that Step 6 prices. Mirroring to
GitHub or GitLab is a later integration, not a prerequisite for
adopting change.

conduit already knows where the KB is — the suite pair from Step 2,
same discovery order as every taps tool. No other configuration
exists.

## Make the execution workspace

Like the authoring workspace in Step 2, but for the Adopt room: the
shipped kit is the harness config, and the skills are the procedures.

```sh
cp -r ~/taps/conduit/kit/workspace ~/conduit-workspace
cp -r ~/taps/conduit/kit/skills ~/conduit-workspace/.claude/skills

# point the kb entry at the appliance's standing door — one engine,
# every client, same as every step before this one
KB_ADDR=$(incus list kb -c4 -f csv | cut -d" " -f1)
cat > ~/conduit-workspace/.mcp.json <<EOF
{
  "mcpServers": {
    "kb": { "type": "http", "url": "http://$KB_ADDR:8080/mcp" },
    "conduit": { "type": "stdio", "command": "conduit", "args": ["mcp"] }
  }
}
EOF
```

Then trust the room once — a workspace's settings don't apply until
its first interactive session accepts them:

```sh
cd ~/conduit-workspace && claude
# accept the folder-trust dialog and both MCP servers; /mcp should
# show kb and conduit connected — then exit
```

The kit's settings are the room's law, and worth reading
(`.claude/settings.json`): the eight conduit doors and the wiki *read*
tools are pre-allowed; there is no wiki write grant at all — a harness
writing a work-item page directly wouldn't be authoring, it'd be
forging. `signoff` isn't on the harness's door, by design. Shell
commands (git, the gate) prompt as they come; answer as you watch.

## Plan the work — the PM posture

Paste this into a workspace session (`cd ~/conduit-workspace &&
claude`):

```text
Plan work from the accepted decisions in this knowledge base — use the
plan-work skill. Keep this first trip deliberately small: one project
(one internal repo), one story, and one or two shovel-ready tasks
sized for a single session each. When the tree is drafted and
self-verified, present the goals at altitude and hand me the sign-off
list.
```

Watch the first `new` land: conduit registers its three schemas on
first contact, and every page enters through the same admission gates
as everything else. The session drafts top-down — project (a goal an
executive could confirm, `implements` the decision), story (behavior
as scenarios), tasks (each body carrying `## Goal` and `## Test set`,
including the *deliberate gaps*) — self-verifies against the KB, and
hands you a sign-off list it cannot execute. Everything sits `draft`
and unsealed until you act.

## Sign off — this part is you

Sign-off is a terminal seat, and it flows downhill: project before
story before tasks. Your signature records who you are — set your git
identity first if this machine doesn't have one
(`git config --global user.email you@example.com`).

```sh
cd ~/conduit-workspace
conduit list                    # the tree, with seal states
conduit show <item>             # read what you are about to sign
conduit signoff <item>          # seal it; the item goes ready
```

`<item>` is any unique fragment of the slug `list` shows —
`conduit show review-standard` beats typing sixty characters. An
ambiguous fragment errors with the candidates.

What you sign is the **body** — the goal, the scenarios, the test
set. The seal pins its exact bytes: any edit afterwards breaks it, and
every door bounces a broken item back to draft instead of acting on
it. A wrong contract doesn't get edited around — disagree with a
draft, `conduit bounce <item>`, tell the session why, and it
redrafts for a fresh signature.

## Execute — the execution posture

One session, one hat. Open a fresh workspace session for each signed
task and paste:

```text
A task is signed and ready — use the execute-task skill. Take it to a
pushed branch and stop there: report what you built and hold before
complete, because the review gate runs out-of-band first.
```

The session claims the task (the door verifies the seal and provisions
the project's internal repo and branch), clones into a scratch
directory, writes the signed test set as failing tests *first*, then
implements until the project's gate is green and pushes the branch.
If it finds the contract wrong mid-work, the right move is a `bounce`
with the finding — never a quiet extra test, never code the contract
doesn't cover.

## Review — the standing gate

The gate for what tests can't measure runs in a **separate session** —
one that did not implement the work. A session reviewing its own diff
is a mirror, not a gate. Fresh session, paste:

```text
A task's branch is pushed and awaiting review — use the review-task
skill and report your verdict.
```

The reviewer reads the signed contract and the actual diff through
five standing lenses — contract fidelity, test honesty, security,
dependency hygiene, conventions — and reports a structured verdict:
*ready to complete*, *fix first*, or *bounce*. It never fixes what it
finds and never touches the merge door; you weigh the verdict, send
findings back to the executing session if needed, and when it's ready
tell the executor to knock:

```text
The review came back ready — complete the task.
```

The merge door does the rest mechanically: seal re-verified, gate run
on the branch by the door itself, one squash commit onto `main`, and
the page updated with `merge_commit` and `work_ms`. If the door
refuses, its reason is information — fix and knock again. Repeat
execute → review → complete for each remaining task; when every task
under the story is terminal, the session closes the story through its
own door.

## Close the project — the human bookend

The other end of intent: sign-off opened the work, and confirming the
outcome closes it. The harness's door refuses this one — it's yours:

```sh
conduit show <project>          # the goal you signed; is it true now?
conduit close <project>
```

## Verify

```sh
cd ~/conduit-workspace
conduit list
git -C .conduit/repos/<project-stem>.git log --oneline main
incus exec kb -- su - kb -c 'llm-wiki schema list --wiki myproject'
```

The tree reads `done`/`closed` end to end; the internal repo's `main`
carries one squash commit per landed task, each with a `work-item:`
trailer naming the page it realized; and `project`, `story`, `task`
sit in your wiki's vocabulary carrying `x-owner: conduit`. Then ask
your workspace session:

```text
What work landed for our decisions, and what did it cost? Cite pages.
```

It answers from the work-item pages — the same pages Step 6 reads
when it prices the decision. The loop moves on: Measure.

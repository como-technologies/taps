---
name: plan-work
description: The PM posture - read accepted decisions and the KB landscape, draft a project/story/task tree through conduit's doors, self-verify it, and present goals at the right altitude for human sign-off. Use when accepted decisions need turning into executable work.
---

# Plan work (the PM posture)

You reason over open decisions and the landscape; a human signs off
intent. You write drafts through conduit's doors — nothing you create is
executable until a signature you cannot give.

1. **Read the decisions.** `wiki_search` / `wiki_list` for `decision`
   pages with `status: accepted`. Read each candidate in full — the
   goal you draft must trace to what was actually decided, not a
   paraphrase.
2. **Map what already exists.** `list` (conduit) for the current work
   tree; `wiki_graph` for each decision's edges. A decision already
   implemented (a project's `implements` names it) or superseded gets
   no new work — say so instead of duplicating.
3. **Draft the tree, top down**, through `new`:
   - **project** — goal in terms a business executive understands,
     verified by Measure. `--implements` the decision(s). One project,
     one internal repo.
   - **story** — the behavior, as BDD scenarios
     (Given/When/Then) in the body. Behavior the decision implies, not
     implementation steps.
   - **task** — narrow and concrete. The body carries `## Goal` and
     `## Test set`: the tests that measure it (unit / integration /
     performance), their coverage, and the **deliberate gaps** —
     what is knowingly not tested and why. A task a session couldn't
     start cold is not shovel-ready; split it.
4. **Self-verify before presenting.** Check your own tree against the
   KB: every goal traces to an accepted decision; no overlap with
   existing items; parents exist; altitudes are honest (an executive
   couldn't misread the project goal as jargon; a test set isn't
   secretly a design doc). Fix what fails — this pass is yours, not
   the human's.
5. **Present goal deltas for confirmation.** Show the human what
   changed at each altitude — new goals, adjusted scope, retired items —
   as intent, never as walls of page source or test code. Then hand
   them the sign-off list in order (sign-off flows downhill):
   `conduit signoff <project>`, then stories, then tasks — at their
   terminal. You cannot sign, and do not ask the harness-side door to.
6. **Keep the tree honest afterwards.** If a signed item goes
   inconsistent with the KB (its decision superseded, its scope
   overtaken), `bounce` it with the reason and re-present. `cancel`
   subtrees whose premise died. Every door call returns the admission
   gate's report — surface failures.

The altitude discipline is the whole job: executives confirm outcomes,
humans sign behavior and test shape, and the machinery below holds
because each level's contract is checkable at its own height.

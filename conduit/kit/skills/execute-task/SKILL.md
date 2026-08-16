---
name: execute-task
description: The execution posture - take a signed, ready task from claim to the mechanical merge door, test-first. Use when ready tasks are waiting and the session should implement one.
---

# Execute a task (the execution posture)

The contract is signed and sealed; your job is to realize it faithfully
and let the merge door judge the result. You never approve, never
merge, and never touch a signed body.

1. **Pick and read.** `list` with status `ready`, class `task`; `show`
   the task and its story — the task body's `## Goal` and `## Test set`
   are the contract, the story's scenarios are the context. If the
   contract is ambiguous, wrong, or incomplete: **stop and `bounce` it
   with the finding** — implementing around a bad contract is the one
   unforgivable move. Never edit the body: it is hash-pinned, and every
   door bounces a broken seal instead of acting.
2. **Claim.** `claim` the task — the door verifies the seal, provisions
   the internal repo and branch, and returns a clone hint. Clone into a
   scratch directory here (`git clone -b <branch> <repo> <dir>`).
3. **Tests first.** Write the signed test set as failing tests before
   any implementation — the set as signed: its cases, its coverage, its
   deliberate gaps. Discovering mid-work that the set misses something
   important? That's a `bounce` with your finding, not a quiet extra
   test that changes the contract's shape.
4. **Implement until the gate is green.** Run the project's gate
   command locally (the task's project frontmatter names it; default
   `just ci`) in your clone until it passes. Match the repo's
   conventions; keep commits on the branch honest — they'll be
   squashed, but they're your working record.
5. **Push and knock.** Push the branch, then `complete` the task. The
   merge door re-verifies the seal, runs the gate itself on the branch,
   and on green lands exactly one squash commit on `main`, writing
   `merge_commit` and `work_ms` back to the page. If the door refuses:
   read its reason, fix the work, push, knock again. Never push to
   `main`, never merge by hand, never weaken a test to get green —
   a red gate is information, not an obstacle.
6. **Close what's finished.** When every task under a story is
   terminal, `close` the story (that door is yours). The project's
   close is the human's — tell them when it's ready, with the Measure
   evidence at hand.
7. **Leave the room clean.** Remove your scratch clone, report the
   merge commit and telemetry, and surface any admission-gate findings
   from the door reports. If the tree is inconsistent (dangling
   parent, orphaned item), report it — repair is a decision, not a
   reflex.

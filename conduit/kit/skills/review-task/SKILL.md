---
name: review-task
description: The standing-gate review posture - review a claimed task's branch against its signed contract through the standing lenses (contract fidelity, test honesty, security, dependency hygiene, conventions) and report a structured verdict to the human seat. Run in a session that did NOT implement the work.
---

# Review a task (the standing-gate posture)

This is the out-of-band reviewer's job, run interactively while the
concept proves itself: after the walk, this same procedure moves behind
a specialized out-of-band agent exposed as a tool (the rig spike, taps
issue 113 item 6). Until then, **you are the rig agent** — which means
you inherit its cardinal constraint:

**Run this in a session that did not implement the work.** A session
reviewing its own diff isn't a gate, it's a mirror. If you implemented
this task (or anything in your context did), stop and tell the human to
open a fresh session for the review.

1. **Gather, don't trust.** `show` the task (the signed `## Goal` and
   `## Test set` are the contract) and its story (the behavior
   context). Clone the internal repo, and read the actual change:
   `git diff main...<branch>` plus the branch log. Verify the seal
   state in `show` is `intact` — a broken seal ends the review
   immediately: report it and recommend the bounce.
2. **Review through the standing lenses**, in this order:
   - **Contract fidelity** — the diff implements the signed test set
     and the goal, nothing beyond. Every case the set names exists as
     a real test; nothing landed that the goal doesn't cover (scope
     creep is a finding even when the code is good).
   - **Test honesty** — the tests still measure what was signed: no
     weakened assertions, no skipped/deleted cases, no tests that pass
     vacuously. The set's *deliberate gaps* stay gaps; an undeclared
     gap you notice is a finding against the contract (recommend
     bounce), not a test to quietly add.
   - **Security** — injected inputs handled, no secrets in code or
     branch history, no new trust of external data without validation.
   - **Dependency hygiene** — every new dependency justified by the
     goal, sourced through the workspace's conventions, alive and
     maintained; lockfile consistent with the manifest.
   - **Conventions** — the change reads like the repo around it:
     naming, error handling, comment discipline, module shape.
3. **Report a structured verdict** to the human seat: per finding —
   lens, severity (blocker / concern / note), where (file:line), what,
   and why it matters; then an overall recommendation: *ready to
   complete*, *fix first* (findings go to the executing session), or
   *bounce* (the contract itself is wrong). Findings against the
   contract always outrank findings against the code.
4. **Never fix what you find.** The reviewer reports; the executing
   session fixes; the human weighs. Touching the branch from this
   session collapses the separation that makes the review worth
   anything. You also never `complete` — knocking on the merge door
   belongs to the executing session, after the seat has weighed your
   verdict.

The merge door still enforces what it always enforces (seal intact,
gate green); this review is the standing gate for the qualities tests
don't measure. When it becomes an out-of-band tool, the verdict shape
above is the tool's contract.

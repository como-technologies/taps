# Step 6 — Measure

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

Six months after a decision, someone asks what it cost and whether it
helped. This step is how you answer from records instead of memory. Two
instruments, one destination: the measurements land in the same knowledge
base as the decisions they price.

## Price the decisions — tuesday

[tuesday](../portfolio/loop/measure.html) prices each decision from the
work-item pages Step 5 landed. Nothing is asked of anyone: conduit's
doors already stamped everything the report needs — `work_ms` and
`merged_at` on each task the merge door landed, the friction counters
(`bounces`, `door_refusals`) on items whose gates pushed back, and the
sign-off and close records at every human seat. Attribution is the
graph: task → story → project → `implements` → decision, the edge your
PM session drew when it planned the work.

The report speaks two currencies, and neither is hours: **machine
time** (milliseconds of execution between claim and landing) and
**human gate actions** (sign-offs given, verdicts weighed, projects
closed, bounces issued). Agents do the work; your attention is the
scarce input — the report prices both, and never pretends one is the
other.

```sh
tuesday-report                    # prices the current month
tuesday-report --period 2026-08   # or a named one
```

tuesday reads the suite pair from Step 2 like every taps tool — no
flags, no forge, no filesystem access to the wiki. The month's
`measure-report` page lands beside the decisions it prices, through
the same admission gates as everything else, and it is deterministic:
the same pages produce byte-identical reports, so re-running converges
instead of churning history.

> 🚧 **Unverified.** The walk confirms the report against Step 5's
> real project: one decision priced, its machine time and gate actions
> matching what the doors recorded, unattributed and discarded work
> called out honestly.

## Read the room — pulse

Effort tells you what a decision cost; [pulse](../portfolio/loop/measure.html) tells
you how it landed. Anonymous by cryptography, not by policy: blind
signatures guarantee that no one — including the operator — can link who
answered to what they said.

> 🚧 **Unverified.** pulse today ships a deterministic dogfood report
> (`just dogfood` in `pulse/`) with simulated respondents; the walk
> establishes what a real small-team poll looks like from this tutorial's
> vantage and how its signal lands in the KB.

## The point

After this step your knowledge base holds the whole thread: the
assessment that found the gap, the decision that addressed it, the PR
that shipped it, the capacity it consumed, and the team's reaction. That
thread is what the next step queries.

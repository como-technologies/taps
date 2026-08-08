# Step 6 — Measure

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

Six months after a decision, someone asks what it cost and whether it
helped. This step is how you answer from records instead of memory. Two
instruments, one destination: the measurements land in the same knowledge
base as the decisions they price.

## Price the decision — tuesday

[tuesday](../portfolio/loop/measure.html) attributes team effort to decisions from
merged PRs: each PR carries an effort label (a relative 1–5 scale — team
capacity, never individual surveillance), and the report rolls that up
per ADR.

```sh
tuesday-report --kb ~/myproject-kb
```

The month's capacity report lands as a typed page beside the decisions it
prices, carrying its per-ADR attribution.

> 🚧 **Unverified.** The walk confirms the label convention on the
> Step 5 PRs (the shared contract crate defines it), how the binary is
> built and invoked from a fresh clone, and what the report page looks
> like for a one-decision month.

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

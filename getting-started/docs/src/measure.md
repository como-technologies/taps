# Step 6 — Measure

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
flags, no forge, no filesystem access to the wiki. Expect a briefing
shaped like this (these are Step 5's real numbers):

```text
Measure — myproject, 2026-08

ADR-0001  Adopt a written code review standard carried by the pull request template
  landed     1 project(s), 1 story(ies), 2 task(s)  (c6908be, ea6c661)
  machine    45m of execution between claim and the merge door
  attention  4 sign-off(s), 2 close(s), 0 bounce(s), 0 refused knock(s)

unattributed  none — every landed task traces to a decision
discarded     6 cancelled item(s), no execution time lost

full report: measures/2026-08 — a typed page beside the decisions it prices
```

Reading it: each block is one decision that had work land this month,
priced in the two currencies. **machine** sums the door-stamped
`work_ms` of every task the merge door landed — what execution cost.
**attention** counts what *you* spent at the gates: signatures given,
closes confirmed, bounces issued, knocks the door refused.
**unattributed** is work whose graph walk reaches no decision — real
work, honestly orphaned, never hidden. **discarded** is cancelled
items: planning paid for and set aside. The terminal shows the digest;
the `measure-report` page it names is the full record, landed through
the same admission gates as everything else — and deterministic: the
same pages produce byte-identical reports, so re-running converges
instead of churning history.

## Read the room — pulse

tuesday tells you what a decision cost.
[pulse](../portfolio/loop/measure.html) tells you how the team feels
about it. pulse is an anonymous poll. The anonymity is cryptographic,
not a policy: blind signatures guarantee that no one — not even the
operator — can link who answered to what they said.

A real poll needs a team. pulse hides any answer group with fewer than
`k` people, so one reader cannot produce a real number. This tutorial
runs pulse's built-in simulation instead: ten simulated respondents
answer a short retro survey through the full protocol, and the report
labels its data as simulated. A poll with real teammates is future
work.

Run it from your taps clone:

```sh
cd ~/taps/pulse
pulse-report
```

The command starts both pulse services in one process, runs ten
respondents through the protocol over HTTP, and lands the result in
your knowledge base as one typed `pulse-report` page. Like tuesday, it
finds the KB through the suite pair from Step 2 — no flags. Expect a
briefing shaped like this:

```text
Pulse — myproject, survey iteration-retro

   3.7  How confident are you that this iteration's changes improved the portfolio? (company, 10 responses)
   3.9  How well did the dogfood loop (prescribe, adopt, measure, assess) support the work this iteration? (company, 10 responses)
   2.7  How sustainable is the current iteration pace? (company, 10 responses)
   3.7  How much do you trust the artifacts the loop produced this iteration (assessments, seeded decisions, merged PRs)? (company, 10 responses)
   4.0  How much did this iteration's hardening (gates tested under attack, broader forge support) increase your confidence in the suite? (company, 10 responses)

flows   10 of 10 respondents completed the protocol
source  simulated respondents — synthetic demo data, not a real survey

full report: pulse/iteration-retro — a typed page beside the decisions it speaks to
```

Reading it: each line is one question with its average score (1 to 5)
and how many answered. **flows** confirms every respondent completed
the protocol. **source** is the honesty label — this data is
simulated, and the page says so too. A question whose group has fewer
than `k` unique respondents shows no score; the page marks it
suppressed.

The run uses a fixed seed, so it is reproducible: run `pulse-report`
again and the page's bytes do not change. To see the full record, ask
your knowledge base session to show `pulse/iteration-retro`.

## The point

After this step your knowledge base holds the whole thread: the
assessment that found the gap, the decision that addressed it, the work
that shipped it, what it cost in machine time and human attention, and
the team's reaction. That thread is what the next step queries.

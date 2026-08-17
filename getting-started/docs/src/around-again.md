# Around again

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

The loop closed the moment the measurements landed next to the
decision. This step proves it, then starts the next trip.

## Ask

There is no question-answering tool to learn. Your session is the
asking surface: it searches the wiki, reads the pages, and answers
with citations. That is the suite's standing decision — tools write
typed pages; the harness reads them.

Open your authoring workspace:

```sh
cd ~/kb-workspace
claude
```

Then ask, in your own words:

```text
What did our first decision cost, and why did we make it? Answer from
the myproject wiki only, and cite a page for every claim.
```

Expect the whole thread back: the assessment report that found the
gap, the decision that addressed it, the work items that shipped it,
tuesday's measure page with the machine time and human attention it
cost, and pulse's poll page with the team's reaction. Every claim
points at a page you can open — you trust the pages, not the model.

## Re-assess

Modernization is a cycle, not a project. Step 3's assessment runs
again — but this time it does not start from zero. Your decision
aimed at specific questions; the second assessment measures whether
their answers actually changed.

Bring the amaker stack up again if you stopped it (`cd
~/taps/assessments && just run` — clean room: `just run-exposed`).
Open your assessment at `:3001/assess/<project-id>`, the same address
as Step 3, and answer honestly — the questions your decision aimed at
are the ones to watch. Then publish, exactly as before: ask your
workspace session, or

```sh
cd ~/taps/assessments
amaker publish <project-id>
```

Re-publishing overwrites the report page, and nothing is lost: the
wiki keeps every page's history, and the pages that cited the first
report still quote what it said. Back in your session, close the
loop:

```text
Compare the new assessment report with the first one (the wiki's page
history has it). What moved, and does the change match what our
decision aimed for?
```

The delta between two assessments is the evidence of movement. The
next trip's Step 3 starts from there — with a corpus that already
holds the prior assessment, the decisions taken, and what they cost.

## Where to go next

- **The [portfolio book](../portfolio/introduction.html)'s loop chapters**
  ([Assess](../portfolio/loop/assess.html),
  [Prescribe](../portfolio/loop/prescribe.html), [Adopt](../portfolio/loop/adopt.html),
  [Measure](../portfolio/loop/measure.html)) — the *why* behind each step you just
  walked.
- **Each product's own book** — depth on the tool you'll use most:
  [one site, per-product paths](../).
- **[Services](../portfolio/services.html)** — when you want Como running the loop
  with you.

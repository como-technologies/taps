# Step 4 — Assess

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

The loop opens with evidence: a structured maturity assessment of your
project, not a gut feeling. [amaker](../portfolio/loop/assess.html) is the authoring
environment — you and an assistant build the assessment together, a
respondent fills it in, and the analysis exports in exactly the shape
Step 5 consumes.

## Stand up amaker locally

```sh
cd ~/taps/assessments
cp .env.example .env        # put your ANTHROPIC_API_KEY in it
just run                    # builds, starts author/assess/analyze, opens the browser
```

Three services come up — author (`:3000`), respond (`:3001`), analyze
(`:3002`) — writing to a local `./data` directory. Ctrl-C stops them all.

> The hosted amaker instances are gated to Como's own organization —
> as a new user you run it locally, which also keeps your material on
> your machine.

## Author your assessment

In the authoring UI, describe what you want to understand — your
project's testing maturity, its platform readiness, its delivery
practice — and co-create the assessment tree (domain → practice →
question) with the assistant: it drafts, you correct and steer. Publish
a version when it says what you mean.

> 🚧 **Unverified.** The walk will pin down the minimal authoring path —
> how small a useful first assessment can be, and how long it takes.

## Respond and analyze

Fill in the published assessment as the respondent (`:3001`), then open
the analysis (`:3002`): scorecard, gaps, roadmap, narrative.

## Export for the next stage

Export the assessment analysis — a schema-validated document, not a
deck. This file is the hand-off: Step 5 seeds your decision backlog from
it directly.

> 🚧 **Unverified.** Where the export lives in the UI, its default
> format, and the exact file the Prescribe step imports — the walk will
> confirm and this page will name them precisely.

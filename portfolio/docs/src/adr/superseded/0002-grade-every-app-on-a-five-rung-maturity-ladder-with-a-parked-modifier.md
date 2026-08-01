# ADR-0002: Grade every app on a five-rung maturity ladder with a parked modifier

> State: Superseded

## Status

Superseded by [ADR-0011](../accepted/0011-rewrite-the-book-as-a-short-reader-first-product-guide.md) — the book's editorial reset retired maturity grading from the book entirely; the book describes the portfolio aspirationally, and current per-tool reality lives in each tool's own repo.

## Stakeholders

Portfolio owner (assigns and publishes badges); sibling-app maintainers
(their repos are the evidence a badge is graded against); prospective client
readers (the badge is the book's honesty signal).

## Context and Problem Statement

The book hedged maturity adjective-by-adjective — "(in development)",
"(M0 protocol proof; production pilot parked)", "(spike in progress)" — with
no shared scale, so overstatement crept in wherever an adjective was missing.
Como's internal model was a three-stage ladder (Stage 1 dogfood-daily,
Stage 2 SME-usable, Stage 3 self-serve), but it has no rung for anything
below Stage 1 — a decided design, or a completed end-to-end spike — and no
vocabulary for an app whose development is intentionally frozen. Every app
section in the restructured book (ADR-0001) needs one consistent badge line.

## Decision Drivers

- One badge vocabulary shared between the book and Como's internal ladder,
  so external claims and internal truth cannot diverge.
- The ladder must distinguish "decided on paper", "proven once, end to end",
  and "exercised every iteration" — three states the portfolio actually
  contains today.
- Intentional freezes (pulse) need honest vocabulary that neither fakes
  activity nor erases the proof that exists.
- A badge must be assignable from observable evidence in the app's own repo,
  not from ambition.

## Considered Options

- A five-rung ladder — spec / spike / dogfooding / SME-usable / self-serve —
  plus a "(parked)" modifier.
- The existing three-stage ladder plus a single pre-stage "spec" rung.
- Free-form per-app status sentences (the status quo).

## Decision Outcome

Chosen: the **five-rung ladder with the parked modifier**, because the
portfolio today genuinely occupies three distinct rungs below SME-usable,
and a single "spec" pre-stage would force a completed, evidence-backed spike
and a paper design to share one badge.

The rungs:

| Badge | Meaning |
|---|---|
| **spec** | The design is decided and written down; no runnable end-to-end proof exists. |
| **spike** | A runnable end-to-end proof exists with captured evidence; not yet exercised every iteration. |
| **dogfooding** | Exercised on Como's own work every iteration; build from source. Equals internal Stage 1. |
| **SME-usable** | An external subject-matter expert can drive it with Como alongside. Equals internal Stage 2. |
| **self-serve** | Production-grade: a client team installs and runs it without Como in the room. Equals internal Stage 3. |

Modifier: **(parked)** — development is intentionally frozen by a recorded
decision while the dogfood proof is kept green.

Badges as of this decision, graded against each repo's captured evidence
(badges move only with evidence, and moves are recorded per iteration):

- assessments — **dogfooding**
- adroit — **dogfooding**
- the template content product (since retired by ADR-0010) — **dogfooding**
- conduit — **spike**
- tuesday — **dogfooding**
- pulse — **dogfooding (parked)**

Nothing in the portfolio is SME-usable or self-serve yet; the badge line
saying so plainly is the point.

### Positive Consequences

- The class of overstatement the accuracy pass fixed adjective-by-adjective
  is fixed wholesale: every app section opens with the same honest signal.
- Badge moves become per-iteration news the book can report.
- Book vocabulary and internal ladder vocabulary are the same words.

### Negative Consequences

- Five rungs plus a modifier need definitions a reader must learn (one
  table on the introduction).
- A badge can rot like any other claim — mitigated by the verify-claims
  gate (ADR-0003) and the per-iteration review.
- "dogfooding" spans a wide quality range (a hardened CLI and a zero-test
  web head can share the rung); the per-app evidence links carry the nuance
  the single word cannot.

## Implementation

The ladder table lands on the book's introduction; each app section in the
stage chapters (ADR-0001) opens with its badge line; assignments above are
reflected in the TAPS classification table.

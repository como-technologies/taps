# ADR-0003: Enforce book truthfulness with a scripted verify-claims gate

> State: Superseded

## Status

Superseded by [ADR-0011](../accepted/0011-rewrite-the-book-as-a-short-reader-first-product-guide.md) — the book's editorial reset removed the mechanically-verified claims the gate existed to police; seam enforcement lives in the owning repos' contract tests, and `just ci` is the book build plus `adr-check`.

## Stakeholders

Portfolio owner (runs the gate, fixes red claims); sibling-app maintainers
(their changes are what the gate detects); prospective client readers (the
gate is why the book's claims can be trusted).

## Context and Problem Statement

The book describes six sibling repos that all move, and its claims rot
silently when they do — it has already happened once: conduit went from
"(in development)" to a completed, evidence-captured spike between two
drafts of the same chapter. A docs-current working agreement only holds when
something fails on drift. The book needs its truthfulness to be mechanical,
not aspirational: a check that runs locally as part of `just ci` and goes
red when a sibling repo no longer matches what the book says.

## Decision Drivers

- Drift must fail a command, not wait for a human re-read.
- Checks must be read-only against the sibling checkouts and skip gracefully
  when a sibling is absent (fresh-checkout CI has no siblings).
- Checks must be pinned to stable anchors (contract constants, schema ids,
  `--help` output) — a flaky gate gets disabled, which kills the mechanism.
- Simplicity: the book is an mdBook; the gate should not require a custom
  preprocessor to exist.

## Considered Options

- A `scripts/verify-claims` script wired into `just ci` alongside the book
  build and `adr-check`.
- An mdbook preprocessor that asserts claims at build time.
- A periodic manual review checklist (the status quo, formalized).

## Decision Outcome

Chosen: the **script wired into `just ci`**, because a script is simple
enough to stay alive, runs the same way locally and in CI, and can read
sibling repos directly — a preprocessor couples the book build to checks
that should be able to fail independently, and a manual checklist is the
silent-rot status quo with extra steps.

The gate's checks, pinned to drift-resistant anchors:

- The book's conduit → tuesday contract table string-matches the constants
  in conduit's `src/contract.rs` (the closed `effort:*` label set, the
  `adr:` label prefix, the title prefix, the trailer, the branch shape).
- adroit's subcommands advertised by the book appear in `adroit --help`.
- tuesday's headless JSON report surface (`tuesday-report`, `--strict`,
  `adr_totals`) exists where the book says it does.
- pulse's Measure artifact schema id matches `pulse.measure-report/v1`.

Every check skips with an explicit "skip:" line when its sibling checkout is
missing, mirroring the `adr-check` recipe. A red `just ci` when a sibling
drifts is the feature, not a bug.

### Positive Consequences

- Claim drift becomes a local CI failure instead of a credibility failure.
- The gate documents, in executable form, exactly which claims the book
  stakes its credibility on.

### Negative Consequences

- The gate couples the book to pre-1.0 sibling CLIs; checks must stay
  tolerant and anchored or they will flake.
- Sibling maintainers inherit a duty: a change that breaks the gate lands
  with a portfolio fix.
- Skip-when-missing means a machine without sibling checkouts proves less;
  the full proof exists only on a full portfolio checkout.

## Implementation

Decided now, built later: this decision is accepted ahead of its
implementation, which lands as build-queue item 28 (`scripts/verify-claims`
plus the `just ci` wiring and a "How this book stays true" page). Until that
item lands, `just ci` is the book build plus `adr-check`, and this ADR is
the recorded commitment the implementation will be checked against.

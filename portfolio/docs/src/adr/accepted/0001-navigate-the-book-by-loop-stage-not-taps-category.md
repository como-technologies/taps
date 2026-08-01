# ADR-0001: Navigate the book by loop stage, not TAPS category

> State: Accepted

## Status

Accepted

## Stakeholders

Portfolio owner (authors and publishes the book); sibling-app maintainers
(each app's section must match its repo's reality); prospective client
readers (the executive audience the navigation serves).

## Context and Problem Statement

The book's navigation was TAPS-category-first: four one-page stubs (Tools,
Apps, Products, Services) with one to three sentences per app. A reader
tracing the Assess → Prescribe → Adopt → Measure loop — the actual product
narrative — bounces across four category pages, and the loop has no home.
Meanwhile the sibling apps have grown real, citable evidence (captured demo
runs, dogfood pages, machine-readable artifacts) that a category stub cannot
house. The book's stated direction is to be the loop's evidence ledger, not
a brochure; the navigation has to carry that.

## Decision Drivers

- The loop is the narrative spine, and the dogfood demo walks it in order;
  category-first navigation scatters it.
- Every app needs a standard skeleton — what it is, maturity badge, how it
  enters the loop, where its evidence lives — which is too much content for
  a shared category stub.
- TAPS classification still matters commercially and must stay visible.
- The services and products chapter content is owned by a separate work item
  and must survive this restructure untouched.

## Considered Options

- Loop-first chapters: one chapter per loop stage with per-app sections;
  TAPS becomes a classification table on the introduction.
- Keep category-first navigation and grow per-app subsections inside the
  existing Tools/Apps/Products/Services stubs.
- Hybrid: add a loop walkthrough chapter while keeping full category
  chapters, describing every app in both places.

## Decision Outcome

Chosen: **loop-first chapters**, because the loop is the product narrative
and a single home per app keeps the book truthful — the hybrid would say
everything twice and drift twice.

Concretely: `SUMMARY.md` gains a "The Loop" part with one chapter per stage —
Assess (assessments), Prescribe (adroit + the knowledge base), Adopt (conduit, plus
where services wrap it), Measure (pulse + tuesday). Each app is rendered with
the standard skeleton: what it is / maturity badge / how it enters the loop
(its produce and consume seams, with exact commands) / where its evidence
lives (links into its own book and repo paths). The introduction keeps TAPS
as a compact classification table. The old `tools/` and `apps/` stubs fold
into the stage chapters and are deleted; `services/` and `products/` remain
first-class chapters with only their navigation links fixed.

### Positive Consequences

- A reader walks the loop in order, with each stage's tooling and evidence
  in one place.
- Per-app sections give maturity badges and seam commands a stable home.
- One description per app — no duplicated copy to keep consistent.

### Negative Consequences

- Published URLs for the deleted `tools/` and `apps/` pages break; accepted
  while the book is young rather than carrying redirect stubs.
- Two classification axes (stage chapters and the TAPS table) must be kept
  consistent by hand.
- Per-app sections are a larger truth surface to maintain — mitigated by the
  verify-claims gate (ADR-0003).

## Implementation

Carried out in the same change set that records this decision: new
`src/SUMMARY.md`, four stage chapters under `src/loop/`, the TAPS table on
the introduction, deletion of the `tools/` and `apps/` stubs, and link fixes
in the surviving chapters.

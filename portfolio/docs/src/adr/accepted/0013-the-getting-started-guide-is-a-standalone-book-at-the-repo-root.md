# ADR-0013: The Getting Started guide is a standalone book at the repo root

> State: Accepted

## Status

Accepted

## Stakeholders

Suite owner (decides how the suite meets new users); portfolio owner (the
book's charter is at stake); product maintainers (their READMEs and books
join the funnel); new users (the guide's audience).

## Context and Problem Statement

The portfolio book convinces a reader the loop is worth trying — and then
leaves them hanging. There is no step-by-step path from a fresh machine
to a first trip around the loop: which binaries to build, how to stand up
a knowledge base, how an assessment becomes decisions becomes a PR
becomes a measurement. OPERATIONS.md covers standing the suite up, but it
is Como-facing operational detail, not a tutorial. Meanwhile ADR-0011
deliberately made the portfolio book short and reader-first; growing a
multi-page tutorial inside it would strain that charter in exactly the
way ADR-0011 was written to prevent.

## Decision Drivers

- New users need a walkable, step-numbered path — get the software,
  create the KB, assess, prescribe, adopt, measure — with a clear
  done-state per step.
- The portfolio book's charter (ADR-0011) is the story, kept short; the
  tutorial's natural growth must not erode it.
- The guide must be discoverable from every door a new user enters by:
  the repo README, the published site, the portfolio book, each
  product's README.
- The guide is being dogfooded into existence: we follow it ourselves
  and fix what breaks, so its honesty markers and iteration cadence are
  part of its design.

## Considered Options

- A **"Getting started" section inside the portfolio book** — one front
  door, but the book stops being short.
- A **standalone Getting Started book** at the repo root, published on
  the same Pages site, linked from everywhere.
- A **portfolio itinerary page linking into each product's book** — honors
  "one home per fact", but shreds the walkthrough thread across six books.

## Decision Outcome

Chosen: the **standalone book**, at `getting-started/` in the repo root —
completely independent of the portfolio book.

- **Layout follows the book convention**: `getting-started/docs/`
  (`book.toml` + `src/`), gruvbox theme, its own `justfile` (`book`,
  `book-serve`, `ci`). No crate — it is docs only.
- **Shape**: an overview plus one page per loop step, ending with the
  loop closing ("Around again"). Steps not yet verified by a walkthrough
  carry visible 🚧 markers — published honestly, removed as walks
  complete.
- **Published with the other books**: the root `just site` recipe is the
  one definition of the site layout (every book under one root,
  per-book paths); `pages.yml` publishes exactly its output, and
  `just books-serve` serves the same assembly locally on one port.
  Cross-book links are sibling-relative (`../portfolio/…`), so they
  resolve identically locally and on Pages.
- **The funnel**: the repo README links the guide prominently; each
  product README carries a one-line pointer; the portfolio book's
  introduction closes with "Ready to try it?" → the guide. The portfolio
  book itself stays untouched otherwise — its charter holds.

### Positive Consequences

- The portfolio book stays the ten-minute story ADR-0011 made it; the
  tutorial can grow as much as walking the loop actually requires.
- One walkthrough thread in one place, with one voice, instead of six
  books each owning a fragment.
- The site layout is defined once (`just site`) — CI and the local
  preview cannot drift.

### Negative Consequences

- A seventh book to build and publish — accepted: it is one more
  `mdbook build` in the existing recipes.
- Cross-book sibling-relative links assume the books stay siblings on
  one site — accepted and now load-bearing; moving a book means fixing
  the guide's links.
- Duplicated orientation prose between the guide's overview and the
  portfolio introduction can drift — mitigated by keeping the overview
  thin and linking to the portfolio for the why.

## Implementation

Landed with this decision: the `getting-started/` book (overview + seven
step pages moved from the portfolio draft), the root `just site` and
`books-serve` recipes with `pages.yml` publishing `just site`'s output,
the `books` recipe's seventh entry, the README funnel links (root +
seven products), and the portfolio introduction's closing pointer.

# ADR-0011: Rewrite the book as a short, reader-first product guide

> State: Accepted

## Status

Accepted

## Stakeholders

Portfolio owner (maintains the book); sibling-app maintainers (their repos
now carry all technical detail); prospective client readers (the book's
actual audience, finally).

## Context and Problem Statement

The book began as a product guide: tell the story of the tools and how
they work together, in a way people would want to read. Over successive
AI-assisted iterations it accreted governance apparatus — a five-rung
maturity ladder with per-badge gating evidence (ADR-0002), a 1,300-line
scripted truthfulness gate pinning dozens of claims against six sibling
repos (ADR-0003), an operations runbook, a full KB specification, spike
evidence pages, and CLI transcripts on every chapter. The result: a book
too long to read, too technical to follow, and too expensive to maintain —
every sibling change rippled into book edits and gate updates. The
apparatus built to keep the book honest had made the book unreadable,
which is its own kind of dishonesty.

## Decision Drivers

- The book's job is the story: what the tools are, how they work
  together, why a reader should care. Every sentence must earn its place
  with that reader.
- Each tool has (or should have) docs of its own; that is the place for
  technical detail, evidence, and contracts. One home per fact.
- Maintenance must scale with the suite: adding a tool should add a short
  chapter, not a new family of verified claims.
- Claims that genuinely need mechanical enforcement belong in the repos
  that own them (contract tests on each side of a seam), not in a
  suite-wide prose-checking script.

## Considered Options

- Keep the apparatus and automate it harder (single-source the claims via
  mdBook includes, move checks into sibling tests, keep the gate).
- **A radical editorial reset**: rewrite the book as a short product
  guide, retire the ladder and the truthfulness gate, move specs and ops
  out of the book, link to sibling docs for detail.
- Retire the book entirely and let the repos speak for themselves.

## Decision Outcome

Chosen: the **radical editorial reset**. The book becomes ~10 short
pages: an introduction (the problem, the loop, the portfolio at a
glance), one page per loop stage, one page each for the knowledge base
and starter content, and one services page. Concretely:

- **No maturity grading in the book at all** (supersedes ADR-0002). The
  book describes the portfolio aspirationally — what each offering is
  and how the loop fits together — with no badges or status words.
  Current per-tool reality, and the evidence for it, lives in each
  tool's own repo.
- **No scripted truthfulness gate** (supersedes ADR-0003).
  `scripts/verify-claims` is deleted; `just ci` is the book build plus
  `adr-check`. The book avoids restating mechanical facts (CLI shapes,
  label sets, counts, contract tables); where a seam needs enforcement,
  the owning repos' contract tests carry it.
- **Specs and ops leave the book.** The Como KB specification moves to
  the llm-wiki repo (`docs/specifications/como-kb-spec.md`) — it is the
  KB product's contract and belongs with the product. The operating
  runbook moves to `OPERATIONS.md` in this repo, outside the book.
- **The changelog and spike-evidence pages are deleted**; git history is
  the ledger.
- Chapters link to each tool's repo for details instead of quoting its
  CLI, evidence pages, or ADRs.

ADR-0001 (navigate by loop stage) and ADR-0004 (the uniform resolution
convention, still used by `adr-check` and `cold-sim`) stand.

### Positive Consequences

- A book a prospective client can read end to end in ten minutes, in one
  consistent voice.
- Maintenance drops to editing prose when the story changes — no gate to
  feed, no badges to re-grade, no pinned counts to true up.
- One home per fact: technical detail lives with the code that makes it
  true, where it is maintained and tested anyway.
- The suite's last Python script in this repo's CI path is gone.

### Negative Consequences

- Claims can drift silently; nothing mechanical notices the book
  overstating a sibling. Mitigated by making far fewer claims and
  linking out for anything volatile.
- Readers who want evidence must follow a link instead of reading it
  inline — accepted: those readers are a minority, and the links are
  there.

## Implementation

Landed with this decision: the rewritten `docs/src/` (SUMMARY,
introduction, four loop pages, knowledge-base, starter-content,
services), `OPERATIONS.md`, the kb-spec move into llm-wiki, the deletion
of `scripts/verify-claims` and the retired pages, the `just ci` and
`scripts/cold-sim` updates, and the README refresh.

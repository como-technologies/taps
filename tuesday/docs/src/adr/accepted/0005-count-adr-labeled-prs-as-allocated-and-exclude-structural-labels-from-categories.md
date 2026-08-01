# ADR-0005: Count ADR-labeled PRs as allocated and exclude structural labels from categories

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner (referee ruling); tuesday maintainers;
conduit maintainers (the producer side of the contract).

## Context and Problem Statement

conduit's dogfood forge pre-creates only `effort:*` and `conduit:*` labels —
no category labels exist there, and conduit PRs carry exactly one effort
label plus `adr:<reference>`. Two of tuesday's own rules collided: excluding
structural labels (`adr:*`, `conduit:*`) from category allocation would leave
every conduit PR with zero category labels, dumping it in `unallocated_prs`
and making strict mode exit nonzero — directly contradicting the dogfood pass
criterion "`unallocated_prs` is empty (so `--strict` exits 0)". On current
`main` the opposite failure exists: every non-effort label counts as a
category, so `adr:ADR-NNNN` would pollute category totals. The portfolio
referee resolved the contradiction; this ADR records the ruling.

## Decision Drivers

- The dogfood pass criterion and the structural-label exclusion must both
  hold simultaneously.
- ADR attribution answers "what did this decision cost?" — splitting its
  credit would understate every decision's cost.
- The category breakdown must stay internally consistent (every allocated
  hour appears in some category).
- Strict mode must be a meaningful contract check, not a tautology.

## Considered Options

- **ADR attribution counts as allocation (the referee ruling)**: strict mode
  passes a merged PR with exactly one `effort:N-*` label AND (at least one
  category label OR an `adr:*` label). `adr:*` / `conduit:*` are excluded
  from category totals; a PR with no category labels falls back to
  `Uncategorized` in the category view; the ADR is credited the PR's FULL
  allocated hours (no splitting).
- **Require category labels on every PR** — forces conduit to invent fake
  categories or forces the forge bootstrap to grow a category taxonomy
  nobody asked for; the dogfood would test label ceremony, not measurement.
- **Let `adr:*` count as a category** (main's accidental behavior) — keeps
  strict mode green but corrupts the category breakdown with ADR ids and
  makes the same hours mean different things in different report sections.

## Decision Outcome

Chosen: **PRs carrying an `adr:*` label count as allocated even with no
category label**, because ADR attribution is itself an allocation of capacity
to a decision. Concretely: strict rule = exactly one effort label AND
(category label OR `adr:*` label); `adr:*` and `conduit:*` prefixes are
attribution/machinery, excluded from category totals; categories default to
`Uncategorized` when only structural labels are present; `adr_totals` credits
the ADR with the PR's full allocated hours. The ADR rollup is a first-class
report section — the Measure→Prescribe feedback artifact.

### Positive Consequences

- Both halves of the contract hold: strict dogfood runs exit 0 on
  contract-conforming PRs, and category totals stay free of machinery labels.
- "Hours per decision" is honest: a decision's cost is never diluted by
  category arithmetic.
- conduit's label bootstrap stays minimal (effort + machinery only).

### Negative Consequences

- The category view of a conduit-only month is all `Uncategorized` — the
  category breakdown carries no signal there; the ADR rollup is the view
  that matters, and readers must learn that.
- Post-merge label edits can still make `conduit verify` and tuesday
  disagree (label final at merge vs read at report time) — accepted and
  documented as a contract caveat rather than engineered around.
- The strict rule now encodes portfolio policy inside tuesday's calculator;
  changing the ruling requires a superseding ADR on both sides.

## Implementation

The `adr:*` category exclusion and full-credit `adr_totals` are implemented
on the in-review `adr-attribution` branch; the `conduit:*` exclusion and the
strict-mode rule land with milestones M2 (exclusion tests) and M4
(`--strict`). The ruling is documented on the book's Dogfood Contract page
and in the portfolio book's contract table.

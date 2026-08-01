# ADR-0007: Emit multi-month ranges as an additive envelope of unchanged per-month MonthlyReports

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner; tuesday maintainers; downstream consumers
of the Measure artifact (the portfolio book's evidence chapter, cross-check
scripts, the next Assess iteration); SMEs catching up past months or
reading quarterly views.

## Context and Problem Statement

`tuesday-report` takes exactly one `--year/--month` window. The real SME
need the iteration-2 direction names — catching up past months and
quarterly views — requires a range, and the portfolio referee's graded
matrix carries it as an open FAIL cell ("`--from/--to` multi-month"). The
question forcing a decision is the **output shape**: ADR-0004 made the
single serde `MonthlyReport` a public contract with a byte-pinned canonical
serialization, and the web export head serializes the same struct. A range
output that invents a second report schema, or that mutates the per-month
shape, forks that contract — the exact risk ADR-0004 was recorded to
prevent. The shape also has to carry the cross-month view a range exists
for: what each decision (ADR) cost over the whole window, not just inside
one month.

## Decision Drivers

- ADR-0004's single canonical `MonthlyReport` stays the only report schema;
  single-month output must not change by a byte.
- A range exists to answer cross-month questions: the per-decision
  (`adr_totals`) rollup over the whole window must be first-class, not a
  consumer-side second pass.
- `--strict` (ADR-0005) is a per-month contract — each calendar month's PRs
  must satisfy it within that month.
- The month budget (`--monthly-hours`) is per month by definition; a range
  must not smear one budget across N months.
- Window math must be explicit and tested across year boundaries (the
  month-boundary trap, one year up).
- Machine consumers need to find the per-month reports and the window
  echoed back without re-deriving them.

## Considered Options

- **An additive envelope around unchanged per-month reports**:
  `{ "from": "YYYY-MM", "to": "YYYY-MM", "reports": [MonthlyReport…],
  "adr_totals": {…} }` — one **unchanged** canonical `MonthlyReport` per
  month of the inclusive range, plus the derived cross-month ADR rollup.
- **A bare JSON array of MonthlyReports** — the minimal reading of "an
  array of per-month reports", but an array cannot carry the cross-month
  `adr_totals` rollup or echo the window; every consumer re-implements the
  summing pass the range exists to provide.
- **One merged multi-month MonthlyReport** (sum the window into a single
  report) — destroys the per-month budget semantics, hides which month a PR
  landed in, and changes the meaning of every field; a second schema in all
  but name.
- **A new purpose-built range schema** with restructured per-month entries
  — maximal ergonomics, guaranteed divergence from ADR-0004's contract.

## Decision Outcome

Chosen: **the additive envelope**, because it adds the two things a range
needs (the echoed window and the cross-month `adr_totals` rollup) while
keeping ADR-0004's contract intact — each element of `reports` is
bit-for-bit what single-month `-o json` emits for that month, which is
test-enforced. Semantics fixed by this ADR:

- `--from YYYY-MM --to YYYY-MM`, both inclusive; the range may cross year
  boundaries; an inverted range is an error, never an empty report.
- Single-month mode (`--year/--month`) is untouched: it still emits the
  bare canonical `MonthlyReport`. The envelope appears only in range mode.
- One report per month of the range; an empty month yields an empty report,
  not a hole (a quarter includes its quiet months).
- `monthly_hours` is the per-month budget, applied to each month
  independently.
- `--strict` is checked month by month; any month's violation makes the
  exit code nonzero, and the envelope is still emitted for inspection.
- `adr_totals` in the envelope is the derived sum of the per-month
  `adr_totals` (full credit per decision, ADR-0005), carrying no new
  semantics.
- The table form mirrors the envelope: one sectioned per-month table plus
  the cross-range ADR rollup.
- The web export endpoint is **not** range-aware: it keeps serializing the
  single-month `MonthlyReport` unchanged (its parity with the CLI is
  byte-pinned by test). A future range need on the web head consumes this
  same envelope rather than inventing another shape.

### Positive Consequences

- Catching up past months and quarterly views are one command; the
  year-boundary window math is unit-tested instead of hand-assembled.
- Cross-month decision cost is first-class: consumers read
  `envelope.adr_totals` instead of re-summing.
- Per-month compatibility is mechanical: any consumer of the single-month
  schema can consume `reports[i]` unchanged.
- The envelope is additive — new range-level fields can be added later
  without touching the per-month contract.

### Negative Consequences

- Range consumers must unwrap an envelope rather than index a bare array —
  one level of indirection the minimal reading would not have had.
- The envelope itself becomes a second public *container* contract
  (`from`/`to`/`reports`/`adr_totals`): renaming its keys is now a breaking
  change for script consumers.
- A long range issues one fetch pass per month per repository — N-month
  catch-up costs N times the forge calls; acceptable at SME scale, no
  caching is built.

## Implementation

Implemented in the same change that records this ADR: `--from/--to` on
`tuesday-report` (clap-enforced pairing and conflict with `--year/--month`),
`window.rs` month enumeration with year-boundary unit tests, the per-month
pipeline in `run_range_with_source`, the envelope/sectioned-table renderers,
and an e2e stub-forge test crossing the 2025→2026 boundary with a per-month
strict violation. The book's CLI page is regenerated from the real
`--help`; the quickstart documents the range command verified against a
live forge.

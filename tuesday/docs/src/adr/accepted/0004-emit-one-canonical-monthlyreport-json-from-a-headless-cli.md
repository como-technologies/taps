# ADR-0004: Emit one canonical MonthlyReport JSON from a headless CLI

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner; tuesday maintainers; downstream consumers
of the Measure artifact (the portfolio book's evidence chapter, the next
Assess iteration).

## Context and Problem Statement

On `main`, tuesday's report exists only inside a browser session — nothing
downstream can consume it. The in-review `adr-attribution` branch adds a JSON
export endpoint (`POST /api/export_report`) serializing the serde
`MonthlyReport`. The dogfood loop needs the same report headlessly, from a
CLI, with a machine-checkable pass/fail. The risk named by the portfolio
referee is schema divergence: if the CLI invents its own output shape, the
loop gets two competing "canonical" reports.

## Decision Drivers

- The Measure artifact must be consumable by scripts and other portfolio
  apps (plain JSON file, zero AI).
- One schema: the export endpoint and the CLI must serialize the SAME serde
  `MonthlyReport` — converge, do not fork.
- The dogfood proof needs a nonzero exit signal when the month's PRs violate
  the contract (`--strict`).
- The CLI must run offline against conduit's local Gitea forge with a token
  file (gitignored secrets, pinned filenames).

## Considered Options

- **Headless CLI in `tuesday-cli` emitting the existing `MonthlyReport`**:
  `tuesday report --forge gitea|github --url <base> --owner <o> --repo <r>...
  --year Y --month M --monthly-hours H --scaling <series> --token-file <path>
  -o json|md`, plus `--strict` (nonzero exit if `unallocated_prs` is
  nonempty under the allocation ruling of ADR-0005).
- **A new purpose-built CLI schema** — tempting for CLI ergonomics, but it
  forks the canonical report and guarantees divergence from the web export.
- **Script the export endpoint with curl** (no CLI) — works today, but
  requires a running server, a configured browser-shaped `ReportConfig`
  including a GitHub token field, and gives no strict exit semantics.

## Decision Outcome

Chosen: **a headless `tuesday report` CLI that serializes the same serde
`MonthlyReport` as the export endpoint**, because one struct rendered by
every head (web UI, HTTP export, CLI) makes schema drift a compile error
rather than an integration surprise. `-o json` is the canonical
machine-readable form (including `adr_totals`, per-allocation `adr_id`, and
the `unallocated_prs` QC list); `-o md` is a human convenience over the same
data. `--strict` turns the QC list into an exit code, making the dogfood
pass criterion scriptable.

### Positive Consequences

- The Measure beat of the dogfood demo becomes one command with a meaningful
  exit code.
- Downstream consumers (portfolio book evidence, cross-check scripts,
  iteration-2 Assess) read one documented schema.
- CLI and web UI cannot disagree about what a report contains.

### Negative Consequences

- The CLI must wait for the `adr-attribution` branch to merge and then
  converge on its `MonthlyReport` — building first would fork the schema.
- `MonthlyReport` becomes a public contract: future field changes are
  breaking changes for script consumers, not internal refactors.
- A second head doubles the places ingestion configuration is parsed
  (UI settings vs CLI flags).

## Implementation

Milestone M4: `crates/tuesday-cli` with the flag surface above; JSON output
is the serde `MonthlyReport` including the ADR rollup; document the exact
verified command in the book and add a `just report` recipe. M5 wires
`just dogfood-report` against conduit's demo forge and a cross-check script
asserting tuesday's JSON and `conduit verify <task> -o json` agree on PR
number, effort label, and ADR reference.

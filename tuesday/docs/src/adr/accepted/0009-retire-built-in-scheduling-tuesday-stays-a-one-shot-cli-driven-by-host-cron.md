# ADR-0009: Retire built-in scheduling: tuesday stays a one-shot CLI driven by host cron

> State: Accepted

## Status

Accepted

## Stakeholders

tuesday maintainers; SMEs running recurring capacity reports; the portfolio
owner (whose suite mandate fixes tuesday's target rung at SME-usable).

## Context and Problem Statement

A measurement tool invites the next feature request: run itself every
month. The iteration-2 direction lists a "built-in scheduler /
scheduled-report daemon" among the things to retire by ADR rather than
leave ambient. The question is whether tuesday should ever grow a resident
scheduling component (a daemon, a `--watch` loop, a hosted cron), or stay a
deterministic one-shot CLI whose recurrence belongs to the host. Settling
it now keeps the suite-done line crisp: with deployment retired (ADR-0008),
a built-in scheduler would be the only long-running process left in the
design — and it would exist solely to re-implement what every host already
provides.

## Decision Drivers

- Determinism: the Measure artifact must be reproducible — same window,
  same forge state, same bytes; a resident process adds state and drift.
- The real SME need behind "scheduling" is catching up past months and
  quarterly views — met by the `--from/--to` range (ADR-0007), which
  back-fills any missed window on demand.
- Local-first, owner-only publishing: nothing in the portfolio operates
  long-running hosted services (ADR-0008).
- Hosts already own recurrence: cron, CI schedules, systemd timers — all
  better tested than anything tuesday would build.
- A documented wrapper must exist so "use cron" is a recipe, not a shrug.

## Considered Options

- **One-shot CLI + host scheduling, with a documented example wrapper**:
  no scheduler code; the book shows the exact cron line wrapping
  `tuesday-report` (and the `just dogfood-report` recipe as the wrapper
  pattern).
- **A built-in daemon / `--watch` mode**: convenient in demos, but it
  imports clock handling, missed-run semantics, overlap locking, and log
  rotation — an ops surface tuesday's rung never asked for.
- **A hosted scheduled service** (e.g. the retired Cloud Run + Cloud
  Scheduler pairing): violates local-first and owner-only publishing
  outright; retired with ADR-0008.

## Decision Outcome

Chosen: **one-shot CLI + host scheduling with a documented example
wrapper**, because recurrence is a solved host problem and every driver
points the same way: `tuesday-report` stays a process that starts, fetches,
calculates, prints, and exits — with a meaningful exit code (`--strict`)
that schedulers and CI can act on. The missed-month worry is answered by
the range, not by a daemon: a quarter of missed runs is one
`--from/--to` command.

The book's "Running tuesday" page carries the normative example (a cron
line invoking `tuesday-report` for the previous month, token from a file),
so the decision lands as a copyable recipe.

### Positive Consequences

- tuesday keeps zero resident state: every run is auditable and
  reproducible from its command line.
- No overlap/missed-run/locking semantics to design, test, or document.
- Schedulers get what they are good at: a binary with honest exit codes.

### Negative Consequences

- Recurrence setup is the SME's job — tuesday's docs can show the recipe
  but cannot make it turnkey across every host.
- "Last day of month at 23:59" style windows are at the mercy of the host
  scheduler's semantics, not tuesday's.
- Each scheduled run authenticates afresh from the token file; rotating
  tokens is likewise the host's concern.

## Implementation

No code to remove — the scheduler was never built; this ADR closes the
option. The "Running tuesday" book page documents the cron example and
points at `just dogfood-report` as the wrapper pattern; the quickstart
names the `--from/--to` range (ADR-0007) as the catch-up path.

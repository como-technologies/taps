# Dogfood Contract — the Measure stage

tuesday is the **Measure-stage consumer** of the TAPS loop and the
**independent second verifier** of conduit's emission contract. conduit
(Adopt) turns accepted ADRs into merged PRs and tags every one of them with a
fixed set of markers; tuesday reads those markers at report time and turns
them into hours attributed back to the originating decision. Two independent
codebases agreeing on the same merged PR's labels, title, and trailer is the
loop-closure evidence.

The normative statement of the shared markers is `como-contract`
(`contract/src/tuesday.rs`), which both sides implement. This page is the
consumer-side statement: what tuesday relies on, and how it allocates from
it. Note: conduit is being rebuilt (portfolio ADR-0015) — the carrier moves
from forge PR labels to work-item frontmatter, and this page changes with
the rebuild's Measure step.

## What conduit emits on every PR

| Element | Value |
|---|---|
| Effort label | exactly **one** of `effort:1-super-quick` `effort:2-not-long` `effort:3-average` `effort:4-a-while` `effort:5-felt-like-forever` (closed enum) |
| ADR label | `adr:<reference>`, e.g. `adr:ADR-0003` |
| PR title | prefix `[ADR-0003] ` |
| PR body | final line is the trailer `Adr-Reference: ADR-0003` |
| Commits | same `[ADR-0003]` prefix + `Adr-Reference` trailer |
| Branch | `conduit/<reference-lower>/<task-slug>` — never the `adr/` prefix, which is adroit's namespace |

The effort label is **final at merge time** — that is the moment tuesday
reads it.

## What tuesday reads, and how

- **Effort:** the `effort:N-*` label, parsed against the closed enum
  byte-for-byte. No valid effort label ⇒ the PR lands in `unallocated_prs`
  (the QC list); multiple ⇒ warn and take the first.
- **ADR:** the first `adr:<reference>` label; fallback to the
  `Adr-Reference:` body trailer when no label is present. The title prefix is
  for humans and is not parsed.
- **Categories:** all remaining labels after the structural prefixes
  (`effort:*`, `adr:*`) are removed.

## The allocation ruling

conduit's dogfood forge pre-creates only `effort:*` and `conduit:*` labels —
**no category labels exist** there. Without a ruling, every conduit PR would
have zero category labels, land in `unallocated_prs`, and fail a strict run.
The portfolio referee resolved this:

> **PRs carrying an `adr:*` label COUNT AS ALLOCATED, even with no category
> label.**

Concretely:

- **Strict rule:** a merged PR passes strict mode when it has exactly one
  `effort:N-*` label **and** (at least one category label **or** an `adr:*`
  label).
- **ADR credit is whole:** the PR's **full** allocated hours are credited to
  its ADR in `adr_totals` — ADR attribution answers "what did this decision
  cost?", so it is never split the way categories are.
- **Categories still default:** with no category labels, the hours fall under
  `Uncategorized` in the category breakdown — the category view stays
  internally consistent while the ADR rollup carries the meaning.
- **Structural labels are not categories:** `adr:*` and `conduit:*` prefixes
  are attribution/machinery and are excluded from category totals.

## Gitea ingestion (the dogfood proving path)

The dogfood path runs against **conduit's local demo forge** — a throwaway
Gitea at `http://localhost:3000`, org `como`, repo `conduit-dogfood`, seeded
and labeled by conduit's `demo/gitea-init.sh`. That script writes two tokens
with **pinned filenames** into conduit's gitignored secrets directory
(`${COMO_CONDUIT_DIR:-../conduit}/.secrets/` — the `COMO_CONDUIT_DIR` env
knob overrides; the default `../conduit` is the workspace's own conduit
product, relative to `tuesday/`):

- `.secrets/conduit-bot.token` — conduit's write identity
- `.secrets/reviewer.token` — the reviewer identity; **tuesday reads with
  this one** (Measure is read-only)

These tokens are **runtime secrets**, minted per `forge-up` session and
valid only while the throwaway Gitea lives. They are env-first and **never
resolved via git** — never cloned, never committed, never logged. tuesday's
resolution order (crates/tuesday-cli/src/token.rs):

1. an explicit `--token-file <PATH>` flag,
2. the `TUESDAY_GITEA_TOKEN` environment variable,
3. the documented local path
   `${COMO_CONDUIT_DIR:-../conduit}/.secrets/reviewer.token`.

The proving command (`just dogfood-report` wraps it; see
[The Headless CLI](./usage/cli.md)):

```sh
tuesday-report --source gitea --base-url http://localhost:3000 \
  --owner como --repo conduit-dogfood \
  --year <Y> --month <M> \
  --monthly-hours 160 \
  --token-file ../conduit/.secrets/reviewer.token \
  -o json --strict
```

`--monthly-hours` is pinned to `160` on the dogfood path (run-1 learnings):
the CLI's `360` default models a multi-person team and misallocates the
one-engineer dogfood month, so the recipe passes the referee-parity budget
explicitly.

Pass criteria: every merged conduit PR appears as an allocation with nonzero
hours and exactly one effort score; each rolls up under its `adr:ADR-NNNN` in
`adr_totals`; `unallocated_prs` is empty, so `--strict` exits 0. The result is
cross-checked against `conduit verify <task> -o json` — the same PR number,
effort label, and ADR reference from two independent implementations.

## Contract caveats (accepted, documented)

- **Post-merge label edits:** conduit's contract makes the effort label final
  at merge, but tuesday reads at report time. A human editing labels after
  merge makes `conduit verify` and tuesday disagree. This is accepted and
  documented rather than engineered around.
- **Month-window timing:** the report is keyed to a calendar month; PRs merged
  near a boundary can produce an empty or split report. Dogfood runs must pass
  the year/month explicitly rather than relying on defaults.

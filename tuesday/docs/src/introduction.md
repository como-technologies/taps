# Introduction

tuesday turns merged pull requests into **team capacity reports**: each PR
carries a self-reported effort label (`effort:1-super-quick` …
`effort:5-felt-like-forever`), tuesday converts those scores into points on a
configurable scaling series, and a month's budget of hours is allocated
proportionally across the PRs and rolled up by work category. The unit of
measurement is the **team**, never the individual — the goal is "where does
capacity flow?", not surveillance.

In the Como Technologies portfolio, tuesday is the **Measure stage of the TAPS
loop**: a decision recorded with adroit (Prescribe) becomes work driven by
conduit (Adopt) becomes a merged, labeled PR that tuesday measures — so the
hours a team spends trace back to the decision that caused them. The
contract tuesday consumes from that loop is documented on the
[Dogfood Contract](./dogfood-contract.md) page.

## Honest maturity

This page states what exists and what is merely decided.

**Works today (on `main`):**

- A Cargo workspace: pure `tuesday-core` (the calculator, the domain types,
  and ingestion) with two heads — `tuesday-web` and `tuesday-cli`.
- An interactive [Dioxus](https://dioxuslabs.com/) fullstack web app: configure
  an organization, repositories, a month, a monthly-hours budget, and a scaling
  series; get the capacity report rendered in the browser.
- A headless CLI, `tuesday-report` (`--source github|gitea … -o json
  --strict`), emitting the canonical `MonthlyReport` JSON — over one month
  (`--year/--month`) or an inclusive multi-month range (`--from/--to`,
  one unchanged per-month report each plus a cross-month ADR rollup) — see
  [The Headless CLI](./usage/cli.md).
- A read-only `PrSource` ingestion trait with GitHub **and Gitea** providers
  in `tuesday-core` — both heads dispatch through it: the CLI on `--source`,
  the web head on the forge picker / the `source` field of `ReportConfig`.
- The effort calculator (shared by both heads): closed-enum `effort:N-*` label
  parsing that matches the conduit emission contract byte-for-byte, six
  scaling series, category allocation from PR labels, and an unallocated-PR
  quality-control list.
- ADR attribution: hours rolled up per `adr:<reference>` label (with an
  `Adr-Reference:` body-trailer fallback), `adr_totals` in the report, the
  exclusion of structural `adr:*` / `conduit:*` labels from category totals,
  and the allocation ruling (ADR-labeled PRs count as allocated even without
  a category label).
- A machine-readable JSON export endpoint, `POST /api/export_report` — see
  [JSON Export](./usage/json-export.md).
- A test suite across the workspace (calculator, domain, CLI lanes), pinned
  by the four cargo lanes in `just lanes`.

See [Architecture Decisions](./dev/decisions.md) for the corpus.

**Known limits, stated plainly:**

- The `--from/--to` range is CLI-only; the web UI and the export endpoint
  report one month at a time (a range need on the web head would consume
  the CLI's envelope shape, per ADR-0007).
- In the interactive UI the **browser** fetches the forge API directly, so a
  self-hosted Gitea must allow CORS from the app's origin; the headless
  `POST /api/export_report` path fetches server-side and has no such
  constraint.
- There is no Como-hosted deployment and no built-in scheduler — both
  retired by ADR (ADR-0008, ADR-0009) as the suite-done state. The generic
  self-host story (local build, static release bundle, container) lives on
  the [Running tuesday](./running.md) page; recurrence belongs to host
  cron/CI.

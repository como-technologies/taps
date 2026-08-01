# The Monthly Report

The report answers one question: **where did this month's team capacity go?**

## How hours are computed

1. Every PR **merged** in the configured month window is fetched from each
   configured repository.
2. Each PR's effort label is parsed. Exactly one of the closed enum is
   expected:

   | Label | Score |
   |---|---|
   | `effort:1-super-quick` | 1 |
   | `effort:2-not-long` | 2 |
   | `effort:3-average` | 3 |
   | `effort:4-a-while` | 4 |
   | `effort:5-felt-like-forever` | 5 |

   Multiple effort labels produce a warning and the first is used; an unknown
   `effort:` label, or none at all, leaves the PR **unallocated**.
3. Scores become points through the configured
   [scaling series](./configuration.md#scaling-series); the monthly-hours
   budget is divided by the month's total points, and each PR is allocated
   `its points × hours-per-point`.
4. A PR's allocated hours are split **equally across its category labels**.
   Categories are all remaining PR labels after the structural ones are
   removed; a PR with no category labels falls back to `Uncategorized`.

> **Structural labels are not categories:** `effort:*`, `adr:*`, and
> `conduit:*` labels are excluded from category totals. Hours on an
> ADR-labeled PR are attributed to the ADR in `adr_totals` instead — see
> [JSON Export](./json-export.md) and the
> [Dogfood Contract](../dogfood-contract.md).

## What the report shows

- **Summary** — total hours, total effort points, hours per point.
- **Allocations** — one row per PR (team-level; no individual attribution):
  number, title, repository, effort score, allocated hours, percentage of the
  month, and the per-category split.
- **Category totals** — hours per category across the month.
- **Hours by ADR** — full allocated hours per `adr:<reference>`,
  the Measure-side artifact that traces effort back to a decision.
- **Unallocated PRs** — the quality-control list: merged PRs that could not be
  allocated because no valid effort label was found. A clean month has this
  list empty.

# JSON Export

The JSON shape below is the **canonical report schema**: the headless CLI
([`tuesday-report`](./cli.md)) serializes this same `MonthlyReport` rather
than invent a second one (see
[Architecture Decisions](../dev/decisions.md)). This is test-enforced: a
server-lane parity test drives both this endpoint and the CLI pipeline over
the same stub Gitea forge and asserts byte-identical canonical JSON
(`crates/tuesday-web/src/server_fns.rs`).

This endpoint reports one month at a time. The CLI's multi-month range
(`--from/--to`, ADR-0007) wraps **unchanged** reports of this same schema in
an additive envelope with a cross-month `adr_totals` rollup — there is no
second report schema; see [The Headless CLI](./cli.md).

The server exposes the monthly report at a stable endpoint so other tools can
fetch it headlessly, with no UI session:

```sh
curl -sS -X POST http://127.0.0.1:8080/api/export_report \
  -H 'Content-Type: application/json' \
  -d '{
    "config": {
      "source": "gitea",
      "base_url": "http://localhost:3000",
      "token": "<token>",
      "monthly_hours": 160.0,
      "repositories": ["my-repo"],
      "organization": "my-org",
      "year": 2026,
      "month": 5,
      "scaling_series": "Linear"
    }
  }'
```

The request body wraps a `ReportConfig` (the same fields the UI collects — see
[Configuration](./configuration.md)); `scaling_series` is one of `Linear`,
`Doubling`, `Fibonacci`, `Exponential`, `TShirtSizes`, `Square`. The forge
fields:

- `source` — `"github"` or `"gitea"`; defaults to `"github"` when absent, so
  request bodies predating the field keep working.
- `base_url` — Gitea only (GitHub's API base is fixed; supplying it for
  GitHub is an error). Defaults to `http://localhost:3000`, conduit's
  dogfood forge.
- `token` — required for GitHub; optional for Gitea (anonymous read).

## Response: the `MonthlyReport` schema

The response body is the serialized `MonthlyReport`:

| Field | Type | Meaning |
|---|---|---|
| `month` | string | Month name, e.g. `"May"` |
| `year` | number | Report year |
| `total_hours` | number | The configured monthly-hours budget |
| `total_effort_points` | number | Sum of scaled effort points across allocated PRs |
| `hours_per_point` | number | `total_hours / total_effort_points` |
| `allocations` | array | One entry per allocated PR: `pr_number`, `pr_title`, `repository`, `effort_score`, `allocated_hours`, `percentage_of_total`, `categories` (map of category → hours), `adr_id` (the attributed ADR reference, or `null`) |
| `category_totals` | object | Hours per work category (structural `effort:*` / `adr:*` labels excluded) |
| `adr_totals` | object | **Full** allocated hours per ADR reference — unlike categories, ADR credit is never split |
| `unallocated_hours` | number | Hours that could not be allocated |
| `unallocated_prs` | array | Quality-control list of `[repository, pr_number, pr_title]` for merged PRs with no valid effort label |
| `organization` | string | Organization name (used to build PR links) |

## ADR attribution rules

- Primary source: the first `adr:<reference>` label (e.g. `adr:ADR-0012`).
  Multiple ADR labels log a warning; the first wins.
- Fallback: a body trailer line `Adr-Reference: <reference>`, used only when
  no `adr:` label is present.
- The `[ADR-NNNN]` title prefix is a redundant carrier for human readers; it
  is **not parsed**.
- A PR has at most one ADR; its full allocated hours are credited to that ADR
  in `adr_totals`.

These are exactly the elements of the conduit emission contract — the
normative statement lives on the [Dogfood Contract](../dogfood-contract.md)
page.

# The Headless CLI

`tuesday-report` (crate `tuesday-cli`) is the headless head over
`tuesday-core`: it fetches merged PRs from a forge through the read-only
`PrSource` trait, runs the same effort calculator as the web head, and emits
the canonical `MonthlyReport` — the schema documented on the
[JSON Export](./json-export.md) page, including the `adr_totals` rollup.

```sh
cargo run -q -p tuesday-cli -- --source <github|gitea> \
  --owner <OWNER> --repo <REPO> --year <YYYY> --month <M> -o json --strict
```

## Multi-month ranges (`--from/--to`)

`--from YYYY-MM --to YYYY-MM` replaces `--year/--month` with an inclusive
range — catching up past months or a quarterly view in one command:

```sh
cargo run -q -p tuesday-cli -- --source <github|gitea> \
  --owner <OWNER> --repo <REPO> --from 2025-11 --to 2026-02 -o json --strict
```

Range semantics (ADR-0007):

- One **unchanged** canonical `MonthlyReport` per month of the range; in
  `-o json` they sit in an additive envelope —
  `{ "from", "to", "reports": [...], "adr_totals": {...} }` — where
  `adr_totals` is the cross-month rollup (full credit per decision, summed
  across the range). Each `reports[i]` is bit-for-bit what single-month
  `-o json` emits for that month.
- The range may cross year boundaries; an inverted range is an error.
- An empty month yields an empty report, not a hole — a quarter includes
  its quiet months.
- `--monthly-hours` is the per-month budget, applied to each month
  independently.
- `--strict` is checked month by month; any month's violation makes the
  exit code nonzero, and the report is still emitted for inspection.
- The default table mode prints one sectioned per-month table plus the
  cross-range ADR rollup.

## `tuesday-report --help`

Captured from the real binary (regenerate this page when the CLI surface
changes):

```text
Headless tuesday: fetches merged PRs from a forge, runs the effort
calculator, and emits the canonical serde MonthlyReport — the same schema
as the web head's JSON export, including the adr_totals rollup.

The window is one month (--year/--month) or an inclusive multi-month
range (--from/--to, ADR-0007): the range emits one unchanged canonical
MonthlyReport per month plus a cross-month adr_totals rollup.

With -o json the report is pure JSON on stdout (logs go to stderr).
With --strict, every merged PR must carry exactly one effort:N-* label
AND (a category label OR an adr:* label); violations are listed on
stderr and the exit code is nonzero (ADR-0005 allocation ruling). In
range mode the contract is checked month by month.

Usage: tuesday-report [OPTIONS] --source <SOURCE> --owner <OWNER> --repo <REPO>

Options:
      --source <SOURCE>
          Forge provider to read merged PRs from

          [possible values: github, gitea]

      --owner <OWNER>
          Repository owner (organization or user)

      --repo <REPO>
          Repository name; repeat the flag for multiple repositories

      --year <YEAR>
          Report year, e.g. 2026 (single-month mode; pair with --month)

      --month <MONTH>
          Report month, 1-12 (single-month mode; always pass it explicitly: the month-boundary trap is real)

      --from <YYYY-MM>
          First month of a multi-month range, inclusive (range mode, ADR-0007; pair with --to)

      --to <YYYY-MM>
          Last month of a multi-month range, inclusive

      --monthly-hours <MONTHLY_HOURS>
          Total team hours to allocate across the month's merged PRs

          [default: 360]

      --base-url <BASE_URL>
          Forge base URL (gitea only; defaults to conduit's dogfood forge at http://localhost:3000. GitHub's API base is fixed.)

      --token-file <PATH>
          Read the API token from this file (overrides GITHUB_TOKEN / TUESDAY_GITEA_TOKEN; gitea also falls back to the documented ${COMO_CONDUIT_DIR:-../conduit}/.secrets/reviewer.token)

  -o, --output <OUTPUT>
          Output format: compact table for humans, json for the canonical report

          [default: table]
          [possible values: table, json]

      --scaling <SCALING>
          Effort-score scaling series for the hour split

          [default: linear]
          [possible values: linear, doubling, fibonacci, exponential, t-shirt-sizes, square]

      --strict
          Enforce the dogfood contract: exit nonzero unless every merged PR has exactly one effort label and a category or adr:* label

      --kb <SPACE_DIR>
          Also write each month's report as a measure-report typed page into this KB space (the directory holding wiki.toml), at wiki/measures/<owner>-<YYYY-MM>.md. Deterministic — same forge data and arguments, byte-identical page. Admission (llm-wiki ingest) and committing stay with the caller; with --strict, pages are skipped when violations are found (a contract-violating month doesn't enter the record)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Emit into the knowledge base — `--kb`

tuesday is a Measure head of the Como knowledge base (portfolio ADR-0010 /
portfolio#7 wave 4): `--kb <space-dir>` writes each month's report as a
`measure-report` **typed page** — frontmatter carrying the period,
instrument, totals, and the `adr_hours` attribution map; body carrying the
by-decision and by-category tables — into the space's `wiki/measures/`, so
a harness answers "what did ADR-N cost last month?" from KB pages alone.

The head writes and stops, per the kb-spec admission model: strict schema
validation is the space's ingest gate, and committing stays with the
caller. Pages are deterministic (sorted maps, no emission timestamp), so a
re-run over the same forge data converges byte-identically; under
`--strict`, a violating month writes no page — a broken contract doesn't
enter the record.

```sh
tuesday-report --source gitea --owner como --repo conduit-dogfood \
    --year 2026 --month 6 --strict -o json \
    --kb ../team-space          # + wiki/measures/como-2026-06.md
llm-wiki ingest . --wiki team   # admit it through the strict gate
```

## The dogfood proving command

`just dogfood-report` wraps the Measure-stage run against conduit's local
demo forge — see the [Dogfood Contract](../dogfood-contract.md) page for the
full contract and pass criteria.

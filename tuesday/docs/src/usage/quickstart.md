# Quickstart — point tuesday at your forge

This page takes an engineering lead from a fresh clone to a capacity
report over **your own** repositories — GitHub or self-hosted Gitea, one
month or a multi-month range. Everything here runs locally
([Running tuesday](../running.md)); the only thing tuesday ever does with
your forge is **read merged pull requests**.

What you need:

- a Rust toolchain ([rustup](https://rustup.rs/)) and a clone of this repo;
- a forge API token with read access (scopes below);
- merged PRs labeled with effort scores (`effort:1-super-quick` …
  `effort:5-felt-like-forever`) — see the
  [Monthly Report](./report.md) page for the labeling model.

## 1. Create a read-only token

**GitHub** — <https://github.com/settings/tokens>:

- *Fine-grained token*: select your repositories, Repository permissions →
  **Pull requests: Read-only** (Metadata: Read-only is added
  automatically). This is the least-privilege option.
- *Classic token*: scope `repo` (private repositories) or `public_repo`
  (public only).

**Gitea** — your instance → Settings → Applications → **Generate New
Token**:

- scope **read:repository** (the CLI reads
  `/repos/{owner}/{repo}/pulls` only);
- add **read:organization** if you also want the web head's
  organization/repository pickers.

Measure never writes — if a token form offers write scopes, leave them off.

Put the token in a file (recommended — it stays out of shell history):

```sh
install -m 600 /dev/null my-forge.token && $EDITOR my-forge.token
```

The CLI also accepts `GITHUB_TOKEN` / `TUESDAY_GITEA_TOKEN` from the
environment; `--token-file` wins when both are present. Gitea may be read
anonymously if your instance allows it.

## 2. One month

GitHub:

```sh
cargo run -q -p tuesday-cli -- --source github \
  --owner my-org --repo my-repo \
  --year 2026 --month 5 --monthly-hours 160 \
  --token-file my-forge.token -o json --strict
```

Self-hosted Gitea (same shape, plus the instance URL):

```sh
cargo run -q -p tuesday-cli -- --source gitea --base-url https://gitea.example.com \
  --owner my-org --repo my-repo \
  --year 2026 --month 5 --monthly-hours 160 \
  --token-file my-forge.token -o json --strict
```

Both command shapes are verified against live forges (GitHub, and Gitea
1.24 with a June-2026 month carrying a labeled merged PR). You get the
canonical `MonthlyReport` on stdout — allocations per PR, `adr_totals`
per decision, category totals, and the unallocated-PR QC list (the
[JSON Export](./json-export.md) page documents every field). Drop
`-o json` for a human table. Repeat `--repo` for more repositories;
`--monthly-hours` is your team's real hour budget for the month.

## 3. A range of months

Catching up a quarter (or a year boundary) is one command:

```sh
cargo run -q -p tuesday-cli -- --source gitea --base-url https://gitea.example.com \
  --owner my-org --repo my-repo \
  --from 2025-11 --to 2026-02 --monthly-hours 160 \
  --token-file my-forge.token -o json --strict
```

The output is an envelope: one unchanged per-month `MonthlyReport` under
`reports`, plus a cross-month `adr_totals` rollup — semantics on
[The Headless CLI](./cli.md) page (ADR-0007).

## 4. Read the exit code

- `0` — report emitted; with `--strict`, every merged PR in the window
  carries exactly one effort label and a category or `adr:*` label.
- `1` — runtime failure (bad token, unreachable forge), or `--strict`
  violations: each offending PR is listed on stderr **and the report is
  still printed** for inspection. In a range, the contract is checked
  month by month.
- `2` — usage error (e.g. `--from` without `--to`, month outside 1–12).

A missing/wrong token fails fast with the fix spelled out (which env var
or flag to set); an empty month is not an error — it yields an empty
report, because a quiet month is a true measurement.

## 5. Prefer a UI?

`just serve` runs the interactive web head (see
[Running tuesday](../running.md) for the `dioxus-cli` requirement). Enter
the same values in the Settings page's per-forge integration cards —
Gitea auth there is the same token paste (token-only by decision,
ADR-0010; no OAuth registration on your instance). The
[JSON export endpoint](./json-export.md) serves the same canonical report
headlessly from the self-hosted server.

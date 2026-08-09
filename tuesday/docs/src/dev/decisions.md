# Architecture Decisions

tuesday's architectural decisions live in the committed `docs/src/adr/`
corpus, managed with [adroit](https://github.com/como-technologies/taps/tree/main/adroit) —
the portfolio's own Prescribe-stage tool (dogfooding is the point). adroit is
KB-only (adroit ADR-0020): it operates against an ephemeral wiki seeded
from the committed corpus, which stays canonical:

```sh
tmp=$(mktemp -d)
printf 'name = "adrs"
' > $tmp/wiki.toml && mkdir -p $tmp/wiki/decisions
adroit seed --from docs/src/adr --dir $tmp   # bootstrap the wiki
adroit list --dir $tmp                       # what has been decided
adroit show 5 --dir $tmp -o json             # one decision, machine-readable
adroit check --dir $tmp                      # validate the seeded wiki
```

New decisions are authored as legacy-shape files in `docs/src/adr/`
(see the corpus for the house form) and validated the same way.

## Reading the statuses honestly

**Accepted records a decision, not a shipped implementation.** The seed
corpus deliberately accepts ADRs whose code lands in later milestones — the
decision is made and reviewable now; each ADR's *Implementation* section
names the milestone that builds it.

## The corpus

| ADR | Decision | Built? |
|---|---|---|
| ADR-0001 | Split tuesday into a `tuesday-core` library with web and CLI heads | Yes — the three crates (`crates/tuesday-core`, `-web`, `-cli`) are on `main` |
| ADR-0002 | Keep `tuesday-core` on async reqwest for wasm32 compatibility (recorded divergence from the sync-ureq house choice) | Yes — the wasm32 check on `tuesday-core` is a CI lane (`just lanes`) |
| ADR-0003 | Ingest merged PRs through a read-only `PrSource` trait with GitHub and Gitea providers | Yes in both heads — the CLI dispatches on `--source github\|gitea`, and the web head's `generate_report` and org/repo pickers dispatch on `ReportConfig.source` through the same seam |
| ADR-0004 | Emit one canonical `MonthlyReport` JSON from a headless CLI (`-o json`, `--strict`) | Yes — `tuesday-report` emits it; the web export endpoint shares the schema |
| ADR-0005 | Count ADR-labeled PRs as allocated and exclude structural labels from categories (the portfolio referee's allocation ruling) | Yes — `adr:*` / `conduit:*` excluded from categories, ADR-labeled PRs allocated, enforced by `--strict` |
| ADR-0007 | Emit multi-month ranges as an additive envelope of unchanged per-month `MonthlyReport`s | Yes — `--from/--to` on `tuesday-report`; see [The Headless CLI](../usage/cli.md) |
| ADR-0008 | Retire the GCP deployment machinery as the suite-done state (reopen criteria recorded) | Yes — `cloudbuild.yaml`, `.gcloudignore`, and the three GCP scripts are deleted; `Containerfile` + `scripts/build-static-release.sh` survive as the generic self-host story on [Running tuesday](../running.md) |
| ADR-0009 | Retire built-in scheduling: tuesday stays a one-shot CLI driven by host cron | Yes (a decided non-feature) — the cron wrapper example lives on [Running tuesday](../running.md); catch-up is the ADR-0007 range |
| ADR-0010 | Retire Gitea OAuth: token auth is the web head's only Gitea path | Yes (a decided non-feature) — the Gitea Settings card is instance URL + token; OAuth stays GitHub-only |

The consumer-side contract these decisions serve is stated on the
[Dogfood Contract](../dogfood-contract.md) page.

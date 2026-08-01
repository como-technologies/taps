# Running tuesday

tuesday is **local-first**: you build and run it on your own machine (or
your own host). There is no Como-hosted instance — the GCP deployment
machinery was retired by ADR-0008, and what survives is the generic
self-host story on this page. Recurrence (monthly runs) belongs to your
host's scheduler, not to tuesday — ADR-0009 retired the built-in-scheduler
option; the [cron example below](#scheduling-host-cron-not-a-daemon) is the
documented wrapper.

Prerequisites for everything on this page: a Rust toolchain
([rustup](https://rustup.rs/)) and a clone of the
[taps](https://github.com/como-technologies/taps) workspace (tuesday lives
in `tuesday/`).

## The headless CLI (no web stack needed)

The fastest path to a report is `tuesday-report` — plain cargo, no
`dioxus-cli`, no server:

```sh
cargo run -q -p tuesday-cli -- --help
```

or build a release binary once and put it on your PATH:

```sh
cargo build --release -p tuesday-cli
# → target/release/tuesday-report
```

See the [Quickstart](./usage/quickstart.md) for tokens and exact commands,
and [The Headless CLI](./usage/cli.md) for the full flag surface.

## The web app, interactively (`dx serve`)

The interactive head is a Dioxus fullstack app with hot reload:

```sh
just serve        # cd crates/tuesday-web && dx serve
```

This requires a `dioxus-cli` release matching the workspace's dioxus 0.7
dependency (`cargo install dioxus-cli`); a mismatched `dx` (e.g. 0.6.x)
will not serve this app. Configuration happens in the UI — see
[Configuration](./usage/configuration.md).

## The headless server (JSON export without a browser)

The web crate also builds **without** the web UI feature into a plain
server binary that mounts the [JSON export](./usage/json-export.md)
endpoint:

```sh
just build-server # cargo build -p tuesday-web --no-default-features --features server
# → target/debug/tuesday
PORT=8123 IP=127.0.0.1 target/debug/tuesday
```

The server binds `IP`/`PORT` from the environment (default port 8080).
Verified flow: with the server on port 8123,
`POST http://127.0.0.1:8123/api/export_report` returns the canonical
`MonthlyReport` for the posted `ReportConfig`.

## The static release bundle

For a long-running self-hosted instance, build the fullstack release
bundle (requires the same dioxus 0.7-matching `dx`):

```sh
scripts/build-static-release.sh   # dx build --fullstack --release
# → target/dx/tuesday/release/server        (the server binary)
# → target/dx/tuesday/release/web/public    (the static web assets)
```

Copy both to your host and run `./server` next to `public/`.

## A container image

`Containerfile` is the OCI recipe over the same release bundle: a
`rust:slim-bookworm` build stage running `dx build --fullstack --release`,
and a `debian:bookworm-slim` runtime stage carrying the server binary and
web assets. Provide a dioxus-cli binary at `tools/dx` before building
(`tools/` is not committed). It targets any OCI host — there is no
provider-specific deployment config in the repo (ADR-0008) — and is
documented as the recipe, not exercised by CI.

## Scheduling: host cron, not a daemon

tuesday has no resident scheduler and will not grow one (ADR-0009): every
run is a one-shot process with an honest exit code, which is exactly what
cron and CI schedulers want. The wrapper pattern is the repo's own
`just dogfood-report` recipe; a host crontab equivalent for "last month's
report, first day of each month" looks like:

```cron
# m h dom mon dow  command  (note: % must be escaped as \% inside crontab)
0 6 1 * * tuesday-report --source github --owner my-org --repo my-repo \
  --year $(date -d "last month" +\%Y) --month $(date -d "last month" +\%-m) \
  --token-file /etc/tuesday/token -o json --strict \
  > /var/lib/tuesday/$(date -d "last month" +\%Y-\%m).json
```

Missed a few months? Don't replay the scheduler — catch up with one
inclusive range (`--from/--to`, [ADR-0007](./dev/decisions.md)):

```sh
tuesday-report --source github --owner my-org --repo my-repo \
  --from 2025-11 --to 2026-02 --token-file /etc/tuesday/token -o json --strict
```

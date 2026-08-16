# conduit

Forge-neutral agentic development harness — the Adopt-stage engine of the Como
TAPS loop. Design spec (normative): `docs/src/dev/spike-design.md`.

## Working agreements (IMPORTANT — read first)

- **Never push to a real remote. Never open a PR on any public forge.** The only
  push target that ever exists is the throwaway localhost Gitea container (and
  local bare repos in tests). GitHub and GitLab mutations are ALWAYS
  DryRun-decorated — the constructors only hand out `DryRun(GitHubForge)` /
  `DryRun(GitLabForge)` (ADR-0012 / ADR-0016).
- **All work stays under `~/repos/como-tech/**`.** Tokens live in gitignored
  `.secrets/`; never commit or log them.
- **Humans hold every gate.** No `Forge::merge` method exists; the `conduit:run`
  label and PR review/merge are human actions. Do not add automation that
  bypasses a gate.
- **conduit never authors, edits, or transitions an ADR** — that is adroit's
  lane. conduit no longer invokes adroit at all: the subprocess seam died
  with portfolio ADR-0015 (the rebuild is taps issue 113).
- **All documentation lives in the mdbook** (`docs/src/**`, wired into
  `docs/src/SUMMARY.md`). No standalone Markdown docs elsewhere. Keep code and
  docs in sync; `just book` must build.
- **No client names** in docs/comments/examples — keep examples generic.
- Never write a bare `#<number>` in forge-rendered text (commits, PR/issue
  bodies) — use `task N` / plain `N`.

## Build & test

Always use `just` recipes — never raw `cargo`/`mdbook`.

```sh
just init        # toolchain components + mdbook
just ci          # fmt-check + clippy + test + book (the gate)
just test        # all tests
just forge-up    # throwaway Gitea on localhost:3000 (demo/; FORGE_PORT overrides the host port)
just forge-down  # destroy it
```

The customer demo kit (`demo/kit/demo-up`, per-beat scripts, `demo-down`)
packages the full TAPS engagement demo — narrated script:
`docs/src/usage/customer-demo.md`; design: ADR-0015.

`cargo audit` runs as a separate CI job (`just crate-audit`, plus a weekly
schedule) so a fresh advisory can't mask the code gates.
The `docs/src/adr` corpus is the legacy-format repo of record (ADR-0017):
adroit is KB-only (adroit ADR-0020), so validation seeds the corpus into
an ephemeral space and checks it there. A new entry matches the existing
legacy format exactly and must pass `adroit check` on a seeded space.
adroit's forge integration stays disabled.

Env-gated test legs: `CONDUIT_E2E_GITEA=1` (live Gitea conformance),
`CONDUIT_E2E_GITHUB=1` (GitHub live reads), `CONDUIT_E2E_CLAUDE=1` (live
claude CLI engine smoke).

## Design rules

- Fully synchronous — no tokio. HTTP via ureq behind the `HttpTransport` seam;
  unit tests inject `FakeTransport`, never the network.
- Typed errors (`thiserror`) in lib modules; `anyhow` only in `main.rs`.
- Pure core, effectful shell: `contract.rs`, `machine.rs`, `forge::diff` are
  pure and exhaustively unit-tested; `router.rs` owns all effects.
- State is files under `.conduit/` you can `cat` — no database.
- Never put test-only state in a production type; use injected fakes
  (`FakeForge`, `FakeEngine`, `FakeTransport`) and documented env overrides.

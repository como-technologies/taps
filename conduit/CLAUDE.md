# conduit

The Adopt-stage engine of the Como TAPS loop, being rebuilt as a
harness-first execution store: work items as conduit-owned KB classes,
humans gating intent (sign-off) rather than diffs, a mechanical merge
door. The decision is portfolio ADR-0015; the plan of record and running
implementation notes are taps issue 113. The old forge-integrator surface
(forge adapters, driven engine, poll-tick router, Docker demo) is still
in-tree and dies with the rebuild's item 7.

## Working agreements (IMPORTANT — read first)

- **Never push to a real remote. Never open a PR on any public forge.** The
  only push targets that ever exist are the throwaway localhost Gitea
  container (old surface, dies at item 7) and local bare repos in tests.
  GitHub and GitLab mutations are ALWAYS DryRun-decorated.
- Tokens live in gitignored `.secrets/`; never commit or log them.
- **Complete alpha until the Getting Started walk finishes** (taps issue
  46): anything may change or break at-will, no compatibility or history
  obligations. Don't preserve "what was" unless it is critical to
  understanding "what is".
- **Humans gate intent, not diffs** (portfolio ADR-0015): sign-off and
  project close are human seats; the harness can neither write nor grant
  approval; a task's `done` belongs to the mechanical merge door alone.
  `src/workitem.rs` is the rule table — extend it, never bypass it.
- **conduit never authors, edits, or transitions an ADR** — that is
  adroit's lane. conduit does not invoke adroit at all.
- **No book during the rebuild — deliberate.** The old book described the
  deleted shape and was removed whole. Implementation notes land on taps
  issue 113 as a landing comment per checklist item (what shipped, the
  design calls the code can't self-explain, threads left open). A new book
  is written from the new code, commit messages, and issue record once the
  shape stabilizes after the walk.
- **No client names** in docs/comments/examples — keep examples generic.
- Never write a bare `#<number>` in text conduit renders to a forge — use
  plain `N`. (Governs the old surface's PR/issue bodies while it lives.)

## Build & test

Always use `just` recipes — never raw `cargo`.

```sh
just init        # toolchain components
just ci          # fmt-check + clippy + test (the gate)
just test        # all tests
just forge-up    # throwaway Gitea on localhost:3000 (old surface; dies at item 7)
just forge-down  # destroy it
```

`cargo audit` runs as a separate CI job (`just crate-audit`, plus a weekly
schedule) so a fresh advisory can't mask the code gates.

Env-gated test legs: `CONDUIT_E2E_GITEA=1` (live Gitea conformance),
`CONDUIT_E2E_GITHUB=1` (GitHub live reads), `CONDUIT_E2E_CLAUDE=1` (live
claude CLI engine smoke) — all old-surface legs.

## Design rules

- Typed errors (`thiserror`) in lib modules; `anyhow` only in `main.rs`.
- Pure core, effectful shell: rule tables (`workitem.rs`, `contract.rs`)
  are pure and exhaustively unit-tested; effects live at the edges.
- The work-item doors (`surface.rs`, `mcp.rs`, `work.rs`) are async over
  the KB client, one clap definition serving terminal and MCP; the old
  poll-tick surface stays sync until item 7 deletes it. (The tokio-free
  lane retired with this.)
- Actor honesty: terminal = human seat, MCP = harness, and the merge
  door's authority is internal to `complete` — `signoff` is absent from
  the MCP door entirely, and `Actor` is never accepted from a caller.
- Never put test-only state in a production type; use injected fakes and
  documented env overrides (the doors test against an in-memory
  `WorkStore`; internal git is real in tests).

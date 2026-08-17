# conduit

Harness-first execution store for the Adopt stage (portfolio ADR-0015;
the rebuild record is taps issue 113). Work items — project/story/task —
are conduit-owned KB classes behind the llm-wiki appliance; humans gate
intent (sign-off seals), the lifecycle is a pure rule table, and the
doors (one clap definition, terminal + MCP) are the only way work-item
state changes. Internal bare repos under `.conduit/repos/` are the local
remotes work lands on through the mechanical merge door.

## Working agreements (IMPORTANT — read first)

- **Never push to a real remote.** The only push targets that exist are
  the internal bare repos (and tempdir repos in tests). Mirroring to
  external forges is a deferred integration, not product code.
- Tokens and secrets never get committed or logged.
- **Complete alpha until the Getting Started walk finishes** (taps issue
  46): anything may change or break at-will, no compatibility or history
  obligations. Don't preserve "what was" unless it is critical to
  understanding "what is".
- **Humans gate intent, not diffs** (portfolio ADR-0015): sign-off and
  project close are human seats; the harness can neither write nor grant
  approval; a task's `done` belongs to the mechanical merge door alone.
  `src/workitem.rs` is the rule table and `src/approval.rs` the seal —
  extend them, never bypass them.
- **Actor honesty is wiring**: terminal = `HumanSeat`, MCP = `Harness`,
  `MergeDoor` only inside `complete`. `signoff` is absent from the MCP
  door entirely; `Actor` is never accepted from a caller.
- **conduit never authors, edits, or transitions an ADR** — that is
  adroit's lane. conduit does not invoke adroit at all.
- **No book during the rebuild — deliberate.** Implementation notes land
  on taps issue 113 as a landing comment per checklist item (what
  shipped, the design calls the code can't self-explain, threads left
  open). A new book is written from the new code, commit messages, and
  issue record once the shape stabilizes after the walk.
- **No client names** in docs/comments/examples — keep examples generic.
- Never write a bare `#<number>` in text conduit renders into a repo
  (the squash-commit messages) — use plain `N`.

## Build & test

Always use `just` recipes — never raw `cargo`.

```sh
just init        # toolchain components
just ci          # fmt-check + clippy + test (the gate)
just test        # all tests
```

`cargo audit` runs as a separate CI job (`just crate-audit`, plus a
weekly schedule) so a fresh advisory can't mask the code gates.

Config surface: the suite pair `KB_URL`/`KB_WIKI` (discovery order:
process env > cwd `.env` > `~/.config/taps/env`) and
`CONDUIT_GATE_TIMEOUT_SECS` (merge-door gate deadline, default 1800).
The per-project gate command is frontmatter (`gate:`, default `just ci`).
The harness workspace template and posture skills live in `kit/`.

## Design rules

- Typed errors (`thiserror`) in lib modules; `anyhow` only in `main.rs`.
- Pure core, effectful shell: the rule tables (`workitem.rs`,
  `approval.rs`) are pure and exhaustively unit-tested; effects live in
  the doors (`surface.rs`) and their seams (`work.rs`, `repo.rs`).
- The doors are async over `como-kb-client` (one tokio runtime per
  invocation); everything below them is sync.
- The body is the contract, the frontmatter is state: no door ever
  rewrites a body, and the seal pins body bytes only.
- Never put test-only state in a production type; the doors test against
  an in-memory `WorkStore`, and internal git is real in tests.

## Writing standard (STRONG REQUIREMENT)

All output follows ASD-STE100 (Simplified Technical English): short
sentences, active voice, one idea per sentence, plain words. Do not
coin jargon — if a term is not defined where the reader stands, do not
use it. This applies to docs, book and guide pages, kit files, CLI and
report output, commit messages, and issue comments.

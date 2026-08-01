# ADR-0002: Keep tuesday-core on async reqwest for wasm32 compatibility

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner; tuesday maintainers; maintainers of the
other house-stack apps (the divergence from the house HTTP choice is the point
of record).

## Context and Problem Statement

The house HTTP choice for portfolio CLIs is synchronous `ureq` behind a thin
transport seam (adroit, conduit). tuesday's core cannot follow it: the
workspace split (ADR-0001) requires `tuesday-core` to compile to
`wasm32-unknown-unknown` so the Dioxus web head reuses the same ingestion and
report code in the browser, and `ureq` does not target wasm32. Diverging from
a house convention without a recorded reason is exactly the drift the ADR
corpus exists to prevent, so the divergence must be decided explicitly.

## Decision Drivers

- `tuesday-core` must compile for both native and `wasm32-unknown-unknown`.
- The existing crate already depends on async `reqwest` with rustls — zero
  migration cost.
- House preference for synchronous HTTP exists to keep CLIs thin; any
  divergence needs a recorded, reviewable reason.
- No tokio (or any native-only dependency) may leak into core, or the web
  head breaks silently.

## Considered Options

- **Async `reqwest` in core** — works on native and wasm32 (browser `fetch`
  under the hood); the CLI head supplies the tokio runtime at its boundary.
- **Sync `ureq` in core (house choice)** — matches conduit/adroit, but does
  not compile to wasm32, which forfeits code reuse in the web head entirely.
- **Dual transport behind a seam** (ureq native, reqwest wasm) — keeps the
  house choice on native at the cost of two HTTP stacks to test and a seam
  with no second consumer; complexity without a driver.

## Decision Outcome

Chosen: **async `reqwest` in `tuesday-core`**, because core must compile to
wasm32 for the web head and reqwest is the one mature HTTP client that spans
both targets; the divergence from the sync-ureq house choice is deliberate
and recorded here. The async boundary stays at the heads: `tuesday-cli` owns
its tokio runtime, the web head rides Dioxus's executor, and core exposes
async functions without spawning.

### Positive Consequences

- One ingestion implementation serves browser and CLI — no dual-stack drift.
- No migration: the dependency and its rustls configuration already exist.
- The CLI head stays free to keep its runtime minimal
  (`tokio::runtime::Runtime::block_on` at the edge).

### Negative Consequences

- tuesday diverges from the portfolio's sync-HTTP convention, and every
  future contributor must learn why (this record is the mitigation).
- Async signatures are viral within core; tests need an async executor.
- CI must build core for `wasm32-unknown-unknown` from the split onward or
  the wasm guarantee rots silently.

## Implementation

Enforced during milestone M2 (ADR-0001's split): `tuesday-core`'s dependency
set is `reqwest`/`serde`/`chrono` (wasm-compatible), with tokio confined to
`tuesday-cli`; CI gains a `wasm32-unknown-unknown` build of core.

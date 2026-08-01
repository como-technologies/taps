# ADR-0001: Split tuesday into a tuesday-core library with web and CLI heads

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner; tuesday maintainers; conduit maintainers
(consumers of the Measure handoff).

## Context and Problem Statement

tuesday is a single Dioxus fullstack crate. Its genuinely good part — the
effort calculator (closed-enum effort label parsing, six scaling series,
category allocation, unallocated-PR QC) — is pure logic, but it consumes
GitHub-GraphQL-shaped types and renders only through the Dioxus UI. There is
no testable core, no headless output, and no place for a second forge provider
to live. The portfolio direction makes tuesday the Measure-stage verifier of
the TAPS loop, which requires exactly such a core: one report engine consumed
by two heads (the existing web UI and a new headless CLI).

## Decision Drivers

- The calculator must become unit-testable without a browser or network.
- A headless CLI head is required for the dogfood loop (read conduit's demo
  forge, emit JSON).
- The existing Dioxus web head must keep working unchanged for partners.
- Core logic must compile to `wasm32-unknown-unknown` so the web head can
  reuse it.
- Shared artifact across portfolio apps is the label/title/trailer contract,
  never code — tuesday's split must not import conduit code.

## Considered Options

- **Cargo workspace split**: `tuesday-core` (calculator + `PrSource` +
  providers + report model; reqwest/serde/chrono; wasm-compatible; no Dioxus,
  no tokio), `tuesday-cli` (clap + tokio runtime), `tuesday-web` (the existing
  Dioxus app, depends on core).
- **Keep the single crate** and add a CLI behind another cargo feature —
  smaller diff, but the feature matrix grows unbounded and nothing stops UI
  types leaking into the calculator.
- **Extract a separate repository** for the core — maximal isolation, but
  splits versioning and review for no benefit at this scale.

## Decision Outcome

Chosen: **Cargo workspace split into `tuesday-core` + `tuesday-cli` +
`tuesday-web`**, because it gives the calculator a pure, wasm-compatible home
with real unit tests while both heads render the same report model, and it
keeps one repo and one review stream. The seam to cut is `calculator.rs`
importing `github::PullRequest`; core consumes only neutral domain types.

### Positive Consequences

- The calculator becomes testable in isolation; the first real unit tests
  land with the split.
- The headless CLI and the web UI cannot drift apart on report semantics —
  they share one struct.
- Forge providers get a home (`tuesday-core`) that neither head owns.

### Negative Consequences

- A workspace migration touches `Cargo.toml`, imports, and the build scripts;
  it must wait for the in-flight `adr-attribution` branch to merge first, and
  rebase over it.
- CI cost roughly doubles (core must additionally build for
  `wasm32-unknown-unknown`).
- Two more crates to version and keep coherent.

## Implementation

Milestone M2 of the iteration-1 direction: extract `crates/tuesday-core` with
`MergedPr` and `trait PrSource`; port `calculator.rs` onto neutral types; wrap
the existing GraphQL client as `GithubSource`; add unit tests for effort
parsing, every `ScalingSeries`, category split math, unallocated handling,
and structural-label exclusion. Acceptance: the web app behaves identically;
`cargo test` passes in core; core builds for `wasm32-unknown-unknown` in CI.

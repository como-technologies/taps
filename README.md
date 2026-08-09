# taps

The Como Technologies **TAPS suite** — the Tools, Apps, Products, and
Services behind the four-stage modernization loop (Assess → Prescribe →
Adopt → Measure) — as one multi-crate Cargo workspace.

| product | what it is |
|---|---|
| [`adroit`](adroit/) | Decision records for the loop — the only writer of the KB's decision pages; rebuilding greenfield, taps #93 (Prescribe) |
| [`assessments`](assessments/) | amaker — chat-driven assessment authoring, respondent forms, and analysis (Assess) |
| [`conduit`](conduit/) | forge-neutral agentic development harness — the Adopt-stage engine |
| [`llm-wiki`](llm-wiki/) | git-backed wiki engine with MCP/ACP servers — the suite's knowledge-base substrate |
| [`portfolio`](portfolio/) | the reader-facing book: the suite's story and how the loop closes |
| [`pulse`](pulse/) | anonymous, cryptographically private team polling (Measure) |
| [`tuesday`](tuesday/) | decision-attributed effort reporting from merged PRs (Measure) |
| [`contract`](contract/) | `como-contract` — the cross-product seams (labels, the adroit read slice, the golden export fixture), shared as one set of types |

**New here?** The **[Getting Started guide](https://como-technologies.github.io/taps/getting-started/)**
([source](getting-started/)) walks you from a fresh machine to a first
trip around the loop on your own project.

## Building

A Rust toolchain (pinned suite-wide in [`rust-toolchain.toml`](rust-toolchain.toml))
and [`just`](https://github.com/casey/just); `just init` installs the
rest (mdbook + preprocessors, cargo-audit, live-server, inotify-tools).

```sh
just init                  # one-time: the tools the root recipes need
cargo build                # everything
cargo build -p <crate>     # one product
just ci                    # the whole suite gate
```

`just ci` at the root is the gate — fmt, clippy, the workspace test suite,
the per-product invariant lanes (`just lanes`), and all seven books.
`just crate-audit` runs separately (its own CI job plus a weekly sweep) so
a fresh advisory can't mask the code gates. Heavier per-product lanes
(adroit's Vue build, tuesday's `dx` builds, assessments' Tailwind) live in
each product's own justfile.

## Documentation

- **Published books:** <https://como-technologies.github.io/taps/> — one
  site, per-product paths. Start with the portfolio book, or jump
  straight to the [Getting Started guide](https://como-technologies.github.io/taps/getting-started/).
  `just books-serve` mirrors the whole site locally on one port.
- **Operating the suite** (stand-up, verification rings, the end-to-end
  demo): [`portfolio/OPERATIONS.md`](portfolio/OPERATIONS.md).
- Each product carries its own docs under `<product>/docs/` — including its
  decision records at the uniform `docs/src/adr/` path — and its own
  README for product-level detail.

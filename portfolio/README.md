# Como Technologies Portfolio

The source for the **TAPS Portfolio** book — Como Technologies' guide to
its Tools, Apps, Products, and Services and the closed four-stage
modernization loop (Assess → Prescribe → Adopt → Measure) they serve. It's
an [mdBook](https://rust-lang.github.io/mdBook/) site; the prose lives in
[`docs/src/`](docs/src/) and the navigation in
[`docs/src/SUMMARY.md`](docs/src/SUMMARY.md).

The book is deliberately short: it tells the story of the tools and how
they work together, and links to each tool's own repo for the details.
Start with the rendered [Introduction](docs/src/introduction.md). This
README is about how to **build and work on the book**. For standing the
whole suite up locally, see [OPERATIONS.md](OPERATIONS.md).

## Prerequisites

| Tool | Why | Install |
|---|---|---|
| [Rust toolchain](https://rustup.rs) (`cargo`) | mdBook and its plugins are installed via `cargo install` | `curl https://sh.rustup.rs -sSf \| sh` |
| [`just`](https://github.com/casey/just) | Command runner for every task below | `cargo install just` (or your package manager) |

## Getting started

```sh
# 1. Install mdBook and the book's preprocessors (mdbook, mdbook-mermaid, mdbook-gruvbox)
just init

# 2. Serve the book locally with live reload; opens your browser
just book-serve
```

That's it — edit anything under `docs/src/` and the page reloads. To
produce a static build instead, run `just book` (output lands in
`target/book/`).

Run `just` (or `just --list`) any time to see every available recipe.

## Common tasks

| Command | What it does |
|---|---|
| `just init` | Install mdBook + the `mermaid` and `gruvbox` preprocessors |
| `just book-serve` | Serve locally with live reload (opens browser) |
| `just book` | Build the static site into `target/book/` |
| `just book-test` | Validate code blocks in the book |
| `just clean` | Remove all build artifacts |
| `just ci` | Build the book and validate the ADR corpus |

## CI

`just ci` runs `book` and `adr-check`. The `adr-check` recipe validates
this repo's own decision corpus (`docs/src/adr/`) by seeding it into an
ephemeral KB space with adroit, resolved by the suite convention
(`ADROIT_BIN` → sibling `../adroit` build → PATH → cached install) — and
**skips with a notice** when adroit isn't available, so it never blocks a
plain `just book`.

## Layout

```
docs/src/       book content (Markdown); SUMMARY.md is the table of contents
docs/book.toml  mdBook configuration
docs/gruvbox/   theme assets
justfile        all tasks — run `just` to list them
scripts/        cold-sim, the pre-review cold-checkout rehearsal (see OPERATIONS.md)
OPERATIONS.md   the suite-wide stand-up and verification runbook
```

# taps

The Como Technologies suite as a single multi-crate Cargo workspace: adroit,
assessments, conduit, llm-wiki, portfolio (the book), pulse, and tuesday, plus
the shared `como-contract` crate.

Decided in portfolio ADR-0012 and executed per portfolio#8. This repo starts
with fresh history; the seven original repos are the archive. Each product was
copied in from its `main` at these SHAs (recorded 2026-08-01):

| product | source repo | SHA |
|---|---|---|
| adroit | como-technologies/adroit | `2de71ef` |
| assessments | como-technologies/assessments | `ca11cee` |
| conduit | como-technologies/conduit | `03414d0` |
| llm-wiki | como-technologies/llm-wiki | `8467a3c` |
| portfolio | como-technologies/portfolio | `2131b11` |
| pulse | como-technologies/pulse | `15e7d5f` |
| tuesday | como-technologies/tuesday | `42076ec` |

## Working in the workspace

- `just ci` at the root is the whole gate — fmt, clippy, the workspace test
  suite, the per-product invariant lanes (`just lanes`), `adr-check` over
  every product's ADR corpus with the in-tree adroit, and all six books.
  `just crate-audit` runs separately (its own CI job plus a weekly sweep)
  so a fresh advisory can't mask the code gates.
- Toolchain is pinned suite-wide in `rust-toolchain.toml`.
- Products keep their directory identity, their own mdBook, and their own
  `docs/src/adr` corpus; one Pages workflow publishes all the books under
  one site with per-product paths.
- Heavier per-product lanes (adroit's Vue build, tuesday's `dx` builds,
  assessments' Tailwind) live in each product's own justfile.

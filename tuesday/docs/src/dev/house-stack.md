# House Stack & CI

tuesday follows the Como house conventions: a `justfile` as the single entry
point, this mdbook under `docs/` (all project documentation lives in the book —
no standalone docs), and an adroit-managed `docs/src/adr/` corpus validated
by the workspace-root `just adr-check`.

```sh
just            # list every recipe
just ci         # the full gate
```

## The four cargo lanes

tuesday is three crates in the taps workspace (`tuesday-core` + the
`tuesday-web` and `tuesday-cli` heads), so the gate runs four cargo lanes
(`just lanes`):

| Lane | Cargo flags | What it proves |
|---|---|---|
| **crate tests** | `cargo test -p tuesday-core -p tuesday-cli -p tuesday-web` | the calculator/domain/CLI suite |
| **web** | `cargo check -p tuesday-web --features web` | the Dioxus fullstack UI as shipped |
| **server** | `cargo check -p tuesday-web --no-default-features --features server` | the headless server build — the JSON export path |
| **wasm32** | `cargo check -p tuesday-core --target wasm32-unknown-unknown` | the ADR-0002 guard: `tuesday-core` stays wasm32-compatible |

`just ci` runs: `fmt-check`, `lint` + `lint-server`, `lanes`, `test-server`,
`book`, `crate-audit`.

## Gate strictness

The reconciliation-era hedges are gone (M2 hedge removal); every gate in
`just ci` is blocking:

- **`fmt-check` fails on any drift.** The pre-split edition-2024 formatting
  drift was cleared by a single pure style commit; new drift fails CI.
- **Both clippy lanes run with `-D warnings`.** The pre-split web head's
  warning backlog (dead code, deprecated Dioxus server-fn structs, unused
  imports) was cleared, not silenced — a new warning fails CI.

Keep the `justfile` recipe comments and this page in sync if gate scope
ever changes.

## The ADR gate

ADR-corpus validation is the workspace-root `just adr-check`, a leg of the
root `just ci`: it builds the in-tree adroit (`cargo build -p adroit`),
seeds each product's `docs/src/adr` — tuesday's included — into an
ephemeral KB space, and runs `adroit check` on it. The per-product
adroit-resolution chain retired with the move to the single workspace.

```sh
just adr-check   # from the workspace root
```

## The book

```sh
just book        # mdbook build docs  -> docs/book
just book-serve  # live reload
```

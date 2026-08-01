# House Stack & CI

tuesday follows the Como house conventions: a `justfile` as the single entry
point, this mdbook under `docs/` (all project documentation lives in the book —
no standalone docs), and an adroit-managed `adr/` corpus validated in CI.

```sh
just            # list every recipe
just ci         # the full gate
```

## The four cargo lanes

tuesday is a workspace (`tuesday-core` + the `tuesday-web` and `tuesday-cli`
heads), so the gate runs four cargo lanes (`just lanes`, pinned by
`.github/workflows/ci.yml`):

| Lane | Cargo flags | What it proves |
|---|---|---|
| **workspace tests** | `cargo test --workspace` | the calculator/domain/CLI suite |
| **web** | `cargo check -p tuesday-web --features web` | the Dioxus fullstack UI as shipped |
| **server** | `cargo check -p tuesday-web --no-default-features --features server` | the headless server build — the JSON export path |
| **wasm32** | `cargo check -p tuesday-core --target wasm32-unknown-unknown` | the ADR-0002 guard: `tuesday-core` stays wasm32-compatible |

`just ci` runs: `fmt-check`, `lint` + `lint-server`, `lanes`, `test-server`,
`book`, `adr-check`.

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

`just adr-check` validates `adr/` with adroit, resolved by the suite's
uniform cross-repo convention (the resolver is self-contained in the
justfile — never sourced from a sibling):

1. **`ADROIT_BIN`** — explicit env override.
2. **Sibling build** — `../adroit/target/release/adroit`, then the debug
   build. (Note the order change from the PATH-first days: a fresh sibling
   build now beats a stale globally-installed `adroit`.)
3. **PATH** — an installed `adroit`.
4. **Clone cache** — `.como/tools/bin/adroit` (gitignored). An existing
   cache is always used; a fresh
   `cargo install --git $COMO_GIT_BASE/adroit.git --locked` is attempted
   only when `COMO_GIT_BASE` is explicitly set and `COMO_OFFLINE` isn't —
   the gate never reaches for the network by default.
5. **Skip with a notice** naming all the knobs — the gate is advisory, so
   CI works on machines without the sibling repo.

```sh
cd ../adroit && cargo build --release   # to arm the gate
# or: ADROIT_BIN=/path/to/adroit just adr-check
# or: COMO_GIT_BASE=https://github.com/como-technologies just adr-check
```

## The book

```sh
just book        # mdbook build docs  -> docs/book
just book-serve  # live reload
```

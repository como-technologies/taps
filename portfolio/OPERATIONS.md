# Operating the suite

The internal runbook: how to stand the whole TAPS suite up from a cold
clone and verify it, in widening rings — then the end-to-end engagement
demo. Everything here runs locally; nothing is pushed. (This is Como-facing
operational detail; the reader-facing story is the book under `docs/`.)

## Prerequisites

The suite is **one Cargo workspace** (portfolio ADR-0012): every product —
`adroit`, `assessments`, `conduit`, `llm-wiki`, `portfolio`, `pulse`,
`tuesday`, plus the shared `contract` crate — lives in this repo, builds
into one `target/` dir, and shares one dependency table. A single
`git clone` is the whole suite; nothing resolves siblings, pins revisions,
or clones caches anymore. Each app's ADR corpus ships **inside its own
published mdbook source** at the uniform `docs/src/adr/` path.

**Toolchain.** A Rust toolchain with `cargo` and `just` (root
`just init` installs the cargo-side tools, mdbook included); `git`;
Docker with its daemon up and the `docker compose` plugin (the Adopt demo
runs a throwaway Gitea forge in it); `gh` is optional (GitHub read-only
legs). The AI lanes are **optional**: only the demo's `--live` variants call
a local `ollama` serving `llama3.2` (no API key, nothing phones home) — the
pre-baked fast path needs neither.

Verify the demo's runtime prerequisites (and pull the model, if `ollama` is
installed and you want the live lanes) with one command:

```sh
conduit/demo/kit/preflight        # checks docker; pulls llama3.2 if ollama is up
```

**Commit signing.** Nothing here asks you to change your git config. The
suites that spin up throwaway git repos in their tests disable commit
signing *in those disposable repos*, so a global `commit.gpgsign = true`
(with no key for the throwaway identity) can't fail them.

**Every input is a suite product.** The Adopt demo's fictional client corpus
is built from llm-wiki's starter content (`llm-wiki/kit/starter/decisions` —
a legacy-format corpus, which is exactly what a real client brings), read
in-tree. Where an optional runtime piece is absent (Docker down, no ollama)
the gate and the demo **skip or stop with a notice that names the knob**,
never silently.

## Rings 1–2 — the workspace gate

```sh
just ci      # at the workspace root
```

One command is the whole gate: fmt + clippy + the full workspace test
suite + the per-product invariant lanes + every book. Green means every
app is internally sound and every book builds. (ADR corpora are validated
on demand with the in-tree adroit — the automated `adr-check` leg retired
during the onboarding walk, taps#46.)

Suite *coherence* is no longer a gate at all — it is the crate graph.
The seams live once in the `como-contract` crate (effort labels, the
adroit read slice) and in the single golden export fixture at
`contract/fixtures/`, so a seam cannot drift between products; the
compiler and the two fixture-pinned tests hold it.

Dependency advisories gate via `cargo audit` against the workspace's one
`.cargo/audit.toml`, where accepted advisories live as dated, documented
ignores (what was accepted, why, and the removal trigger). A red audit on
a cold clone is not automatically a code failure: a freshly published
advisory reddens an unchanged tree — update the dependency or record the
acceptance, never bypass the gate. Per-product lanes (the wasm lane,
adroit's no-default-features core, the web builds) remain available in each
product's own justfile.

## Ring 3 — the whole loop, live, end to end

The customer demo kit stands the entire engagement up against a throwaway
forge, runs all four loop stages with machine evidence at each seam, and
tears everything down:

```sh
cd conduit
demo/kit/preflight               # verify docker (+ pull llama3.2 for --live)
just init-adroit                 # build the in-tree adroit into .conduit/bin
demo/kit/demo-up                 # throwaway Gitea + client corpus + its derived KB space
demo/kit/beat-1-measure-prior    # pulse's prior-iteration signal
demo/kit/beat-2-assess           # brief + signal -> assessment   (--live for ollama)
demo/kit/beat-3-prescribe        # assessment -> ADRs in a KB space + stored plan (--live)
demo/kit/beat-4-adopt            # stored plan -> human-gated PR -> merge -> verify 6/6
demo/kit/beat-5-measure          # tuesday --strict + measure page into the space + query
demo/kit/demo-down               # destroys the forge; leaves nothing
```

`init-adroit` builds the workspace's own adroit and places it at
`.conduit/bin/adroit` (the path conduit's `AdrSource` resolves) — the old
`adroit.rev` pin and its remote/sibling resolution chain are retired; the
contract is the crate graph itself. `demo-up` builds the client corpus from
llm-wiki's starter content, seeds a **per-run KB space derived from it** —
the corpus repo on the forge stays the legacy repo of record; adroit and
conduit operate on the space (conduit ADR-0017, portfolio ADR-0009 made
visible) — builds the product binaries into the workspace target dir, and
seeds the throwaway forge; it stops early with named knobs if Docker isn't
up (run `preflight` first).

**Verify as you go**: every artifact a run produces lives in one per-run
workdir — `conduit/demo/kit/.current` points at the active one — with the
KB at `$WORK/corpus-space` (read it any time:
`.conduit/bin/adroit list --dir "$WORK/corpus-space"`; after beat 5,
`cat "$WORK/corpus-space"/wiki/measures/*.md`). The customer-demo page's
"Seeing it" section carries the beat-by-beat table of what you should
see, and each beat prints an ` -> inspect:` hint.

Each beat prints its talking point and the machine evidence it just produced
(verify 6/6, byte-identical forge transcripts, `CROSS-CHECK PASS`, the
measure-report page landing in the run's space with its `adr_hours`
attribution — and, when an llm-wiki binary is built, a search over the
space answering "what did this decision cost?" from pages alone; absent,
that close skips with a notice, and `preflight` prints the fact). The
pre-baked path runs every beat in seconds and needs only Docker; `--live`
recomputes the two ollama lanes for real (timings in the customer demo
kit's narrated page). Deeper conformance is env-gated: `CONDUIT_E2E_GITEA=1`
(live forge), `CONDUIT_E2E_ADROIT=1`, `CONDUIT_E2E_GITHUB=1`.

## Ring 4 — the knowledge base, harness-first

The harness-first loop (portfolio ADR-0010) from the same cold clone,
in two legs. The **mechanical leg** is scripted and verifiable with no AI
anywhere — it stands a fully provisioned space up from the kit's
decision seed and ends gate-clean (the kit's wiki starter set is gone:
content follows schema ownership, and each tool contributes its own):

```sh
cargo build -p llm-wiki -p adroit             # both in-tree, one target dir
cd llm-wiki
export LLM_WIKI_CONFIG="$DIR.registry.toml"   # scope the registry to the run:
                                              # disposable space, disposable registry —
                                              # nothing lands in ~/.llm-wiki
llm-wiki spaces create "$DIR" --name team --set-default
adroit seed --from kit/starter/decisions --dir "$DIR"   # starter decisions, fresh identities
llm-wiki ingest . --wiki team                           # strict admission gate
adroit check --dir "$DIR"                               # semantic gate: clean
llm-wiki lint --wiki team                               # zero errors is the rehearsed bar
```

The **conversational leg** needs a harness: copy the kit's
`claude-code/.mcp.json`, `CLAUDE.md`, and `skills/` into the space and
open Claude Code there (or configure Claude Desktop with both MCP
servers, adroit started `--allow-write` — its ADR-0021). Then author:
content classes through the engine seams, decisions through adroit,
every write behind the same gates the mechanical leg just proved. The
captured evidence for both shapes lives with the kit —
`kit/worked-example/session.md` (a real Claude Code session, gates
catching real mistakes) and the recorded MCP-only rehearsal (the full
decision lifecycle over raw JSON-RPC). tuesday closes the ring's loop:
`tuesday-report --kb "$DIR"` lands the month's capacity report beside
the decisions it prices, and a search over the space answers "what did
this decision cost?" from pages alone.

## The pre-review cold gate

A cold clone of this repo is the whole suite: the pre-review rehearsal
is `git clone` into a fresh directory, then rings 1–2 (`just ci` at the
root) and, hardware permitting, ring 3's pre-baked demo path.

## Publishing

Standing the suite up locally needs no remotes. Publishing the books to
Pages rides the repo's own CI (one workflow, all seven books, one site —
the layout is the root `just site` recipe; `just books-serve` mirrors it
locally).

# Operating the suite

The internal runbook: how to stand the whole TAPS suite up from a cold
checkout and verify it, in widening rings — then the end-to-end engagement
demo. Everything here runs locally; nothing is pushed. (This is Como-facing
operational detail; the reader-facing story is the book under `docs/`.)

## Prerequisites

The suite is a set of sibling repositories that resolve each other by the
uniform convention (each repo's ADR records it): an explicit `COMO_<REPO>_DIR`
env override, else a sibling checkout `../<repo>`, else a pinned git clone into
a gitignored cache. The simplest layout is all repos checked out under one
parent (`assessments`, `adroit`, `conduit`, `tuesday`, `pulse`,
`llm-wiki`, `portfolio`) — then everything resolves with no configuration
(`llm-wiki`, the KB substrate, resolves like any other sibling and builds
from source at HEAD per this repo's ADR-0009). Each app's ADR
corpus ships **inside its own published mdbook source** at the suite's uniform
`docs/src/adr/` path, so every repo's corpus gate finds it in any checkout —
no separate corpus download.

**Toolchain.** A Rust toolchain with `cargo`, `just`, and `mdbook`; `git`;
Docker with its daemon up and the `docker compose` plugin (the Adopt demo runs a
throwaway Gitea forge in it); `gh` is optional (GitHub read-only legs). The AI
lanes are **optional**: only the demo's `--live` variants call a local `ollama`
serving `llama3.2` (no API key, nothing phones home) — the pre-baked fast path
needs neither. conduit builds its pinned adroit itself with `just init-adroit`.

Verify the demo's runtime prerequisites (and pull the model, if `ollama` is
installed and you want the live lanes) with one command:

```sh
conduit/demo/kit/preflight        # checks docker; pulls llama3.2 if ollama is up
```

**Commit signing.** Nothing here asks you to change your git config. The suites
that spin up throwaway git repos in their tests disable commit signing *in those
disposable repos*, so a global `commit.gpgsign = true` (with no key for the
throwaway identity) can't fail them.

**Every input is a suite repo.** The Adopt demo's fictional client corpus
is built from llm-wiki's starter content (`kit/starter/decisions` — a
legacy-format corpus, which is exactly what a real client brings), so it
resolves like everything else: the uniform convention, no special-case
inputs, no extra knobs. Where a repo is absent the gate and the demo
**skip or stop with a notice that names the knob**, never silently.

## Ring 1 — each app on its own

Run the house gate in each repo:

```sh
just ci      # fmt + clippy + tests + book build + ADR-corpus check, per repo
```

Every repo's gate validates its ADR corpus: the five Rust apps each carry an
`adr-check` leg in `just ci` — since adroit went KB-only (adroit ADR-0020,
portfolio ADR-0009) that leg seeds the committed corpus into an ephemeral KB
space and validates it there, so the gate also exercises the KB machinery on
every run. llm-wiki (the KB product) gates with its cargo suite, which
covers provisioning, the Como schema library, and the kit's counted starter
content. Green in all of them means each app is internally
sound — the Rust apps formatted, lint-clean, and tested; every mdbook builds;
every corpus validates. This is the per-app truth check and the fastest
signal.

Ring 1 is also where suite *coherence* is gated: each cross-repo seam is
pinned by contract tests in the repos that own it (assessments' golden
export vendored into adroit's ingest tests, conduit's contract constants
under unit test, tuesday's consumer-side checks), so a seam drifting fails
the owning repo's gate — not a separate suite-wide script.

The Rust repos also gate dependency advisories with `cargo audit` — a
`crate-audit` leg in `just ci` (conduit runs it as a dedicated CI job
instead). A red audit on a cold checkout is not automatically a code
failure: a freshly published advisory reddens an unchanged tree. Accepted
advisories live in each repo's `.cargo/audit.toml` as dated, documented
ignores (what was accepted, why, and the removal trigger), so a new red is
a decision to make — update the dependency or record the acceptance there —
never a reason to bypass the gate.

## Ring 2 — this repo's own gate

```sh
cd portfolio && just ci
```

That builds the book and validates this repo's own ADR corpus
(`adr-check` seeds it into an ephemeral KB space, resolving adroit by the
suite convention and skipping with a notice when it can't). The book makes
no mechanically-verified claims about the siblings anymore — details live
in each tool's own repo, and the seams are gated where they are owned
(Ring 1); the retired `verify-claims` gate is recorded as superseded in
`docs/src/adr/`.

## Ring 3 — the whole loop, live, end to end

The customer demo kit stands the entire engagement up against a throwaway
forge, runs all four loop stages with machine evidence at each seam, and tears
everything down:

```sh
cd conduit
demo/kit/preflight               # verify docker (+ pull llama3.2 for --live)
just init-adroit                 # build the pinned adroit into .conduit/bin
demo/kit/demo-up                 # throwaway Gitea + client corpus + its derived KB space
demo/kit/beat-1-measure-prior    # pulse's prior-iteration signal
demo/kit/beat-2-assess           # brief + signal -> assessment   (--live for ollama)
demo/kit/beat-3-prescribe        # assessment -> ADRs in a KB space + stored plan (--live)
demo/kit/beat-4-adopt            # stored plan -> human-gated PR -> merge -> verify 6/6
demo/kit/beat-5-measure          # tuesday --strict + measure page into the space + query
demo/kit/demo-down               # destroys the forge; leaves nothing
```

`init-adroit` resolves the pinned adroit by the suite convention — the adroit
remote at the pinned rev (reachable there today), else a sibling `../adroit`
when the remote is unreachable (its HEAD, with a loud local-dev notice, if
that checkout lacks the exact pin). `demo-up` builds the client corpus from
llm-wiki's starter content, seeds a **per-run KB space derived from it** —
the corpus repo on the forge stays the legacy repo of record; adroit and
conduit operate on the space (conduit ADR-0017, portfolio ADR-0009 made
visible) — resolves the sibling binaries the same way, and seeds the
throwaway forge; it stops early with named knobs if Docker isn't
up or llm-wiki doesn't resolve (run `preflight` first).

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
attribution — and, when an llm-wiki binary resolves, a search over the
space answering "what did this decision cost?" from pages alone; absent,
that close skips with a notice, and `preflight` prints the fact). The
pre-baked path runs every beat in seconds and needs only Docker; `--live`
recomputes the two ollama lanes for real (timings in the [customer demo](https://github.com/como-technologies/conduit)
kit's narrated page). Deeper conformance is env-gated: `CONDUIT_E2E_GITEA=1`
(live forge), `CONDUIT_E2E_ADROIT=1`, `CONDUIT_E2E_GITHUB=1`.

## Ring 4 — the knowledge base, harness-first

The harness-first loop (portfolio ADR-0010) from the same cold checkout,
in two legs. The **mechanical leg** is scripted and verifiable with no AI
anywhere — it stands a fully provisioned space up from the kit's starter
content and ends gate-clean:

```sh
cd llm-wiki && cargo build --release          # the KB product, from source at HEAD
export LLM_WIKI_CONFIG="$DIR.registry.toml"   # scope the registry to the run:
                                              # disposable space, disposable registry —
                                              # nothing lands in ~/.llm-wiki
llm-wiki spaces create "$DIR" --name team --set-default
adroit seed --from kit/starter/decisions --dir "$DIR"   # starter decisions, fresh identities
cp -r kit/starter/wiki/. "$DIR/wiki/"                   # glossary + guides, typed pages
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

## The pre-review cold gate — `scripts/cold-sim`

Rings 1–3 are what a cold reviewer runs mechanically; `scripts/cold-sim`
(in this repo) rehearses exactly those before a review, as one command
(ring 4's mechanical leg is rehearsed where its content lives — the kit's
starter README in llm-wiki — and isn't in cold-sim yet; the conversational
leg needs a human with a harness by definition). It clones
the suite side by side into a fresh sandbox and runs the runbook verbatim
under a contributor-default environment a warm workspace never exercises: a
hostile global git config (`commit.gpgsign = true` with a throwaway
identity, `/dev/null` system config) and every `COMO_*` knob scrubbed from
the child environment.

```sh
portfolio/scripts/cold-sim                            # all three rings, fresh /tmp sandbox
portfolio/scripts/cold-sim --ring 3 --leg preflight   # stepwise: one ring, one leg
```

- `--from local` (default) clones each repo from its sibling working copy
  via `file://` — the future *pushed* state of any unpushed local commits
  (a clone carries committed history only, never the dirty tree).
  `--from github` clones `https://github.com/como-technologies/<repo>`
  instead — the published reality.
- The sandbox clones **llm-wiki** like every other suite repo, so ring 3's
  `demo-up` must fully stand up — the client corpus builds from its starter
  content, and a failure to resolve it is a real `FAIL`, not a documented
  stop.
- What it cannot simulate it records instead of faking: ollama-on-PATH and
  docker-daemon reachability are printed as env facts, and a down daemon
  degrades the docker-dependent ring-3 legs to `ENV-LIMITED` — preflight
  still runs regardless, because its honest reporting is part of the check.
- Per-leg logs land under `<sandbox>/logs/`, the last output line is one
  JSON result object for tooling, and the exit is nonzero only on a `FAIL`.
  Stepwise runs (`--ring`, `--repo`, `--leg`, `--soak N`) reuse a `--dir`
  sandbox. The caller's cargo registry cache is reused — fresh clones
  already force cold `target/` builds.

## Publishing

Standing the suite up locally needs no remotes. Publishing each app to its
canonical remote is a deliberate owner action, kept out of the loop's
automation. The pinned adroit rev in `conduit/adroit.rev` resolves from the
adroit remote, and a cold checkout that clones the suite side by side runs
the demo with no overrides.

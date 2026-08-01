# ADR-0017: KB-native demo: per-run corpus space, repo of record stays legacy

> State: Accepted

## Status

Accepted
Created: 2026-07-28

## Stakeholders

conduit maintainers (the demo kit, the adroit pin, and the adr-check gate
live here) and the Como portfolio owner, whose ring-3 demo is the suite's
north-star deliverable and whose KB-native mandate (portfolio task 9) this
decision lands.

## Context and Problem Statement

The adroit pin moved to the KB-only line (adroit's ADR-0020): `--dir` /
`ADROIT_DIR` must name a KB **space** — a directory carrying a `wiki.toml`
with decision pages under `wiki/decisions` — and path mode is retired.
`adroit seed --from <legacy-dir> --dir <space>` bootstraps a fresh space
from a legacy corpus. Before this decision, everything demo-shaped in
conduit was path-mode: `client-corpus-init.sh` resolved `[adroit] dir` to
the client corpus repo's bare `docs/src/adr`, beat 3 imported into a bare
scratch directory, the `adr-check` gate ran `check --dir docs/src/adr`
directly, and nothing anywhere created a space. Bumping `adroit.rev` to the
KB-only pin hard-errors every one of those call sites (`not a KB space (no
wiki.toml)`).

Two secondary contract shifts ride the same pin: `-o json` serializes
status **lowercase** (`"accepted"` — the KB decision schema's status enum),
where the pre-KB pins emitted `"Accepted"`; and the kit's Measure beat can
now close the loop *into* the knowledge base — the sibling tuesday grew
`--kb <space>` (a `measure-report` typed page), and llm-wiki can register,
ingest, and search the space.

The question: where do spaces live in the demo, and what happens to the
legacy-format corpus repo the forge is seeded with?

## Decision Drivers

- Portfolio ADR-0009's model: the **repo of record stays canonical, the
  space is derived** — the demo should make that model visible, not invert
  it
- ADR-0015's kit discipline holds: kit-owns-no-state (mutable state lives
  in the per-run workdir), pre-baked/live split, evidence-per-beat,
  idempotent re-runs (wipe and rebuild, never accrete)
- The pre-baked path's only hard runtime prerequisite stays docker — no new
  required binary; llm-wiki must remain optional
- Spaces are ephemeral-first (adroit's ADR-0020): seeded per run from the
  committed corpus, never a second persisted copy to drift
- The hand-scaffold (`wiki.toml` + `wiki/decisions`) is the suite-wide
  bootstrap shape the other repos' gates already use — no llm-wiki binary
  is needed to stand up a space

## Considered Options

- **Per-run derived space in the workdir; forge repo stays legacy.**
  `client-corpus-init.sh` scaffolds `<workdir>/corpus-space` by hand, seeds
  it with the pinned adroit from the built corpus repo's `docs/src/adr`,
  and resolves `[adroit] dir` to the space. Beat 3's scratch import corpus
  becomes a scratch space the same way. `adr-check` seeds `docs/src/adr`
  into an ephemeral `mktemp` space and checks there.
- **Convert the client corpus repo (and `docs/src/adr`) to KB spaces.**
  Makes the forge-seeded repo of record a space — inverting ADR-0009's
  model, churning every committed ADR file, and coupling the corpus format
  to llm-wiki's page profile for no demo gain.
- **Hold the pre-KB pin.** Leaves ring 3 demonstrating a retired adroit
  contract and blocks the suite-wide KB-native mandate on its flagship
  demo.

## Decision Outcome

Chosen: **per-run derived space, repo of record stays legacy**, because it
lands the KB-native pin while keeping ADR-0009's canonical/derived split
visible in the demo itself and changing nothing about what the forge
holds.

Concretely:

- `adroit.rev` pins the KB-only adroit line; the rev-file comment names the
  contract shift (spaces, `seed`, lowercase status).
- `demo/client-corpus-init.sh` hand-scaffolds `<workdir>/corpus-space`
  (`wiki.toml` + `wiki/decisions`), runs the pinned `adroit seed --from
  <client-corpus>/docs/src/adr --dir <workdir>/corpus-space`, and resolves
  the generated `conduit.toml`'s `@ADROIT_DIR@` to the space. Wipe-and-
  reseed on re-run, like every other workdir artifact. The built corpus
  repo — and therefore the forge seed — keeps the legacy `docs/src/adr`
  shape.
- Beat 3 imports into `<workdir>/prescribe/space` (same hand-scaffold);
  the stored-plan determinism proof reads the accepted decision from the
  workdir's corpus-space. Beat 4 needs no change beyond the retargeted
  `[adroit] dir`. Beat 5 adds a second `tuesday-report` run with `--kb
  <workdir>/corpus-space` (evidence: the written page path and its
  `adr_hours` block) and an **optional** llm-wiki close — register the
  space in a run-scoped registry (`LLM_WIKI_CONFIG` in the workdir),
  ingest, and search — that skips with a notice naming the knobs
  (`LLM_WIKI_BIN`, the sibling release build, PATH) when no llm-wiki
  binary resolves. Preflight reports that availability as an env fact,
  like ollama, never failing on it.
- `just adr-check` seeds the committed `docs/src/adr` into an ephemeral
  space and validates there — the same pattern the other suite repos' ci
  legs carry. `docs/src/adr` stays the authored corpus of record.
- Conduit's own Accepted guard compares status case-insensitively
  (mirroring adroit's own case-insensitive status parsing), so the
  lowercase wire format cannot regress the plan gate; the env-gated pinned
  contract tests seed the legacy fixture corpus into a temp space in setup
  with the pinned binary's own `seed`.

### Positive Consequences

- Ring 3 demonstrates the shipped adroit contract, not a retired one, and
  the canonical-repo/derived-space model is on screen in the demo instead
  of only in a portfolio ADR
- The loop's Measure artifact lands in the same KB space the decisions
  live in — queryable on stage when llm-wiki is present, skipped honestly
  when not
- No new hard prerequisite: the hand-scaffold keeps the pre-baked path at
  docker-only, and spaces stay ephemeral (nothing new to commit or drift)

### Negative Consequences

- Every demo run pays a seed step (sub-second at this corpus size) and the
  workdir carries a derived copy of the corpus
- The demo's adroit surface now differs from the committed corpus's
  on-disk shape — readers must hold the canonical/derived distinction the
  scripts' comments and the demo docs now spell out
- The stored-plan proof's corpus path in the transcript is a space path,
  so pre-KB rehearsal transcripts are not line-comparable to new ones

## Implementation

`just init-adroit` resolves the new pin (remote leg first, sibling
fallback); `demo/kit/demo-up` seeds the space via `client-corpus-init.sh`;
`tests/demo_init.rs` pins the scaffold + seed + config wiring;
`tests/adroit_contract.rs` (env-gated `CONDUIT_E2E_ADROIT=1`) runs the five
pinned contract assertions against a seeded temp space; the rehearsal log
under `demo/kit/rehearsals/` captures the KB-native run verbatim per
ADR-0015.

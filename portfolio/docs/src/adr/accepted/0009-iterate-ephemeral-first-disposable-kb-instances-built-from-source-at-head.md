# ADR-0009: Iterate ephemeral-first — disposable KB instances, built from source at HEAD

> State: Accepted

## Status

Accepted

## Stakeholders

Portfolio owner (no quasi-production KB to steward while the product
iterates); KB contributors (a local change is live on the next init, no
release ceremony); head maintainers (adroit, tuesday, pulse, the librarian —
develop against a KB they can destroy and recreate at will).

## Context and Problem Statement

[ADR-0008](./0008-build-the-kb-product-in-the-fork-itself-developing-on-main.md)
made the fork the product and named Como's own KB instance "the product's
permanent dogfood," pinned by release tag (`v0.5.0` first, upgrades as tag
bumps). Standing that instance up now (portfolio#6) would create a
quasi-production store to support — migrations, upgrade paths, data
stewardship — while `llm-wiki` and every head are still changing shape
weekly. The migration work that is actually in front of us (backing
portfolio content like `operating.md` with a KB) needs the *machinery* — an
instance to run against, provisioned in one command — not a durable store.
And tag-pinning taxes exactly the loop we are in: change the engine, then
re-tag before any consumer sees it.

## Decision Drivers

- Fast iteration: a change in any local repo should be what the next build
  runs — no release ceremony between edit and effect.
- No standing operational burden: nothing to migrate, back up, or keep
  compatible while the product is finding its shape.
- The suite's existing patterns: the demo kit's throwaway forge
  (`demo-up`/`demo-down`) and the kb-spike's disposable, re-runnable
  spaces both proved ephemeral infrastructure keeps quality *up* — the
  provision path is exercised constantly instead of once.
- Reversibility: "create the real KB" should stay a small, later decision,
  not a prerequisite.

## Considered Options

1. Stand up the persistent instance now (portfolio#6 as written), pinned
   by tag per ADR-0008.
2. **Ephemeral-first**: instances are disposable spaces stood up
   per-checkout; everything builds from source at HEAD of `main`; tags and
   persistent instances return as one deliberate later decision.
3. Persistent instance but HEAD-pinned (split the difference) — the
   stewardship burden without the reproducibility a tag would buy.

## Decision Outcome

Chosen: **ephemeral-first (option 2).**

- **No persistent KB instance yet.** The target DX is: (1) clone a repo,
  (2) `just init`, (3) build or run any product against the ephemeral KB it
  stood up. Spaces live in gitignored working directories, are provisioned
  by `llm-wiki spaces create` (llm-wiki#14: schema library, admission
  hooks, strict defaults, search weights — zero-flag and idempotent, so
  destroy-and-recreate is the normal lifecycle), and are never a source of
  truth. Canonical content stays where it lives today — e.g. the portfolio
  book's pages remain the docs of record; an ephemeral space is *derived
  from* them, never the other way around.
- **Pinning = HEAD of `main`, built from source.** Consumers resolve
  `llm-wiki` by the suite convention (env override → sibling working copy →
  clone cache) and build what they find; the clone-cache leg tracks the
  default branch, not a tag or rev. A local edit in any repo is live on the
  next init.
- **Deferred, deliberately:** portfolio#6 (the persistent instance),
  release tagging (the `v0.6.0` cut and CHANGELOG discipline), llm-wiki#9's
  git-native admission remainder (an ephemeral, rebuilt-from-scratch space
  needs fast idempotent init, not durable cursors), and llm-wiki#14's
  instance CI template (no persistent instance to CI — it shrinks to the
  init recipe). All return together when a "real" KB is declared.

This amends ADR-0008's *pinning and instance-lifecycle* consequences for
the current stage; its substrate and repo-topology decisions (the fork is
the product, three layers, seams) are unchanged, as are the spec's
contracts.

### Positive Consequences

- Zero KB operations burden during the heaviest iteration phase.
- The provisioning path is exercised on every init by every consumer — the
  ephemeral DX *is* a continuous test of llm-wiki#14.
- Content migrations (operating.md first) can prove KB-representability
  with a round-trip gate without moving any source of truth.

### Negative Consequences

- No reproducible "KB as of version X" until tags return; two checkouts at
  different HEADs can behave differently (accepted — that is what fast
  iteration means).
- kb-spec §8 and ADR-0008's tag-pinning language go stale and must carry a
  pointer here (done with this record).
- Rebuild-from-source on init costs compile time on first run per checkout
  (cargo caching amortizes it).

## Implementation

Portfolio: kb-spec updated (pinning language, §2 schema-library and §6
export-seam claims now landed in-engine, §8 instance lifecycle);
portfolio#6 deferred with a pointer here. llm-wiki: #14 items 1–2 shipped
on `main` (schema library in-engine; `spaces create` provisions hooks,
strict defaults, and search weights); items 3–4 rescoped per this record.
Scope correction (same day): "backed by a KB" means each tool is
**refactored to work against** a KB — never that portfolio doc content
migrates into one. operating.md is documentation of the DX only; the
earlier "operating.md ingest + round-trip gate" idea is dead. Next: the
head retrofits, adroit first (adroit ADR-0020: KB-only operation, the
`seed` bootstrap for legacy corpora, per-tool test data), then the other
tools; each consuming repo's `adr-check` leg moves to bootstrap-a-space in
its own wave; operating.md updates as the runbook's reality changes. Suite
tooling lands in Rust (the suite's no-Python rule).

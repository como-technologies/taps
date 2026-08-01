# ADR-0006: Resolve cross-repo references through the suite's uniform resolution convention

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner (the convention's referee); tuesday
maintainers (this repo's resolver copies); conduit maintainers (the token
mint and checkout this repo resolves); every TAPS sibling adopting the same
convention in the same iteration.

## Context and Problem Statement

tuesday's cross-repo references all assumed a fixed sibling layout:
`adr-check` looked for an installed `adroit` then `../adroit`, and the
dogfood token path was a hardcoded `../conduit/.secrets/reviewer.token`
constant baked into the CLI, the live test, the diagnostics, and the book.
A checkout that isn't laid out as `~/repos/como-tech/*` siblings — a fresh
single-repo clone, a CI runner, a differently-named workspace — silently
loses gates or fails with paths that name no remedy. The portfolio-wide
dependency redesign (iteration 2) settled one resolution convention for the
whole suite; tuesday records its adoption here.

## Decision Drivers

- One resolution order, identical in every consuming repo, so knowledge
  transfers and notices can name the same knobs everywhere.
- Self-contained resolvers: no repo may source helper code from a sibling
  to resolve its siblings (the bootstrap problem).
- Runtime secrets (the forge tokens) must never be resolvable via git —
  they are minted per session into gitignored `.secrets/`.
- Advisory gates must stay fast and offline-safe: no network reach by
  default, `COMO_OFFLINE=1` honored.
- Repos without a public remote today (conduit, docs) must keep
  exactly their current skip/notice behavior until the owner pushes them.

## Considered Options

- **The suite's uniform chain** — env override → sibling checkout → PATH
  (binaries) → gitignored `.como/` clone cache → skip-with-notice /
  actionable error, embedded as a self-contained copy per repo.
- **Status quo** — hardcoded `../sibling` paths; keeps working only for the
  one canonical workspace layout and tells a stranger nothing.
- **A shared resolver script in one repo, sourced by the others** — one
  copy to maintain, but every consumer gains a hard bootstrap dependency on
  the repo that hosts it, which is the disease being cured.
- **Absolute paths in committed config** — pins one machine's layout into
  git; unusable anywhere else.

## Decision Outcome

Chosen: **the suite's uniform resolution convention**, as ruled by the
portfolio dependency redesign. The suite-level decision, recorded
identically in each adopting repo:

> Cross-repo references in the Como TAPS suite resolve through one uniform,
> self-contained chain instead of assuming sibling checkouts: (1) an
> explicit environment override (`ADROIT_BIN` for the adroit binary,
> `COMO_<REPO>_DIR` for a checkout directory), (2) the sibling checkout
> `../<repo>`, (3) for binaries, an installed binary on PATH, (4) a
> gitignored git-clone cache under `.como/` in the consuming repo,
> populated read-only from
> `${COMO_GIT_BASE:-https://github.com/como-technologies}/<repo>.git`, and
> (5) the existing skip-with-notice for advisory gates or an actionable
> error naming the knobs for hard dependencies. Each repo embeds its own
> copy of the resolver — no repo ever sources helper code from a sibling.
> Contract-grade dependencies stay pinned: conduit installs adroit at the
> exact rev in `adroit.rev` (remote URL by default, sibling `file://` only
> as the local-dev override), and any script that reads another repo's
> source as a contract (portfolio's verify-claims) declares the rev it
> clones and prints which source it actually resolved. Runtime secrets and
> live-forge artifacts are never resolved via git — they are env-first with
> documented local-path fallbacks — and the docs evidence repo is
> local-only by policy, so references to it stop at skip-with-notice.
> Resolvers only clone and fetch: they never push, never add the cache as a
> remote, and never carry credentials in URLs. Repos without a public
> remote today (conduit, docs) degrade to exactly the
> skip-with-notice behavior they produce now, so nothing breaks before the
> owner pushes them.

In tuesday concretely: the Gitea reviewer-token default becomes
`${COMO_CONDUIT_DIR:-../conduit}/.secrets/reviewer.token` (a function, not
a constant — the token itself stays a runtime secret with the resolution
order `--token-file` → `TUESDAY_GITEA_TOKEN` → documented local path);
the live e2e test resolves the conduit checkout the same way; and
`adr-check` becomes the uniform binary resolver `ADROIT_BIN` → sibling
release/debug build → PATH → `.como/tools` cached install (fresh installs
only when `COMO_GIT_BASE` is set and `COMO_OFFLINE` isn't) →
skip-with-notice naming every knob.

### Positive Consequences

- tuesday works from any checkout layout: every sibling reference is
  overridable by one documented env knob.
- Notices and errors name the remedy (`ADROIT_BIN`, `COMO_CONDUIT_DIR`,
  `COMO_GIT_BASE`) instead of assuming the reader knows the layout.
- The token path is finally one definition (an env-aware function) instead
  of a constant string repeated across code, tests, and docs.
- The order change to sibling-before-PATH means a fresh sibling adroit
  build beats a stale globally-installed one during suite development.

### Negative Consequences

- The resolver is deliberately copied per repo (~20 lines in the justfile);
  copies can drift in wording or order — the suite ADR's canonical snippet
  and portfolio's verify-claims are the honesty checks.
- Sibling-before-PATH can surprise a developer whose PATH `adroit` was
  intentionally newer than a stale sibling build; the notice text and this
  ADR call it out.
- The `.como/` cache, once populated, is never auto-refreshed; a stale
  cached adroit can lag until it is removed or a pin is bumped.
- `adr-check`'s cached-install leg is unpinned (remote default branch) —
  acceptable only because the gate is advisory and skips on any failure.

## Implementation

Landed with this ADR on the `dep-resolution` branch:
`crates/tuesday-cli/src/token.rs` (`gitea_default_token_path()` honoring
`COMO_CONDUIT_DIR`, with the injected-env test seam),
`crates/tuesday-core/tests/gitea_live.rs` (env-first checkout resolution),
the CLI help and anonymous-read diagnostic naming the knob, the justfile
`adr-check` uniform resolver, `.como/` gitignored, and the book's
dogfood-contract and house-stack pages documenting the chain.

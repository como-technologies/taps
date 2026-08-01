# ADR-0004: Resolve cross-repo references through the suite's uniform resolution convention

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner (the convention's referee; this book is
where the suite-level ruling is recorded and enforced); sibling-app
maintainers (each repo carries its own copy of the resolver); prospective
readers running `just ci` from a checkout that is not the canonical
sibling workspace.

## Context and Problem Statement

The truthfulness gate (ADR-0003) and the evidence machinery read five
sibling repos and the workspace docs ledger through hardcoded `../<name>`
paths: conduit's `src/contract.rs`, the adroit / conduit / tuesday-report /
assessments `--help` surfaces, pulse's ADRs and source, every badged app's
ADR corpus, and `../docs/iteration-N/run-N`. That assumes one machine's
layout. A fresh single-repo checkout silently skips most of the gate, the
notices name no remedy, and nothing records *which* code reality was
validated against — a sibling on a side branch proves something different
from pinned main, invisibly. The portfolio-wide dependency redesign
(iteration 2) settled one resolution convention for the whole suite;
portfolio records its adoption here and is also the convention's honesty
check, since verify-claims is the suite's only script that reads every
sibling.

## Decision Drivers

- One resolution order, identical in every consuming repo, with notices
  that name the same knobs everywhere.
- Self-contained resolvers: no repo may source helper code from a sibling
  in order to find its siblings.
- Contract-grade reads (the contract.rs table) must say which source they
  validated — sibling working tree or a declared pinned rev — so drift is
  visible, not silent.
- Gates must stay fast and offline-safe: no network reach by default,
  `COMO_OFFLINE=1` honored, and a populated cache never auto-updated.
- The docs evidence ledger is local-only by explicit policy (no remote,
  ever) and must never grow a clone leg.
- Repos without a public remote today must keep
  exactly their current skip/notice behavior until the owner pushes them.

## Considered Options

- **The suite's uniform chain** — env override → sibling checkout → PATH
  (binaries) → gitignored `.como/` clone cache at a consumer-declared pin →
  skip-with-notice / actionable error, embedded as a self-contained copy
  per repo, printing the resolved source per assertion group.
- **Status quo** — hardcoded `../sibling` paths; works only for the one
  canonical workspace layout and records nothing about what was validated.
- **A shared resolver sourced from one repo** — single copy, but every
  consumer gains a bootstrap dependency on the hosting repo, which is the
  disease being cured.
- **Always clone at pins, never read siblings** — reproducible but slow,
  network-dependent, and hostile to local suite development where the
  sibling working tree is the thing being iterated on.

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
> remote today degrade to exactly the
> skip-with-notice behavior they produce now, so nothing breaks before the
> owner pushes them.

In portfolio concretely: `scripts/verify-claims` gains a self-contained
`resolve_repo(name)` (`COMO_<REPO>_DIR` → sibling `../<name>` →
`.como/deps/<name>` clone checked out at the rev declared in the script's
`PINS` table → skip-with-notice naming the knobs), every sibling read goes
through it, and the resolved source — sibling + HEAD sha (flagged when it
differs from the declared pin) or cache + pinned rev — is printed per
assertion group. The docs ledger resolves env → sibling → skip in
verify-claims and env → sibling → actionable error in refresh-evidence,
with no clone leg by design. `adr-check` becomes the uniform binary
resolver (`ADROIT_BIN` → sibling release/debug build → PATH →
`.como/tools` cached install, fresh installs only when `COMO_GIT_BASE` is
set and `COMO_OFFLINE` isn't → skip-with-notice). Pin bumps to the `PINS`
table are deliberate, reviewed edits — like conduit's `adroit.rev`.

### Positive Consequences

- The truthfulness gate runs (or skips honestly, naming the remedy) from
  any checkout layout, not just the canonical workspace.
- Every assertion group's evidence now states what it validated — the
  silent "sibling on a side branch" hole is closed by the printed
  resolved-source line and its pin-drift flag.
- A single-repo portfolio checkout can arm the full gate by setting
  `COMO_GIT_BASE` (or per-repo `COMO_<REPO>_DIR`), with the cache pinned
  to declared revs.
- The clone leg is read-only and credential-free by construction: no
  `git push`, no `git remote add`, no tokens in URLs.

### Negative Consequences

- The resolver is deliberately copied per repo; copies can drift in order
  or notice text — this ADR's canonical chain is the reference, and
  verify-claims is the natural place for a future conformance check.
- A present sibling wins over the pin, so local runs may validate
  different code than a pinned CI clone; the printed resolve line is the
  only drift signal and reviewers must read it.
- The `PINS` table is one more thing to bump deliberately; a forgotten
  bump validates yesterday's contract (loudly, via the drift flag, but
  still).
- The clone legs for then-unpublished repos ship untested against a real
  remote (none exists yet); they are exercised only via local `file://`
  mirrors until the owner pushes those repos.

## Implementation

Landed with this ADR on the `dep-resolution` branch: the `resolve_repo` /
`resolve_docs` / `repo_binary` helpers and `PINS` table in
`scripts/verify-claims` with every sibling read routed through them;
`scripts/refresh-evidence` honoring `COMO_DOCS_DIR` with the local-only
policy stated in its error; the justfile `adr-check` uniform resolver;
`.como/` gitignored; and the truthfulness page documenting how the gate
resolves each sibling. The clone-cache leg is verified by a simulated run
against local bare mirrors (`COMO_GIT_BASE=file://…`), since the gates
themselves never touch the network.

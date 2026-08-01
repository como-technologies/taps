# ADR-0006: Adopt llm-wiki-engine (Como fork) as the knowledge-base substrate

> State: Accepted

## Status

Accepted

## Stakeholders

Portfolio owner (decides; steward of the fork); sibling-tool maintainers
(adroit, tuesday, pulse become structured writers against the KB spec); the
future librarian agent layer (consumes the spec's seams); prospective client
readers (the KB is what makes the prescriptions evidenced).

## Context and Problem Statement

The portfolio has no knowledge base: the prescriptions are unevidenced, decisions
and their sources live in disconnected repos, and nothing mechanical stops
knowledge from rotting (portfolio issue #3). Issue #3 specified the need — a
typed, linked, lint-gated KB with a sources layer — and issue #4 posed the
adopt-vs-adapt-vs-build decision (c) plus the identity one-way-door (f). A
dedicated spike ([kb-spike](https://github.com/como-technologies/kb-spike))
evaluated `llm-wiki-engine` as the implementation base: ten findings issues,
each closed with a live-verified, tracked finding.

## Decision Drivers

- **One substrate, not two** (issue #3 §4): tools are heads over the KB, not
  parallel corpora that sync.
- **Anti-rot must be mechanical, not aspirational**: schema violations and
  broken links must fail commands CI can gate on.
- **The identity one-way-door** (issue #4 (f)): page identity must survive
  reorganization or the KB can never be safely restructured.
- **The durable asset is the spec, not the tool** (issue #3 §5): whatever is
  chosen must be replaceable behind the spec's contracts.
- Engine defects must be fixable on our timeline, not upstream's.

## Considered Options

- **Spec-only** — write the KB spec, pin no implementation base yet.
- **Stock `llm-wiki-engine`** (crates.io v0.4.1) — adopt unmodified.
- **Adapt: a Como fork of `llm-wiki-engine`** — carry a small, generic patch
  series; keep upstream engagement optional.
- **Build a bespoke KB substrate.**

## Decision Outcome

Chosen: **the Como fork** ([`como-technologies/llm-wiki`](https://github.com/como-technologies/llm-wiki),
branch `como-main`), together with **adoption of the
[Como KB specification](https://github.com/como-technologies/llm-wiki/blob/main/docs/specifications/como-kb-spec.md)**
(now homed with the KB product) produced by the spike. The spike's
verdict is GO, on evidence:

- **Stock was disqualified on the one-way-door**: identity is the file path;
  a `git mv` breaks every inbound link (verified live). The fork's stable
  ULID identity closes it — the same repro passes with zero link rewrites
  ([findings/issue-01](https://github.com/como-technologies/kb-spike/blob/main/findings/issue-01-identity-and-links.md)).
- **The CI gate is real**: under strict validation every violation class
  fails `ingest` with exit 1 and a named rule; `lint` fails on errors and
  passes warnings
  ([findings/issue-02](https://github.com/como-technologies/kb-spike/blob/main/findings/issue-02-schema-strictness.md)).
- **Our semantics ride cleanly on it**: custom `decision` schema with the
  adroit-aligned status lifecycle, schema-enforced `superseded ⇒
  superseded_by`, configurable search weighting
  ([findings/issue-03](https://github.com/como-technologies/kb-spike/blob/main/findings/issue-03-status-semantics.md),
  [-04](https://github.com/como-technologies/kb-spike/blob/main/findings/issue-04-staleness-lint.md)).
- **The admission model runs today**: commit-as-admission with pre-commit
  strict validation and post-commit indexing works with two git hooks and one
  config flag — no engine changes
  ([findings/issue-09](https://github.com/como-technologies/kb-spike/blob/main/findings/issue-09-admission-verification.md)).
- **The read seam serves adroit**: full-fidelity reads by ULID over CLI and
  MCP; one gap (export drops custom frontmatter) filed and non-blocking
  ([findings/issue-10](https://github.com/como-technologies/kb-spike/blob/main/findings/issue-10-read-seam.md)).
- **The fork is fixable on our timeline, demonstrated**: six engine defects
  found by the spike were fixed on `como-main` the same day (583 tests
  passing), each re-verified live in the spike.

Spec-only postpones the one decision the spike existed to make; bespoke
rebuilds a substrate whose hard parts (index, lint, MCP seam) the engine
already does well; stock fails the one-way-door. The fork's patch series is
deliberately **generic engine improvements only** (stable ids, fail-loud
registry, split confidence semantics, citations, cursors) — nothing
Como-specific lives in the engine, which is what keeps it replaceable behind
the spec and upstreamable if engagement ever resumes.

### Supersedes ADR-0005 (spec-only entry)

An earlier bounded spike (evidence page since removed; in git history) reached
ADR-0005 (since removed; in git history):
spec-only, no substrate dependency — the right call against stock 0.4.1
before the spec existed, and its own text scheduled this revisit. Its
disqualifiers are each retired above; two of its findings carry forward:

- **adroit's write path silently destroys unknown frontmatter keys**
  ([adroit#28](https://github.com/como-technologies/adroit/issues/28)) —
  until fixed, `docs/src/adr` remains the sole adroit-writable home of
  decisions and any KB-resident copy is read-only projection. This gates
  the corpus port, not this decision.
- **conduit's frozen read slice ran green over a KB-resident corpus** —
  complementary evidence for the read-seam contract (§ read contract).

### Consequences

- **Three layers, three homes.** The *fork* stays an engine
  (`como-main` = upstream + generic patches; `feat/stable-page-identity`
  remains the clean upstream-PR candidate). The *KB itself* is a new repo —
  the production space (`wiki/`, `evidence/`, `schemas/`), hooks, weights,
  and librarian policy, provisioned by a kb-setup descendant that installs
  the engine from the fork. The *heads* (adroit, tuesday, pulse, the
  librarian) write typed pages and read the seams.
- **Fork stewardship is a standing commitment**: track upstream when it
  revives (its `main` is currently broken), keep patches generic, upstream
  what is accepted. The spike demonstrated the payoff side of this trade.
- **Tool retrofits**: ADRs gain YAML frontmatter as the machine seam; adroit
  reads/writes it and keeps numbering, review-due, and plan extraction
  head-side; tuesday and pulse register their artifact page types.
- **The ADR corpus eventually lives in the KB space** (one-substrate rule),
  with this book's copy becoming a projection.
- Open engine work
  ([llm-wiki#8–#13](https://github.com/como-technologies/llm-wiki/issues))
  proceeds in parallel; none of it blocks adopting the spec.
- kb-spike is archived as the evidence trail; its findings are the citations
  behind every claim above.

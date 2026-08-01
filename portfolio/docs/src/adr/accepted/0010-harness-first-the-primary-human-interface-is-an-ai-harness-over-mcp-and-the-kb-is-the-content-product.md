# ADR-0010: Harness-first — the primary human interface is an AI harness over MCP, and the KB is the content product

> State: Accepted

## Status

Accepted

## Stakeholders

Portfolio owner (decides; drives the plan of record); tool maintainers
(adroit, tuesday, pulse — MCP surfaces and typed page emission become
first-class work); the template product owner (it retires to starter
content, staged); prospective clients (bring their own harness and
model); services (the engagement shape follows the interface).

## Context and Problem Statement

The KB workstream made the substrate real: llm-wiki is the KB product
([ADR-0006](./0006-adopt-llm-wiki-engine-como-fork-as-the-knowledge-base-substrate.md),
[ADR-0008](./0008-build-the-kb-product-in-the-fork-itself-developing-on-main.md)),
instances are ephemeral and built from source at HEAD
([ADR-0009](./0009-iterate-ephemeral-first-disposable-kb-instances-built-from-source-at-head.md)),
and adroit operates exclusively against KB spaces (adroit ADR-0020).
But the content-product story still assumed teams clone a template
repository and hand-maintain markdown. Reviewing the Prescribe narrative
exposed the seams in that story: a client cloning the template gets no
KB at all; guides have no KB representation (only decisions have a
writer); and agents can only know content that reaches a space. The
deeper problem is the premise. Given how content authoring now actually
happens, teams will not hand-write, hand-organize, and hand-update a
cloned doc repo — they will work with an LLM and expect the output to
land where it belongs, in the right shape. The hand-maintained doc repo
is the rotting artifact this portfolio exists to replace.
[portfolio#7](https://github.com/como-technologies/portfolio/issues/7)
records the ratified plan.

## Decision Drivers

- **One substrate** (ADR-0006): a clonable content product beside the KB
  is exactly the parallel-corpora drift the KB was adopted to prevent.
- **The engine was built for this**: llm-wiki is headless and
  agent-facing — 23 MCP tools, ACP workflows, no LLM inside — with
  deterministic gates designed around outside models.
- **The admission model already assumes untrusted authors**: strict
  schema validation fails the commit; pages are born `generated` with
  low confidence and promoted on review; propose-then-verify with a
  deterministic verifier, never another model.
- **BYOx**: clients bring their own harness and model, mirroring
  conduit's forge/model/cloud neutrality.
- The gap between the decision and reality is thin — instructions and
  seams, not an application.

## Considered Options

1. **Status quo** — the template remains the clonable content product
   beside the KB; the book labels the disconnect honestly.
2. **Repo-cloning UX, KB-backed** — the template stands up a space on
   init, guides ported to typed pages in-repo; cloning remains the
   human workflow.
3. **Harness-first** — the primary human interface is the user's own AI
   harness configured with Como's tools over MCP; content is born in
   the KB; the template product retires to starter content.

## Decision Outcome

Chosen: **harness-first (option 3).** Concretely:

- **The primary human UI is the harness.** Humans work in the AI
  harness of their choice (Claude Code, Claude Desktop, any MCP-capable
  client) configured with Como's MCP surfaces — llm-wiki's server and
  adroit's projection. CLI tools remain first-class for humans and
  local harnesses, and native surfaces (adroit's TUI and web dashboard)
  remain for practitioners who prefer them: alternate doors to the same
  substrate, none deprecated.
- **The KB is the content product.** No repo-cloning content workflow
  is offered. Content classes (`guide`, `glossary-entry`,
  `worked-example`) are authored conversationally through the harness
  and land behind the admission gates; `decision` pages are authored
  only via adroit — the substrate-vs-head boundary
  (kb-spec §6) stands unchanged.
- **The authoring contract**: agent-authored pages are born `generated`
  with low confidence and are promoted on human review; citations are
  pinned `path@commit`; humans hold acceptance. The gates check shape
  deterministically; truth stays a human judgment.
- **The template product retires, staged.** Its repo re-scopes to
  a starter-content corpus in its own wave (superseding its ADR-0014
  there); the demo kit and this book's truthfulness pins move in
  lockstep; the self-serve badge retires with the product until the
  harness-first offering earns a rung on its own evidence.
- **The librarian is unchanged and remains future state.** This
  decision covers human-prompted authoring; background curation stays
  specified-not-built (kb-spec Part I). Both run behind the same gates.
- **Plan of record**:
  [portfolio#7](https://github.com/como-technologies/portfolio/issues/7)
  (waves 0–5). This record is wave 0.

### Positive Consequences

- One content story — the two-products ambiguity is resolved instead of
  documented around.
- The "loop in end-users' hands" future state becomes a concrete,
  planned trajectory: the end-user story *is* their harness against
  their KB.
- The missing piece is a thin authoring kit, not a new application —
  converge, don't accumulate.
- BYO-harness widens the funnel: Como ships seams and instructions,
  not a chat product to maintain.
- Conversational authoring exercises the KB machinery continuously,
  the same way the ephemeral gates exercise provisioning.

### Negative Consequences

- The portfolio loses its only self-serve rung at the retirement wave
  until re-earned — accepted; the ladder stays honest.
- adroit's read-only MCP scope (its ADR-0015) must be superseded for
  MCP-only harnesses — a deliberately widened write surface that needs
  the same sanitizer, property, and fuzz coverage as the CLI path.
- Schema gates cannot catch confident nonsense; the confidence flow and
  citation discipline carry more load, and the authoring kit's
  instructions must enforce them.
- The human-readable projection (a site to *read*) is deliberately
  unresolved until someone needs it; review happens through the harness
  and the existing books meanwhile.

## Implementation

Wave 0 (this record): the book truth-syncs in the same commit —
introduction (the AI-through-the-loop paragraph, the Prescribe loop
item, One substrate under the loop), roadmap (Where this is going),
products overview (the template product marked retiring to starter content),
and the Knowledge base authoring service page — all in honest tense:
the decision is present, the authoring kit is in flight, the librarian
stays future. Waves 1–5 per portfolio#7: the authoring kit lands in
llm-wiki (the fork is the product, ADR-0008); adroit's guarded MCP
write slice supersedes its ADR-0015 in its own corpus; the template
retirement re-scopes its repo and retargets this book's pins; tuesday
and pulse emit typed report pages; operating.md gains the harness ring.
Suite rules apply throughout: Rust or shell only, trunk-based waves,
pre-GA schema freedom, and the banned-terms scans keep client names out
of published material.

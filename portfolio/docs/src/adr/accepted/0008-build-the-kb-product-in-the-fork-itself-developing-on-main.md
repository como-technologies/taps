# ADR-0008: Build the KB product in the fork itself, developing on main

> State: Accepted

## Status

Accepted

## Stakeholders

Portfolio owner (one less repo to steward); KB contributors (one codebase,
one branch); head maintainers (adroit, tuesday, pulse, the librarian —
unchanged seams); clients (ship the `llm-wiki` binary + tagged releases).

## Context and Problem Statement

ADR-0007 (since removed; in git history)
introduced lore, a product layer between the engine fork and KB instances,
to preserve the fork's upstream purity (generic patches only, a clean
upstream-PR branch) while giving Como-specific assets a shippable home.
Before any lore code existed, the premise was re-examined: upstream
`llm-wiki-engine` is a young single-maintainer crate whose `main` does not
currently build, upstream engagement was already deferred indefinitely, and
the purity discipline (patch routing, a frozen PR-candidate branch, a
two-hop fork→lore→instance upgrade path) is standing overhead purchasing an
option we may never exercise. Meanwhile the suite's working reality is
simpler: we control all the code, side by side, and change it at will.

## Decision Drivers

- Simplicity: fewest repos, branches, and hops between "decide" and "ship".
- The upstream option's real value is low (broken build, deferred
  engagement) and its carrying cost is daily.
- The KB spec's contracts — not repo topology — are what keep the substrate
  replaceable (ADR-0005's negative-consequence note, still true).
- Pinning must survive without a distro layer.

## Considered Options

1. **Keep the lore plan** (ADR-0007) — product repo over a pure fork.
2. **Build the product in the fork, single `main`** — lore's deliverables
   become engine features; upstream optionality retired to cherry-picks.
3. Spec-only retreat (re-litigating ADR-0005/0006) — not on the table.

## Decision Outcome

Chosen: **the fork is the product.** `como-technologies/llm-wiki` is the
Como KB codebase: features land on `main`, the only branch. Consequences,
concretely executed with this decision:

- `como-main` was fast-forwarded into `main` (build + 583 tests green) and
  both `como-main` and `feat/stable-page-identity` are deleted; PR
  llm-wiki#1 closed as merged. The generic-vs-Como patch routing rule is
  retired: Como-specific schemas, provisioning, and config are engine
  features now.
- **Pinning = release tags on the fork.** `v0.5.0` (upstream 0.4.1 + stable
  ULID identity + the spike-driven hardening) is the first; instances and
  CI pin tags, and an upgrade is a tag bump.
- The `upstream` git remote stays configured for opportunistic
  cherry-picks; no discipline is owed to it.
- The architecture is **three layers**: `llm-wiki` (the product) → KB
  instances (near-pure data spaces it creates and manages) → heads
  (adroit, tuesday, pulse, the librarian) over the spec's seams.
- lore's planned deliverables move into the fork's backlog (provisioning
  with hooks; the schema library incl. the `decision` type; the instance
  CI template) alongside the existing engine issues.

This supersedes ADR-0007 (which itself refined ADR-0006's consequences;
0006's substrate decision and the KB spec's contracts are unchanged).

### Positive Consequences

- One repo, one branch, one release stream from decision to shipped binary.
- No routing judgment calls per change; no two-hop upgrades.
- The engine's own machinery (embedded schemas, `spaces create`, config)
  is the natural home for what lore would have re-wrapped.

### Negative Consequences

- Upstream divergence is permanent in practice; any future re-engagement
  means extracting patches from a mixed history (cherry-pick archaeology).
- Como-specific and generic concerns now share a codebase — the spec's
  substrate-neutrality (ADR-0006) is the remaining guard against the KB
  becoming unreplaceable.
- The archived kb-spike's tooling pins the deleted `como-main` branch and
  its README names lore — accepted as fossils; this record is the pointer.

## Implementation

Fork consolidated and tagged (above). Portfolio: kb-spec §8 rewritten to
the three layers; portfolio#5 (create lore) closed superseded; portfolio#6
(Como's instance) retargeted at `llm-wiki` directly; provisioning/schema
issues filed on the fork. adroit#28 remains the writability gate for the
instance corpus.

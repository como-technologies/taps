# ADR-0012: Collapse the suite into a single Cargo workspace in a fresh repo

> State: Accepted

## Status

Accepted

## Stakeholders

Suite owner (executes the move); every sibling-app corpus (their repo
boundaries dissolve); llm-wiki (changes ownership posture).

## Context and Problem Statement

Seven sibling repos are held coherent by hand-built machinery — the
uniform resolution convention (ADR-0004 and its per-repo twins), pinned
revisions, duplicated resolver shell, and an operating-ring dance that
exists largely to verify the repos still agree. Since ADR-0011 retired
the scripted truthfulness gate, cross-repo seams have no mechanical
protection at all, and two have already drifted. The full evidence and
the migration plan of record live in
[portfolio#8](https://github.com/como-technologies/portfolio/issues/8).

## Decision Outcome

The suite becomes **one multi-crate Cargo workspace** in a **fresh,
blank repo**; the seven existing repos are copied in at recorded SHAs
and never modified — rollback is deleting the new repo.

- **All seven products come in, including llm-wiki.** Shared library
  seams (e.g. decision-page validation used by both adroit and the KB
  engine) are expected once a crate boundary exists to share across.
- **Como breaks with the llm-wiki-engine upstream entirely** and
  maintains the whole codebase. Attribution is retained (dual
  MIT/Apache-2.0 license files, upstream copyright, README credit);
  every other upstream reference — package identity, schema `$id`
  namespace, installers, release machinery, inherited community files —
  is removed. This replaces ADR-0008's cherry-pick posture.
- **Truth by construction**: path dependencies and a shared contract
  crate replace pins, resolvers, and hand-copied constants.
- Multi-repo features of the loop tools are exercised via dedicated
  test projects, since the suite's own topology no longer does.

ADR-0004 and its per-repo twins remain operative for the existing repos
and retire at cutover, when development moves to the workspace.

### Positive Consequences

- Seam drift becomes a compile error, not a discipline.
- One cold clone, one `just ci`; the rings 1–2 dance and `cold-sim`
  (the suite's last CI-orbit Python) go away.

### Negative Consequences

- One workspace build is heavier than seven cached ones; accepted
  pre-GA.
- Upstream llm-wiki improvements now arrive only by manual port.

## Implementation

Executed as the wave plan in
[portfolio#8](https://github.com/como-technologies/portfolio/issues/8);
that issue closes when a cold clone of the workspace passes full parity.

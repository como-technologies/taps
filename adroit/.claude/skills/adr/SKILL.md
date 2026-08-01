---
name: adr
description: Use when working ON adroit and making an architectural decision worth recording — choosing a dependency, a module seam, an on-disk format, an async/sync boundary, etc. Invoke for "should we record an ADR for this", "draft an ADR for <decision>", "accept/supersede adroit ADR N". Records the decision in adroit's OWN `docs/src/adr/` corpus and validates it by seeding an ephemeral KB space (dogfooding).
user-invocable: true
---

# Record adroit's own architecture decisions (in `docs/src/adr/`)

adroit dogfoods itself: its architecture decisions live in the **`docs/src/adr/`**
corpus — the committed **legacy-format** corpus (markdown / by-status, the
suite's uniform corpus path). Since ADR-0020 the live tool operates only
against a KB space, so validation goes through `adroit seed` into an ephemeral
space; the committed files stay canonical.

## Critical: never write into `ADROIT_DIR`
`ADROIT_DIR` in this workspace points at the **dogfood target repo**, not
adroit's own decisions. Every adroit command here takes an explicit `--dir`
(a seeded ephemeral space — see below). A bare `adroit new` would write into
the dogfood repo by mistake.

## When to record one
A decision belongs in `docs/src/adr/` when it shapes adroit's architecture and a
future contributor would ask "why is it this way?": a new dependency (e.g. ADR-0002
adopting rig), a sync/async boundary, a seam design, an on-disk format change, a
feature-gating choice. Routine bug fixes and refactors don't.

## Flow
1. Author `docs/src/adr/proposed/NNNN-short-imperative-title.md` in the legacy
   shape (next free number): `# ADR-NNNN: Title` H1, `> State: Proposed`
   banner, a `## Status` region with `Proposed` + `Created: YYYY-MM-DD`, then
   the MADR sections.
2. Fill **Context / Decision Drivers / Considered Options / Decision Outcome**
   (with honest **Negative Consequences**) and **Implementation**. Match the house
   voice of the existing ADRs.
3. Validate by seeding an ephemeral space:
   ```sh
   just build
   tmp=$(mktemp -d)
   printf 'name = "adroit-adrs"\n' > $tmp/wiki.toml
   mkdir -p $tmp/wiki/decisions
   ./target/debug/adroit seed --from docs/src/adr --dir $tmp
   ./target/debug/adroit check --dir $tmp   # must be clean
   ./target/debug/adroit lint NNNN --dir $tmp
   ```
   (This is exactly what the `just adr-check` CI gate runs.)
4. Once decided, move the file to `accepted/` and update the banner +
   `## Status` region to `Accepted` — a real status transition, so the corpus
   history is genuine. For a supersession, set the old ADR's region to
   `Superseded by [ADR-NNNN](../accepted/NNNN-….md)` and move it to
   `superseded/`; re-run step 3.

## Keep it generic
These ADRs are public. **Never** name a client, their internal tech, or titles
(see the project's "no client names" rule) — keep examples generic, even when the
decision was surfaced while dogfooding against a client repo.

## Don't fight the existing doc rule
ADRs in `docs/src/adr/` are decision records, deliberately **separate** from the
mdbook user manual's chapters (the corpus sits under `docs/src/` only so the suite
keeps one uniform corpus path; it is not wired into `SUMMARY.md`). Don't migrate
the corpus into book chapters.

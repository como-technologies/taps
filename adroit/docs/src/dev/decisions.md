# Decision Records (docs/src/adr/)

adroit dogfoods itself: its own architecture decisions live in the
**`docs/src/adr/`** corpus (the committed **legacy-format** corpus — markdown /
by-status, sequential numbering, the suite's uniform corpus path), authored
with the `adroit` binary — never by hand-editing status or identity. Since
ADR-0020 the live tool operates only against a KB space, so working with this
corpus means **seeding an ephemeral space from it** first; the committed files
stay canonical. The corpus is the record of *why the code is
the way it is*: the AI provider seam, the statelessness invariant, the manifest
semantics table, the MCP read-only projection, and the standing direction
decisions all live there as accepted ADRs (ADR-0001 records why the corpus
itself exists — including the earlier, deliberate decision to *remove* it,
which the current portfolio-wide "every repo carries its own corpus" mandate
reversed).

## Bootstrap a space, then work in it

`--dir` must name a KB space, and `docs/src/adr` is a legacy corpus — so stand
up a throwaway space and seed it (`ADROIT_DIR` in this workspace points at an
**external dogfood target repo**, never at adroit's own decisions, so always
pass `--dir` explicitly):

```sh
just build
tmp=$(mktemp -d)
printf 'name = "adroit-adrs"\n' > $tmp/wiki.toml
mkdir -p $tmp/wiki/decisions
./target/debug/adroit seed --from docs/src/adr --dir $tmp
./target/debug/adroit list --dir $tmp
./target/debug/adroit check --dir $tmp
```

New decision records are still **authored into the committed corpus** at
`docs/src/adr/` (its legacy shape is hand-maintainable: `proposed/NNNN-….md`
with an `# ADR-NNNN:` H1 and a `## Status` region), and validated by seeding a
fresh space as above.

## CI gate (self-hosted)

The workspace-root `just ci` includes the **`adr-check`** recipe — the
bootstrap pattern: build the in-tree adroit, create an ephemeral space,
`seed` it from `docs/src/adr`, and `check` the seeded space —

```sh
just adr-check   # from the workspace root
```

— so a structurally broken corpus (an unparseable document, duplicate
identifiers, broken links or supersession) fails CI the same way a failing
test does. New ADRs should also pass `adroit lint <N> --dir $tmp` clean in the
seeded space (no prompt-only sections, honest negative consequences, at least
two considered options).

## Recording a decision

The [`/adr` skill](./skills.md#adr) is the worked flow. In short:

1. Add `docs/src/adr/proposed/NNNN-short-imperative-title.md` in the legacy
   shape (next free number; `# ADR-NNNN: Title` H1, `> State: Proposed`
   banner, `## Status` region with `Proposed` + `Created: YYYY-MM-DD`).
2. Fill **Context / Decision Drivers / Considered Options / Decision Outcome**
   (with honest **Negative Consequences**) in the house voice of the existing
   ADRs.
3. Seed an ephemeral space and run `check` + `lint <N>` against it (as above)
   — both must be clean.
4. Move the file to `accepted/` (updating banner + `## Status`) once decided —
   a real status transition, so the corpus history is genuine.

Keep the records **generic**: they are public — never name a client, their
internal tech, or titles, even when the decision surfaced while dogfooding
against a client-shaped repo.

## Deliberately separate from the manual's chapters

`docs/src/adr/` holds *decision records*; the book's chapters are the *user
manual*. They serve different readers and change for different reasons — the
corpus sits under `docs/src/` only so every repo in the suite keeps its ADRs at
the same path, and it is not wired into `SUMMARY.md`. Don't migrate ADRs into
book prose, and don't record decisions only in book prose. When a decision
changes behavior, do both: record the ADR **and** doc-sync the affected book
pages.

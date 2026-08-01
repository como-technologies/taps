# ADR-0020: Operate exclusively against a KB space

> State: Accepted

## Status

Accepted
Created: 2026-07-28

## Stakeholders

Suite owner (decided 2026-07-27: tools assume the KB — "create a new KB, point
adroit at it, it works"); adroit maintainers (one storage model instead of a
profile × layout matrix); consuming repos (assessments, tuesday, pulse,
portfolio — their `adr-check` gates change); the Como KB substrate
(`llm-wiki` — adroit becomes its first head).

## Context and Problem Statement

adroit today reads and writes ADRs at a plain directory (`--dir` /
`ADROIT_DIR` / `config.dir`, suite convention `docs/src/adr`), across two
format profiles (markdown, frontmatter) and three layouts (by_status, flat,
by_category). The Como KB workstream (portfolio ADR-0006/0008/0009) makes the
`llm-wiki` fork the KB product and adroit its first head: decision records
become typed pages in a KB space — a git repo with `wiki.toml`, a `wiki/`
content tree, a shipped `decision` JSON Schema, strict frontmatter validation,
and git-hook admission, provisioned in one command (`spaces create`,
llm-wiki 14). The suite direction is ephemeral-first: spaces are disposable,
stood up per-checkout, never a persistent store; committed corpus files in
each repo stay canonical and gates bootstrap a space from them.

Supporting both worlds — path-mode directories and KB spaces — doubles every
seam this repo has spent nineteen ADRs keeping singular: two location
resolutions, two admission models, two status representations (directory vs
frontmatter), two identity stories. The repo's own invariant says it plainly:
converge, don't accumulate. The owner's decision (2026-07-27) settles the
direction; this record settles the shape.

Concrete frictions the retrofit must resolve, found by inspection:

- adroit serializes `status:` in PascalCase (`Proposed`); the KB `decision`
  schema requires lowercase (`proposed`) — a naive write fails strict ingest.
- The frontmatter profile requires a persisted `number:`; the schema has no
  `number` — it has `id` (a tool-generated ULID) and `reference` (the
  head-owned display identity the schema says adroit allocates and writes).
- `Adr.id` is a UUID v4 that is never persisted; KB page identity is a
  persisted ULID.
- Status-by-directory (by_status), the status-move relink machinery, and the
  profile-mismatch guard all assume shapes a flat `wiki/` tree doesn't have.

## Decision Drivers

- One storage model; retire parallel paths rather than keep compatibility
  shims (repo invariant: converge, don't accumulate; simplicity first).
- The KB contract is already written and shipped: kb-spec Part II, the
  `decision` schema, strict admission, `spaces create` provisioning.
- Frontmatter as the single source of truth for machine-owned fields — no
  body/frontmatter dual representation to drift.
- Ephemeral-first: a fresh space plus a seed must be cheap, deterministic,
  and testable; nothing may depend on a long-lived instance.
- Foreign-key preservation (issue 28) already lands writes safely on pages
  carrying KB-owned keys — the write path is proven.
- Every consuming repo's gate keeps working through a staged migration.

## Considered Options

1. **KB-only** — adroit assumes a KB space; path-mode operation, the
   markdown profile, and status-directory layouts retire from live
   operation. Legacy corpora enter through an explicit bootstrap.
2. **Dual-mode** — keep path mode and add space mode behind a flag or
   auto-detection. Every future feature pays the matrix tax (2 modes ×
   formats × layouts); the profile-mismatch guard grows a third axis; the
   test oracle doubles.
3. **Storage trait** — abstract corpus I/O behind a `CorpusBackend` trait
   with path and KB impls. Architecture for a second substrate nobody has
   asked for; kb-spec's substrate-neutral contracts are the real
   replaceability guard, not repo-internal indirection.

## Decision Outcome

Chosen: **KB-only (option 1)**, because the owner has decided the direction
and the repo's own invariants forbid carrying the parallel path.

The shape, concretely:

- **The corpus is a KB space.** `--dir` / `ADROIT_DIR` / `config.dir` keep
  their names but now name the **space root** (the directory holding
  `wiki.toml`). Decision pages live at `<wiki_root>/decisions/` (default
  `wiki/decisions/`, honoring `wiki.toml`'s `wiki_root`). A directory
  without `wiki.toml` is a hard error naming the bootstrap path — the
  missing-dir hard-error rule extends, never auto-creates.
- **One on-disk profile: the KB decision page.** YAML frontmatter is the
  source of truth for every machine-owned field; the body is prose only —
  **no `## Status` section**, no `> State:` banner, no `Created:` /
  `Review by:` status-region lines. Flat layout; status changes rewrite
  frontmatter in place; the by_status move-and-relink machinery, the
  markdown profile, and by_category retire from live operation. The
  markdown parser is retained only behind the legacy-corpus bootstrap.
- **Schema-aligned serialization.** `status:` serializes lowercase
  (`proposed | accepted | rejected | deprecated | superseded`), matching the
  `decision` schema enum; parsing stays case-insensitive. `-o json` output
  casing follows (a breaking surface change, accepted in this wave).
- **Identity per the KB contract.** `id:` is a persisted tool-generated
  ULID (page routing identity). `reference:` is a persisted head-owned
  display identity — adroit allocates it (sequential `ADR-NNNN` by default,
  max+1, the existing naming seam) and writes it; `number:` is no longer
  persisted, it is derived from `reference` where numeric verbs
  (`renumber`, `review`) need it. Typed link fields (`supersedes`,
  `superseded_by`, `relates_to`, `depends_on`, `refines`) carry ids
  (preferred) or slugs per the schema.
- **Admission is the space's.** adroit stays tool-as-committer: it writes
  pages and commits; the space's pre-commit hook (strict `ingest
  --dry-run`) is the deterministic admission gate. `adroit check` remains
  the semantic gate (supersession integrity, duplicate identifiers, link
  health) — the two gates are complementary, not redundant.
- **Bootstrap, not migration-in-place.** A legacy corpus enters a fresh
  space via an explicit seed (`llm-wiki spaces create` + adroit ingesting
  the old files, mapping H1/status-directory/`## Status` into frontmatter).
  Repos keep their committed corpora canonical; their `adr-check` gates
  bootstrap an ephemeral space and run `adroit check --dir <space>` —
  staged repo-by-repo in later waves.

### Positive Consequences

- One corpus shape: the profile × layout matrix, the profile-mismatch
  guard's inference, and the dual status representation all collapse.
- Strict schema validation (an entire class of malformed-corpus bugs)
  becomes the substrate's job, enforced at commit time.
- The KB's identity contract ends the number-collision ambiguity: `id` is
  globally unique by construction; `reference` collisions remain a `check`
  rule with `renumber` as the repair.
- Every other head (tuesday, pulse) inherits a proven pattern: this
  retrofit is the template.

### Negative Consequences

- **Breaking for every consumer.** All five repos' `adr-check --dir <path>`
  invocations, the shipped CI templates, `.env.example`, and the book's
  corpus-location pages break until each is moved to bootstrap-a-space.
  Staged waves keep them green by pinning the pre-retrofit adroit until
  each repo migrates.
- adroit's own `docs/src/adr/` corpus (this file included) is markdown /
  by_status — the dogfood gate itself must move to the bootstrap pattern in
  the same change that removes path mode, and the corpus files migrate to
  the KB page shape.
- History-derived lifecycle (status timeline from directory renames)
  degrades: in a flat tree, status history must come from content diffs or
  is absent going forward.
- The test oracle (`tests/model.rs`) and much of `tests/cli.rs` assume
  path-mode corpora; the harness needs a space-fixture helper and a sweep.
- `-o json` casing change (`"Accepted"` → `"accepted"`) breaks any consumer
  parsing status strings; the manifest schema version must signal it.

## Implementation

Staged inside kb/wave-2 (adroit-side), later waves (consumers):

1. Space resolver: `resolve_dir` result must contain `wiki.toml`; pages at
   `<wiki_root>/decisions/`; hard error otherwise. MCP env re-export and
   `init`'s `.env` write carry the space root.
2. KB page profile: lowercase `status` serde; persisted `id` (ULID) +
   `reference`; drop `number:`; body-only serialization (no status region);
   foreign keys keep round-tripping via `extra`.
3. Retire: markdown profile and by_status/by_category from live operation;
   profile-mismatch guard becomes space-shape validation; `migrate` becomes
   the legacy-corpus seed into a space.
4. Test harness: TempDir space fixture (wiki.toml + wiki/decisions +
   decision schema); oracle and CLI suites swept to it; fixture corpus
   shipped as adroit's test data with a one-command seed.
5. Docs sweep: your-repo, quick-start, adr-format, cli, decisions,
   ci-integration pages; CLAUDE.md; CI templates.
6. Later waves: each consuming repo's adr-check leg moves to
   bootstrap-a-space; portfolio operating.md documents the KB-backed DX.

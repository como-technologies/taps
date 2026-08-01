# ADR Format

adroit has **one on-disk format: the KB decision page** (ADR-0020) — YAML
frontmatter over a prose body, one page per decision, in a **flat**
`decisions/` directory inside a KB space. Frontmatter is the single source of
truth for every machine-owned field; the body is prose only (no H1, no
`> State:` banner, no `## Status` section).

The corpus lives in a **KB space**: `--dir` / `ADROIT_DIR` / `config.dir` name
the space root — a directory carrying a `wiki.toml` — and pages live at
`<wiki_root>/decisions/` (default `wiki/decisions/`, honoring the space's
configured `wiki_root`). A directory without a `wiki.toml` is a hard error
naming the bootstrap path; see [Your repo](../usage/your-repo.md). A pre-KB
corpus enters a fresh space via [`adroit seed`](#seeding-a-legacy-corpus).

## The decision page

```markdown
---
id: 01K1B2C3D4E5F6G7H8J9K0M1N2
title: Use PostgreSQL for primary datastore
reference: ADR-0001
status: proposed
created: 2026-04-15T10:30:00Z
type: decision
---

## Context and Problem Statement

...
```

This is the same frontmatter the Como KB substrate's `decision` schema
validates, so a page adroit writes into a KB space passes strict admission
as-is.

| Field | Type | Description |
|---|---|---|
| `id` | ULID | Canonical unique identifier (26-char Crockford), generated on creation |
| `reference` | string | Head-owned display identity — `ADR-NNNN` for numeric schemes, the bare slug otherwise; assigned on write |
| `title` | string | Short title describing the decision |
| `type` | string | Stamped `decision` by adroit; preserved verbatim if the page already carries one |
| `status` | enum | One of: proposed, accepted, rejected, deprecated, superseded |
| `created` | RFC 3339 | UTC timestamp of creation |
| `supersedes` | ref | *(optional)* An older ADR this one supersedes (a number or a slug) |
| `superseded_by` | ref | *(optional)* The newer ADR that supersedes this one |
| `relates_to` | ref list | *(optional)* Typed relational links — see [Relationships](#relationships) |
| `depends_on` | ref list | *(optional)* |
| `refines` | ref list | *(optional)* |
| `review_by` | `YYYY-MM-DD` | *(optional)* Review deadline; flags review-due when past for a Proposed ADR |

Optional fields are only written when set, so existing files stay clean. A
*ref* is whatever identifies the target under the active naming scheme — a bare
number (`2`) for `sequential`, or a slug string (`20260601-adopt-x`) otherwise.

**Foreign keys are preserved.** Any frontmatter key adroit does not own —
e.g. the KB substrate's `citations:` on a page adroit shares with another
system — is captured on read and re-emitted verbatim (original order, after
adroit's own fields) by every rewrite: `set-status`, `supersede`, `link`,
`renumber`, and the rest. adroit never interprets these keys; it only
guarantees a write can't destroy them.

**Writes are minimal-diff.** A status change rewrites the `status:` line in
place (the file never moves — flat is the only layout); parsing an unchanged
page and writing it back is byte-identical.

### File naming

`NNNN-kebab-case-title.md` under the default `sequential` scheme, where `NNNN`
is the zero-padded, permanent number mirrored by the page's `reference:`.
`README.md` and `adr-template.md` in the decisions directory are skipped when
listing ADRs. [Naming schemes](#naming-schemes) covers the collision-free
`date` / `uuid` alternatives.

## Status values

- **Proposed** — the decision is under discussion
- **Accepted** — the decision is in effect
- **Rejected** — considered but not adopted, kept for historical context
- **Deprecated** — no longer recommended but not replaced
- **Superseded** — replaced by a newer ADR (`superseded_by:`)

## Relationships

ADRs relate to one another in a few ways, all of which feed the dashboard's
[relationship graph](../usage/web.md) (each relationship is a distinct,
colored edge) and `adroit show`:

- **Supersession** — `adroit supersede <new> <old>`, persisted as
  `supersedes` / `superseded_by` frontmatter refs. Directed: newer → older.
- **Typed relational links** — `relates_to`, `depends_on`, `refines`, set with
  [`adroit link`](./cli.md) (structured ref-list fields). `depends_on` and
  `refines` are directed; `relates_to` is non-directional.
- **Body links** — any relative markdown link from one ADR's body to another
  (`[…](./0002-x.md)`) shows as a generic *related* edge and is kept canonical
  by `adroit relink` (which repairs links after a `renumber` or out-of-band
  edits).

When more than one relationship exists between the same pair, the most specific
wins (supersession > typed link > plain body link), so the graph shows one edge
per pair.

## Review deadlines

A still-`Proposed` ADR may carry an optional `review_by:` deadline. Set it with
`adroit set-review <ID> <YYYY-MM-DD>` (`--clear` to remove). Once the date is on
or before today, the ADR is flagged review-due in `stats` and the web
dashboard.

Even with no explicit deadline, a still-`Proposed` ADR is flagged review-due
once it has been sitting (since its creation date) longer than
`review_overdue_days` (config; default 30, `0` disables) — so an aging backlog
surfaces on the dashboard on its own, without stamping each ADR with a
deadline.

## Naming schemes

How an ADR's **identifier and filename** are formed is configurable
(`naming` config / `ADROIT_NAMING` / `--naming`). The identity model is
abstracted behind a single seam, so each scheme is self-contained. Pick **one
for the repo's lifetime** — adroit does not rename existing ADRs when you change
the setting.

Every scheme persists its display identity as the page's `reference:` string.
The `uuid` scheme derives its slug from the page's ULID id.

| Scheme | Filename | Identity | Collisions |
|---|---|---|---|
| `sequential` (default) | `NNNN-title.md` | global number | possible across branches |
| `date` | `YYYYMMDD-title.md` | date slug | collision-free |
| `uuid` | `<uuid>-title.md` | a UUID | collision-free |

**`sequential`** — the classic zero-padded `NNNN`, human-friendly and sortable.
Its one weakness is **cross-branch collisions**: two branches that each create
`0009` conflict on merge. CI on the merged state plus serialized merges catches
this (see [CI integration](../usage/ci-integration.md)), and
[`adroit renumber`](./cli.md#adroit-renumber-old-new---file-path) resolves a
collision after the fact.

**`date`** (log4brains-style) — the filename carries `YYYYMMDD-title`, so two
people creating ADRs the same day on different branches never collide (a
same-day same-title clash is auto-suffixed `-2`, `-3`).

**`uuid`** — a persisted UUID guarantees uniqueness with zero coordination, at
the cost of human-friendliness. adroit displays a short `ADR-<prefix>` and lets
you address an ADR by any unique leading prefix of the UUID.

### Addressing and scheme-specific commands

Read/lifecycle commands — `show`, `status`, `edit`, `set-review`, and
`supersede` — take an `<ID>` resolved through the active scheme: a number for
`sequential` (e.g. `9` or `ADR-0009`), the filename slug for `date`, or a
unique UUID prefix for `uuid`. `renumber` and `review` are **numeric-only**
(their artifacts are a single global number — `sequential`); they error under
`date` / `uuid`.

## Dates come from git

adroit derives an ADR's **creation and last-modified dates from git history**
where it can — a fresh `git clone` resets every file's modification time, so
the filesystem can't tell you when an ADR was written. Resolution precedence
for the creation date, highest first:

1. **git** — the first commit that added the file (when the space is inside a
   git work tree and the file is tracked);
2. the page's authored `created:` frontmatter timestamp — rewrite-stable
   provenance on a corpus without git history (e.g. a freshly seeded ephemeral
   space).

You can control the source with `date_source` (config / `ADROIT_DATE_SOURCE` /
`--date-source`): `auto` (the adaptive default above), `git` (require git —
warns if history is unavailable or the clone is **shallow**, a common CI
footgun that makes creation dates wrong), or `filesystem` (never shell git).

## Templates

New ADRs are scaffolded from a template. The template supplies the **body
prose only** — identity, status, and dates live in frontmatter, so templates
carry no H1, banner, or `## Status` section. Built-ins are:

- **`madr`** (the default) — the [MADR](https://adr.github.io/madr/) sections,
  starting at `## Stakeholders`, each with an italic authoring prompt.
- **`nygard`** — Michael Nygard's original lightweight format
  ([Documenting Architecture Decisions](https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions)):
  Context / Decision / Consequences.

You can also point at a custom template file or a `templates_dir`, and if the
decisions directory contains an `adr-template.md` it is preferred
automatically. Placeholders: `{{heading}}` (the scheme's H1 — for custom
templates that want one), `{{number}}` (the bare identifier), `{{title}}`,
`{{date}}`, `{{status}}`.

## Seeding a legacy corpus

`adroit seed --from <legacy-dir> [--dry-run]` bootstraps a **pre-KB corpus**
into a fresh space — the one-way door for repos whose committed ADRs still use
the retired markdown profile (`# ADR-NNNN:` H1, `## Status` region, optional
`> State:` banner, optionally grouped into `proposed/ accepted/ rejected/
superseded/ deprecated/` status directories; a flat legacy directory is also
accepted, taking status from the `## Status` section).

The mapping, per document:

| Legacy | KB page |
|---|---|
| `# ADR-NNNN: Title` H1 | `reference: ADR-NNNN` + `title:` |
| status directory, else the `## Status` word | `status:` (lowercase) |
| `Supersedes` / `Superseded by` notes | `supersedes:` / `superseded_by:` refs |
| `Review by: YYYY-MM-DD` line | `review_by:` |
| `Created: YYYY-MM-DD` line | `created:` (midnight UTC) |
| body minus H1 / banner / `## Status` region | the page body |

`seed` **refuses a target space that already contains any ADR** — spaces are
ephemeral-first (a gate stands one up per run and seeds it from the committed
corpus), and the refusal makes seeding safe. It exits non-zero on a document
that fails to parse, carries no number, or collides with another number.

After writing, `seed` runs a full **relink**: cross-ADR links written for the
legacy by_status directories (`../accepted/NNNN-x.md` in prose) are healed to
the flat space. Links that leave the corpus (book pages, assets) are left
as-is — `check` reports them as advisory **external** links, never errors.

## References

The ADR formats adroit follows and templates from:

- [MADR](https://adr.github.io/madr/) — Markdown Any Decision Records; the basis
  for the default `madr` template's section structure.
- [Documenting Architecture Decisions](https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
  — Michael Nygard's original ADR post; the basis for the `nygard` template.
- [architecture-decision-record](https://github.com/joelparkerhenderson/architecture-decision-record)
  — a comprehensive collection of ADR templates and examples.
- [ADR = Any Decision Record?](https://ozimmer.ch/practices/2021/04/23/AnyDecisionRecords.html)
  — Olaf Zimmermann on broadening ADRs beyond architecture to any team decision.

# Managing ADRs

This guide covers the day-to-day workflow of creating and maintaining ADRs.
adroit operates against a **KB space** (`--dir` names the directory holding
`wiki.toml`); decision pages live flat under `wiki/decisions/`, with YAML
frontmatter as the source of truth for identity, status, and dates. See
[ADR Format](../reference/adr-format.md) for the page shape. For the bird's-eye
view of how these verbs sequence across a decision's life, see
[The ADR Workflow](./workflow.md).

## Creating an ADR

```sh
adroit new "Use PostgreSQL for primary datastore"
```

This assigns the next sequential number, scaffolds the page from a template
(prose sections only — identity and status are frontmatter), writes it into
`wiki/decisions/`, and opens it in your editor. Use `--no-edit` to skip the
editor and `--template <name|path>` to choose a template.

## Listing ADRs

```sh
adroit list
adroit list --status accepted
```

Lists ADRs sorted by number. `--status` filters.

## Viewing an ADR

```sh
adroit show 1
```

## Searching

```sh
adroit search postgres
```

Case-insensitive search over titles and bodies.

## Updating status

```sh
adroit set-status 1 accepted   # the setter
adroit status 1                # the getter — prints `accepted` (lowercase, scriptable)
```

`set-status` rewrites the page's `status:` frontmatter **in place** — the file
never moves, and every other byte is left identical. `status <ID>` is the
read-only counterpart — just the status word, lowercase, so it pipes cleanly;
`show` gives the full record.

## Concurrent contributors & branching

Because a status change rewrites exactly one file in place, two decision PRs on
two branches touch disjoint files and never produce a false merge conflict.
The one thing that still collides on merge is **numbering**:

**Duplicate numbers.** Two branches each running `adroit new` pick the same
`NNNN`. Keep sequential numbers and catch the collision at the merge queue with
`adroit check`, then resolve with `adroit renumber` — see
[CI Integration](./ci-integration.md#concurrent-adr-numbers-across-branches).

**Prefer to avoid collisions by construction?** Use a collision-free identity:
the `date` or `uuid` [naming scheme](../reference/adr-format.md#naming-schemes)
(the route log4brains and database-migration tools took).

## Superseding a decision

```sh
adroit supersede 6 2   # ADR-0006 supersedes ADR-0002
```

Sets the old ADR's status to `superseded` and records `superseded_by:` in its
frontmatter (in place — no file moves), and adds a reciprocal "Supersedes"
note to the new ADR.

## Relating decisions (typed links)

Beyond supersession, ADRs can carry **typed relational links** to other ADRs —
`relates_to`, `depends_on`, and `refines`:

```sh
adroit link 6 --depends-on 2     # ADR-0006 depends on ADR-0002
adroit link 6 --relates-to 4
adroit link 6 --refines 3
adroit link 6 --depends-on 2 --remove
```

The link is recorded in the source ADR's frontmatter, shows in `adroit show`,
and appears as a distinct, colored edge in the dashboard's
[relationship graph](./web.md). The targets use the same identifiers as
everything else (number / slug / uuid).

## Setting a review deadline

```sh
adroit set-review 3 2026-07-15   # propose a review by this date
adroit set-review 3 --clear      # remove it
```

Records an optional `review_by:` deadline on a Proposed ADR. Once the date has
passed, the ADR is flagged review-due in `stats` and the web dashboard. The
write is byte-minimal (only the `review_by:` field changes).

## Regenerating SUMMARY.md

```sh
adroit index
```

Regenerates the ADR section of `SUMMARY.md` (discovered beside the corpus,
e.g. `wiki/SUMMARY.md`) grouped by status, preserving the rest of the file.
Prints to stdout if no `SUMMARY.md` is found.

## Generating a review kickoff

```sh
adroit review 1
adroit review 1 --days 5 --quorum 3 --out review-kickoff.md
```

Generates a review-kickoff doc for an ADR — the structured "here's what you're
reviewing" document the team writes when opening an ADR for a formal accept or
reject decision. It includes the review timeline (computed in business days,
weekends skipped), the quorum, a table of key docs, and a checklist of what the
review MR changes. This is pure generation: no git operations, and the ADR
itself is untouched. Without `--out` it prints to stdout.

## Editing an ADR

```sh
adroit edit 1
```

Opens the ADR in your configured editor. For an AI-assisted revision of the body,
`adroit compose 1 "<instruction>"` applies a free-form instruction (e.g. "expand
the consequences") to the current content and opens the editor on the result —
see [Automation & AI](./automation.md#ai-assisted-authoring).

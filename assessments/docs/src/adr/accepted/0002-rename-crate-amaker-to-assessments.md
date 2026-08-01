# ADR-0002: Rename crate amaker to assessments

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies engineering — the assessments maintainers; no external
consumers exist (the crate was never published).

## Context and Problem Statement

The repository is named `assessments` and the portfolio book describes the
app as "assessments", but the Cargo package was still named `amaker` — a
leftover internal codename. The binary, log lines, templates, and schema
descriptions all carried the old name, so the repo, book, and binary
disagreed about what the tool is called. With the house-stack baseline
(justfile, mdbook, ADR corpus) landing, the name had to be settled first.

## Decision Drivers

- Repo name, book, package name, and binary name should agree
- The crate was never published, so there is no external surface to preserve
- Avoid confusing newcomers with a codename that appears nowhere else
- Keep the rename cheap: do it before more tooling hardcodes the old name

## Considered Options

- Rename the package to `assessments` (match the repo)
- Keep `amaker` and document the codename
- Publish-oriented neutral name (some third name)

## Decision Outcome

Chosen: **rename to `assessments`**, because the repo and portfolio book
already use that name and nothing external depends on `amaker`. The
Cargo.toml package name, the binary (`target/*/assessments`), doc comments,
schemars descriptions, template titles, and the log file name all now say
`assessments`.

### Positive Consequences

- One name everywhere: repo, book, package, binary, logs, UI titles
- The justfile, mdbook, and CI recipes are written once against the real name

### Negative Consequences

- History references `amaker`; old branches and the origin remote predate the
  rename, so cherry-picks across the rename may need path/name fixes
- Any local scripts that invoked the `amaker` binary must switch to
  `assessments`

## Implementation

Done in the `house-stack-baseline` branch: `Cargo.toml` package name, all
self-references in `src/`, `templates/`, and example docs updated; full test
suite kept green.

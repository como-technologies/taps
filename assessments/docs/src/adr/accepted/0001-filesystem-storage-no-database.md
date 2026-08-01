# ADR-0001: Filesystem storage, no database

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies engineering — the assessments maintainers own this decision;
the portfolio's dogfood loop (adroit import downstream) depends on it.

## Context and Problem Statement

assessments stores projects, conversations, and assessment documents. A
`surrealdb-integration` branch explored moving storage into SurrealDB, which
would add a database dependency to what is otherwise a self-contained binary.
The portfolio has a statelessness invariant: tools keep their durable state as
plain files in the repository or data directory, so state is inspectable,
diffable, and dependency-free. We need to settle where assessments' state
lives before building the headless/CLI surface on top of it.

## Decision Drivers

- Keep the dogfood loop dependency-free: a single binary plus a data directory
- Match the portfolio invariant of filesystem-as-state (plain, inspectable files)
- Exports must remain plain files that downstream tools consume directly
- Avoid operating and migrating a database for what is small, per-project data
- Existing storage layer (`src/services/storage.rs`) already works and is tested

## Considered Options

- Filesystem storage: JSON/YAML files under `DATA_DIR/projects/<uuid>/`
- SurrealDB (the `surrealdb-integration` branch)

## Decision Outcome

Chosen: **filesystem storage**, because plain files satisfy every driver —
no database to install, state stays inspectable and portable, and the
existing `StorageService` already implements it. Each project is a directory
`DATA_DIR/projects/<uuid>/` containing `project.json`, `chat.json`,
`assessment.yaml`, and `uploads/`. The `surrealdb-integration` branch is
abandoned and will not be merged.

### Positive Consequences

- Zero infrastructure: clone, set an API key, run
- State is human-readable and diffable; backups are file copies
- The export seam (files on disk) and the storage model are the same shape
- Matches the rest of the portfolio, so house tooling assumptions hold

### Negative Consequences

- No transactional guarantees; concurrent writes to one project could race
- No query layer — listing projects is a directory scan that reads every
  `project.json`
- Schema migrations become file-format migrations handled in code

## Implementation

Already implemented: `src/services/storage.rs` reads and writes the project
directory layout. Follow-up is to delete the dead `surrealdb-integration`
branch and keep new features (CLI export, headless authoring) on the
filesystem layout.

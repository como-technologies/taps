# Export Contract

The export is the product: a schema-validated data file that downstream tools
can consume. Markdown is **not** an export format — exports are data, in three
interchangeable formats.

## Formats

`ExportService` (`crates/amaker-core/src/services/export.rs`) serializes an
assessment to:

| Format | Extension | Content type       |
| ------ | --------- | ------------------ |
| YAML   | `.yaml`   | `text/yaml`        |
| JSON   | `.json`   | `application/json` |
| TOML   | `.toml`   | `application/toml` |

## Two surfaces, one pipeline

- `GET /api/projects/{id}/export?format=yaml|json|toml` — download the
  project's assessment in the requested format (default `yaml`; `yml` is
  accepted as an alias). The filename is derived from the project name.
  Returns 404 if no assessment has been generated yet.
- `amaker export <project-id> --format yaml|json|toml --out FILE` —
  the same export from the command line, no server or API key needed. See
  [Command Line](./cli.md).
- `GET /api/schema` and `amaker schema` — the JSON Schema for the
  assessment format.

The HTTP route and the CLI subcommand serialize through the same
`ExportService::to_data` call, so their output is **byte-identical by
construction**; the `golden_export` integration test pins that
serialization (see below).

## The schema is generated, not hand-written

The JSON Schema is produced with `schemars` from the same Rust structs that
the app serializes (see [Metamodel](./metamodel.md)), so schema and
serialization cannot drift apart. Validation
(`ExportService::validate_and_parse`) parses any of the three formats to a
JSON value, checks it against the generated schema with `jsonschema`, and
only then deserializes — invalid documents are rejected with the full list of
schema violations.

The schema itself is deterministic: `id` fields default to *freshly minted*
UUIDs when omitted, so the generator strips the meaningless random `default`
values schemars would otherwise embed (a unit test pins that two
generations are byte-identical).

## Minimal valid document

```yaml
name: Example Assessment
description: What this assessment evaluates
goal: Why we are assessing it
domains:
  - name: A domain
    context: What it covers
    value: Why it matters
    risk: What happens if ignored
    practices:
      - name: A practice
        context: What it covers
        value: Why it matters
        risk: What happens if ignored
        questions:
          - text: Is the practice in place?
            polarity: positive
```

IDs and timestamps are optional on import — they default to fresh values.
(Corollary: files *stored by the app* always carry explicit IDs; an ID-less
file would re-mint fresh UUIDs on every parse and could not export
deterministically.)

## The committed golden

The contract is pinned by the `golden_export` integration test
(`crates/amaker-core/tests/golden_export.rs`, in `just test` — part of
`just ci`). It builds one small assessment with every UUID and timestamp
pinned (through the real `Assessment::new` / `Domain::new` /
`Practice::new` constructors, so the default controlled vocabularies ride
along), exports it via `ExportService::to_data`, and asserts:

1. the export **validates against the generated JSON Schema** — the same
   `validate_and_parse` path the CLI `validate` command uses — in all
   three formats, and
2. the YAML export is **byte-identical** to the vendored golden at
   `contract/fixtures/golden-assessment.yaml` (workspace root — the same file adroit's import contract test reads).

If a contract change is intentional, run the test: it writes the current
bytes next to the golden (`golden-assessment.actual.yaml`); review, replace
the fixture, and commit the diff — downstream consumers see a reviewed
change, not silent drift.

## Downstream: seeding ADRs

This export is the seam between the *Assess* and *Prescribe* stages:
[adroit](https://github.com/como-technologies/adroit) mirrors these structures
in its importer, and

```bash
adroit import --from-assessment assessment.yaml --dry-run -o json
```

reads an exported assessment and seeds one Proposed ADR per practice
(`-o json` emits a machine summary of the seeds). The seam is guarded
mechanically from both sides: this repo's `golden_export` test pins the
producer shape (above), and adroit's contract test pins its importer
against a vendored copy of this app's export — so contract drift on either
side fails that side's CI without a model. The full story, including the
live ollama-authored variant and the current state of the vendored copy,
is on the [Dogfood](./dogfood.md) page.

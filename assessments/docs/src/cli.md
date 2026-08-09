# Command Line

`amaker` is the headless CLI for scripting the Assess stage — authoring,
importing, exporting, validating, publishing to the KB, and emitting the
JSON Schema. (The interactive web apps are separate workspace crates:
`amaker-author`, `amaker-assess`, `amaker-analyze`.) Of the commands, only
`author` talks to an AI provider — everything else never constructs one,
so **no API key is needed** (and `author` needs none either with
`AI_PROVIDER=ollama`).

```text
Usage: amaker <COMMAND>

Commands:
  export    Export a project's assessment (reads DATA_DIR, default ./data)
  validate  Validate an assessment file against the JSON Schema
  schema    Write the assessment JSON Schema
  import    Import an assessment file as a new project with a published version
  publish   Publish a project's assessment + analysis to the KB
  author    Author a complete assessment from a brief, headlessly
```

## `import <file> [--name NAME] [--version v1]`

The headless authoring door: a validated assessment file (any drafted
YAML/JSON/TOML — hand-written, agent-written, or `author`'s output)
becomes a new project under `DATA_DIR` with a published version, ready
for respondents. Validation is the same gate `validate` runs, including
placeholder rejection. Prints the new `project_id` as JSON.

## `publish <project-id> [--wiki NAME]`

Lands the project's assessment definition and the primary respondent's
analysis (scorecard, definite gaps, prioritized roadmap) in the KB as
two typed pages — `assessment` and `assessment-report`, classes amaker
owns and registers on first contact (`x-owner: amaker`). Needs `KB_URL`
(the appliance's streamable-HTTP MCP endpoint; `KB_WIKI` optionally
names the wiki — the suite-wide pair, honored from `.env`). The pages
enter through the KB's admission gates; the printed report carries the
gate's verdict, including how many pages the search index picked up.
Repeatable: schemas come back `unchanged`, pages refresh.

## `export <project-id> [--format yaml|json|toml] [--out FILE]`

Serializes a stored project's assessment. The project is looked up under
`DATA_DIR/projects/<project-id>/` (`DATA_DIR` defaults to `./data`, the same
default the web app uses; a `.env` file is honored).

- `--format` defaults to `yaml`.
- `--out` writes to a file; without it the content goes to stdout.
- Exits non-zero if the project does not exist or has no generated
  assessment yet.

```bash
amaker export a55e55ed-0000-4000-8000-000000000001 --format yaml --out assessment.yaml
```

The CLI export and the HTTP route `GET /api/projects/{id}/export` serialize
through one implementation (`ExportService::to_data`), so their output is
**byte-identical by construction**; the `golden_export` integration test
pins that serialization against the schema and a vendored golden (see
[Export Contract](./export-contract.md)). The exported bytes are written
exactly as produced; nothing is appended.

## `validate <FILE> [-o human|json]`

Schema-validates an assessment file. The format is inferred from the file
extension (`.yaml`/`.yml`/`.json`/`.toml`). On success it prints a summary
and exits 0; on schema violations, unreadable files, or unknown extensions it
prints the errors and exits non-zero.

Schema-valid is not enough: `validate` also applies the **degeneracy gate**
(ADR-0007) and exits non-zero when a load-bearing field — assessment
name/description/goal, domain or practice name/context/value/risk — is
empty, an ellipsis stand-in, or a verbatim echo of the structure prompt's
scaffold placeholders. A document literally named "Assessment Name" (as the
first dogfood run produced) fails with every degenerate field listed.

```bash
$ amaker validate assessment.yaml
valid: 'Release Readiness Sample' — 1 domains, 1 practices, 2 questions
```

With `-o json` the summary is machine-readable, listing every practice with
its domain and question count:

```json
{
  "valid": true,
  "name": "Release Readiness Sample",
  "domains": 1,
  "practices": [
    { "name": "Continuous Integration", "domain": "Delivery", "questions": 2 }
  ],
  "questions": 2
}
```

Each `practices[]` entry's `name`/`domain` matches the `title`/`domain` of
the seed `adroit import --from-assessment ... -o json` reports for it, so a
script can join the two and assert every practice produced a Proposed-ADR
seed — the loop line's `seam-check` recipe did exactly that (see
[Dogfood](./dogfood.md)). Validation failures keep the human error output
and the non-zero exit; no JSON error envelope is emitted.

## `schema [--out FILE]`

Writes the generated JSON Schema for the assessment format (pretty-printed,
newline-terminated) to `--out` or stdout. The output is deterministic —
two runs are byte-identical, so downstream consumers can pin it — see
[Export Contract](./export-contract.md).

```bash
amaker schema --out schema.json
```

## `author --brief FILE [--context FILE...] [--jobs N] --out FILE`

Authors a complete assessment from a written brief with no web UI and no
interaction — see [Headless Authoring](./authoring.md) for the pipeline,
the retry behavior and quality gates (placeholder echoes, duplicate
practices, context leakage), and a worked example. `--context` files are
framed as background signal about the assessed organization; their
filenames and data-shape JSON keys are mechanically banned from the
authored output.

`--jobs N` (default 1, max 8) generates questions for up to N practices
concurrently with deterministic assembly order — a real speedup only when
the ollama server is run with `OLLAMA_NUM_PARALLEL >= N`, and each server
slot multiplies KV-cache memory at the pinned `num_ctx=8192`; see
[Configuration](./configuration.md#parallel-authoring-and-ollama_num_parallel)
and the measured comparison in [Dogfood](./dogfood.md#timing-serial-vs---jobs).

```bash
AI_PROVIDER=ollama amaker author \
  --brief examples/dogfood/brief.md \
  --out assessment.yaml
```

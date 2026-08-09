# Dogfood: the Assess→Prescribe Seam

`amaker` is the entry point of a loop: **Assess** produces a
schema-validated assessment, and the **Prescribe** stage
([adroit](https://github.com/como-technologies/taps/tree/main/adroit)) consumes it to seed
one Proposed ADR per practice — turning "here are your gaps" into "here are
the decisions to make". This page documents that seam: how to rehearse it
live, the recorded proof runs, and the mechanical contract gate that
`just ci` actually runs.

## Rehearsing the loop live

The live rehearsal is fully local: no API key, no network beyond
localhost. It needs a running [ollama](https://ollama.com) (`llama3.2` by
default) and an adroit binary — build the in-tree one from the workspace
root (`cargo build -p adroit` → `target/debug/adroit`).

1. **Author** — write a fresh assessment from the committed generic
   engineering-maturity brief, via the tool-call-free pipeline described
   in [Headless Authoring](./authoring.md):

   ```bash
   AI_PROVIDER=ollama cargo run -p amaker-cli -- author \
     --brief examples/dogfood/brief.md --out /tmp/como-dogfood/assessment.yaml
   ```

2. **Validate** — check the file against the generated JSON Schema **and
   the degeneracy gate** (load-bearing fields echoing prompt placeholders
   fail with a non-zero exit — ADR-0007). `author` already wrote only a
   gated document; this re-checks it through the public surface:

   ```bash
   cargo run -p amaker-cli -- validate /tmp/como-dogfood/assessment.yaml
   ```

3. **Import** — against a fresh, empty wiki (adroit is KB-only, its
   ADR-0020: `ADROIT_DIR` must name a directory holding `wiki.toml`):

   ```bash
   SPACE=$(mktemp -d)
   printf 'name = "adrs"\n' > "$SPACE/wiki.toml" && mkdir -p "$SPACE/wiki/decisions"
   ADROIT_DIR=$SPACE adroit import \
     --from-assessment /tmp/como-dogfood/assessment.yaml --dry-run -o json
   ```

   `--dry-run` writes nothing; `-o json` emits a machine summary of what
   would be seeded. Every practice should yield at least one seed — the
   loop-entry contract.

## A real run

The run recorded here was executed fully locally on llama3.2 (3B,
temperature 0, CPU) against the committed brief, driven by the loop line's
one-command `just dogfood` recipe — a script of exactly the steps above
plus two `jq` assertions: a *dedupe assert* (re-derive the practice list
via `validate -o json`, fail if two practice names collide after
case/whitespace normalization) and a *seam assert* (join the practice list
against the import summary's `seeded` array, fail listing any practice
that produced no seed). That driver has not been ported to main's
justfile; the transcript is kept as the recorded proof (abridged to the
first and last practice):

```text
$ just dogfood
[   0.0s] scoping summary...
[  10.6s] generating structure...
[  22.7s] generating structure (attempt 2)...
[  55.7s] structure 'Assessment Name': 4 domains, 8 practices
[  55.7s] questions 1/8: Pipeline Configuration and Management...
[  80.2s]   -> 12 questions
...
[ 179.6s] questions 8/8: Incident Response and Management...
[ 200.5s]   -> 8 questions
[ 215.5s] authored 'Assessment Name' in 215.5s — 4 domains, 8 practices, 60 questions -> /tmp/como-dogfood/assessment.yaml
valid: 'Assessment Name' — 4 domains, 8 practices, 60 questions
Would seed 8 proposed ADR(s) from assessment "Assessment Name".
  Code Quality Improvement: 2 seed(s)
  Delivery Pipeline Maturity: 2 seed(s)
  Operations Maturity Assessment: 2 seed(s)
  Testing Maturity: 2 seed(s)
seam ok: 8 seed(s) cover all 8 practice(s) of 'Assessment Name'
```

Eight practices in, eight Proposed-ADR seeds out — `≥1` seed per practice,
the loop-entry contract — in 3.6 minutes of authoring. Two honest 3B-model
warts are visible: the first structure attempt produced no usable YAML (the
corrective retry recovered it — by design, see
[Headless Authoring](./authoring.md)), and the model copied the literal
placeholder `Assessment Name` from its prompt template instead of naming
the assessment.

That second wart can no longer ship: since ADR-0007 the degeneracy gate
rejects placeholder echoes inside the authoring retry loop (the model gets
corrective feedback naming the echo) and `validate` exits non-zero on them
— the very artifact in the transcript above now fails both. The transcript
is kept as the honest record of why the gate exists.

The import summary the assertion parses looks like:

```json
{
  "source": "/tmp/como-dogfood/assessment.yaml",
  "assessment": "Assessment Name",
  "dry_run": true,
  "seeded": [
    {
      "reference": null,
      "title": "Pipeline Configuration and Management",
      "status": "Proposed",
      "domain": "Delivery Pipeline Maturity"
    }
  ],
  "skipped": []
}
```

(`seeded` truncated to one entry; `reference` is `null` only because
`--dry-run` mints no ADR numbers. A real import — drop `--dry-run`, point
`ADROIT_DIR` at the target corpus — writes the Proposed ADRs and reports
their references, ready for `adroit lint` / `accept` / `plan` on the
Prescribe side.)

## Timing: serial vs `--jobs`

The questions phase dominates authoring wall-clock (one ~25–30s model call
per practice on a CPU-bound llama3.2). `author --jobs N` runs those calls
on N concurrent lanes; whether that is *faster* depends entirely on the
ollama server's parallel capacity. Measured live on the committed
brief (llama3.2 3B, CPU-only host, `num_ctx=8192` pinned, debug build;
2026-06-12):

| Run | Server slots (`OLLAMA_NUM_PARALLEL`) | Practices | Questions | Total wall | Questions phase | s/question |
|---|---|---|---|---|---|---|
| `--jobs 1` (default) | 1 (default) | 6 | 61 | 211.1s | 161.5s | 2.65 |
| `--jobs 2` | 1 (default) | 8 | 86 | 278.9s | 229.1s | 2.66 |
| `--jobs 4` | 1 (default) | 8 | 64 | 226.0s | 178.5s | 2.79 |
| `--jobs 2` | **2** | 6 | 70 | **161.8s** | **109.2s** | **1.56** |

Three honest findings:

1. **Against a default (1-slot) server, extra lanes queue.** Per-question
   throughput is identical (2.65 vs 2.66 s/q) — `--jobs` is safe but
   useless until the server can actually serve requests in parallel.
   That is why the default stays `--jobs 1`.
2. **With `OLLAMA_NUM_PARALLEL=2`, the lanes genuinely overlap.** The
   same brief produced the same 6-practice structure as the serial
   baseline, finishing in 161.8s vs 211.1s (questions phase 109.2s vs
   161.5s — a 1.7x per-question throughput gain even on CPU, where
   batched decoding amortizes weight reads). The memory price: the server
   allocates one `num_ctx=8192` KV cache **per slot**.
3. **Cross-run output is not byte-stable.** Generation runs at
   temperature 0, but the ollama server does not guarantee determinism
   across runs/batching states — the 1-slot concurrent runs authored an
   8-practice structure from the same brief. Every run above passed
   `validate` (schema + degeneracy) and the dedupe gate; compare
   throughput per question, not total wall-clock, across runs.

For the run-2 Assess beat (context-bearing, 8 practices), the serial mark
was 355.9s — the questions phase was ~288s of it, which is the part
`--jobs` attacks when the server has slots.

## The Assess beat with Measure artifacts (`--context`)

The full-loop run authors with `--context` files — the previous
iteration's Measure-stage reports:

```bash
AI_PROVIDER=ollama cargo run -p amaker-cli -- author \
  --brief examples/dogfood/brief.md --out OUT.yaml \
  --context ctx1.json --context ctx2.json
```

Context is wired into every generation prompt as *background*, and the
in-pipeline **leakage gate** (ADR-0007) rejects drafts whose load-bearing
fields cite the context artifacts themselves (file names, data-shape
keys) instead of the domain. The loop line's driver added an external
mirror of that gate — a `just leakage-assert` recipe deriving banned
tokens from the context files (each file's basename; for `.json` files
every data-shape key — at least 4 chars, containing `_` — at any depth,
via jq) and grepping the authored YAML case-insensitively. The first
run's artifact — whose question guidance cited `pulse-report.json`,
`per_tenant`, and `total_flows` — fails it with all three tokens
reported; that recipe, like the rest of the driver, is not yet on main's
justfile.

## What CI actually gates: the export contract

Authoring takes minutes and needs a model; CI gets the seam's contract
guarantee for free, mechanically, from both sides:

- **Producer side (assessments):** the `golden_export` integration test
  (`crates/amaker-core/tests/golden_export.rs`, in `just test` — part of
  `just ci`) builds a small, fully pinned assessment, exports it through
  the real `ExportService::to_data` pipeline, asserts the export
  validates against the published JSON Schema in all three formats, and
  pins the YAML byte-for-byte against the canonical golden at
  `contract/fixtures/golden-assessment.yaml` (workspace root). Any change
  to the export shape fails CI here as a reviewable fixture diff — with
  no model in sight. See [Export Contract](./export-contract.md).
- **Consumer side (adroit):** adroit's contract test pins
  `import --from-assessment` against the SAME file, by path — one golden,
  two tests (this replaced per-product vendored copies, which had
  drifted apart in shape, with this single fixture).

The loop line's `just seam-check` recipe (a jq join of `validate -o json`
against a dry-run import's seed summary) has not been ported to main's
justfile; the golden-export test is the seam gate `just ci` actually
runs. ADR-corpus validation — this book's corpus included — is on demand
with the in-tree adroit (seed into an ephemeral wiki, `adroit check`);
the former resolution chain is retired.

## Where the loop goes next

The dry-run proof stops at the seam on purpose: seeding a real corpus, ADR
triage (`lint`, `accept`), and implementation planning (`plan --save`)
belong to the Prescribe stage and are adroit's to document. From there the
portfolio loop continues to Adopt (conduit) and Measure (tuesday, pulse),
whose reports feed back into the next Assess pass as
`author --context` files.

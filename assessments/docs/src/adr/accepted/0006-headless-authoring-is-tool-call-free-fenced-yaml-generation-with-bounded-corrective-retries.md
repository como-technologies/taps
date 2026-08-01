# ADR-0006: Headless authoring is tool-call-free fenced-YAML generation with bounded corrective retries

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies engineering — the assessments maintainers own this decision;
the portfolio's local-first dogfood loop (authoring an assessment end to end
on a laptop with no hosted API key) and the downstream Prescribe-stage tooling
that consumes `author`'s output depend on it.

## Context and Problem Statement

The web flow authors assessments through a tool-calling chat loop: the model
decides when to invoke `generate_structure` / `generate_questions`, and the
server executes them. ADR-0004 already recorded that tool-call reliability on
small local models (llama3.2 3B) is markedly weaker than Claude's — the model
forgets to call tools, calls them with malformed arguments, or drops required
fields. The headless `author` command (`assessments author --brief FILE
[--context FILE...] --out FILE`) must run unattended in scripts and CI and
work on exactly those small local models, so the question is how it should
drive generation: reuse the tool-calling loop, or orchestrate the steps
itself? And when a small model emits unusable YAML, what turns that from a
hard failure into a recoverable one?

## Decision Drivers

- `author` must be viable on llama3.2-3B via ollama — the local-first,
  no-API-key path is the point of the headless flow
- Unattended operation: no human in the loop to rephrase a prompt, so
  failures must be either self-correcting or loud (non-zero exit with an
  actionable message), never a hang or a silently broken file
- The output file must always pass the committed JSON Schema contract
  (`contract/schema.json`) — downstream tooling pins it
- ADR-0004 pins temperature 0 for the plain-YAML generation paths: an
  identical retry of a failed prompt would fail identically, so retries are
  only worth having if each one changes the prompt
- The brief and `--context` files must actually reach the generation prompts
  (the web flow's upload path historically dropped file content)
- Reuse the existing prompt/parse helpers behind the `LlmProvider` seam so
  the FakeProvider keeps the whole pipeline deterministic in CI

## Considered Options

- **Tool-call-free orchestration in code**: the `author` pipeline itself runs
  scoping summary → `generate_structure` → per-practice
  `generate_questions`, each as a plain completion whose fenced ```yaml
  block is extracted, schema-validated, and on failure retried with the
  error fed back into the prompt, bounded at 3 attempts per step
- **Reuse the interactive tool-calling chat loop headlessly**: synthesize
  user turns and let the model drive the same tools the web flow uses
- **Single mega-prompt**: one completion generating the entire assessment
  (structure and all questions) in one YAML document

## Decision Outcome

Chosen: **tool-call-free orchestration in code with bounded corrective
retries**, because the orchestration is not a judgment call — the steps and
their order are fixed — so handing sequencing to the model only imports the
3B tool-calling failure modes ADR-0004 documented. The code drives brief →
scoping summary → structure → questions per practice as plain fenced-YAML
completions (`src/services/author.rs`), reusing the same prompts and parsers
as the web flow; tool calling stays reserved for the interactive web flow,
where the model genuinely decides what happens next. This is what makes
llama3.2-3B viable headlessly.

Each generation step is gated: the response must contain a fenced YAML block
that parses, passes JSON Schema validation, and is non-degenerate (at least
one domain, practices in every domain, a non-empty questions list per
practice). A failed gate retries up to 3 attempts per step — with the
previous error injected into the next prompt as corrective feedback, since
at temperature 0 a blind retry would reproduce the failure byte for byte.
The structure prompt keeps the ADR-0004 quirk: the literal `questions: []`
nudge is positioned last (recency) so the 3B model keeps the required empty
arrays. Provider errors (network, backend) are not retried; they propagate
immediately. On exhausted retries `author` exits non-zero with the step,
the attempt bound, and the last error, and writes nothing.

The mega-prompt option was rejected because a 3B model reliably degrades
over long generations (the per-practice split keeps each completion small),
and one bad line would invalidate the entire document instead of one step.

### Positive Consequences

- `assessments author` runs fully local against ollama/llama3.2 and produced
  the committed dogfood example (`examples/dogfood/`) in one unattended run
- Deterministic in CI: FakeProvider scripts the exact malformed-then-valid
  retry sequences, so the retry logic itself is pinned by unit tests
- The written file is schema-valid by construction — the final document is
  validated against the same schema `validate` uses before it is written
- `--context` files are read up front and injected verbatim into every
  generation prompt, closing the dead-upload gap for the headless path
- Failures are bounded and actionable: at most 3 attempts per step, then a
  non-zero exit naming the failing step (and practice) and the last error

### Negative Consequences

- Two authoring paths now exist (tool-driven web flow, code-driven headless
  flow); prompt or schema changes must be checked against both
- Corrective-feedback retries triple the worst-case model calls and runtime
  for a step that keeps failing before ultimately exiting non-zero
- The retry bound is a constant (3), not a flag — changing it is a code
  change until someone needs it configurable
- A model whose failure is not promptable-away (e.g. it never emits fenced
  YAML) burns all retries on every step; there is no per-model tuning

## Implementation

Landed with this decision (milestone M5): `src/services/author.rs`
(`author_assessment` orchestration, `Progress` events, retry feedback
assembly, structure/questions gates), the `author` clap subcommand in
`src/cli.rs` (brief/context reading, stderr progress with elapsed time,
actionable failure context), the `author_scoping.md` prompt, the
`examples/dogfood/` brief + authored output, and the book's Headless
Authoring page. The live path is exercised manually against ollama; CI
covers the pipeline with FakeProvider scripts.

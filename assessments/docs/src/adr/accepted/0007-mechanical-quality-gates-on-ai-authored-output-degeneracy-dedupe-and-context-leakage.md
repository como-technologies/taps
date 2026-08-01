# ADR-0007: Mechanical quality gates on AI-authored output: degeneracy, dedupe, and context leakage

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies engineering — the assessments maintainers own this
decision; the portfolio dogfood loop (whose Assess beat consumes `author`'s
output unattended) and the Prescribe-stage tooling that imports the authored
assessment depend on it.

## Context and Problem Statement

The first full dogfood run exposed three output-quality failures that the
existing gates (JSON Schema validation plus the structural checks of
ADR-0006) cannot catch, because the documents were all *schema-valid*:

1. **Placeholder echo**: the run's assessment was literally named
   "Assessment Name", with description "What this assessment evaluates" and
   goal "Intended outcome" — the structure prompt's example scaffold copied
   back as content. Semantically empty, load-bearing, and downstream tools
   carried it everywhere.
2. **Duplicate practices**: the same practice ("Learning from Failure") was
   authored into two different domains. The importer's dedupe guard caught
   it on the Prescribe side — right behavior, wrong place: the authoring
   side shipped a defective document.
3. **Context cited as subject**: `--context` files (Measure-stage reports)
   were injected verbatim, and the model authored questions about the
   artifacts themselves — "Check the 'pulse-report.json' file under
   'per_tenant'..." — instead of about the organization the reports
   describe. Context must steer authoring, not appear in it.

Small local models will keep doing all three. The question is where the
defense lives: in prompt wording alone, in human review, or in mechanical
checks the pipeline enforces.

## Decision Drivers

- The dogfood loop runs unattended; a defective artifact must fail loudly
  at the Assess stage, not surface later in an ADR corpus
- Prompt phrasing alone is not a contract — the first run already proved
  instructions are echoed or ignored by 3B models
- ADR-0006's bounded corrective-retry machinery exists and works; new
  failure modes should feed it rather than hard-fail on first occurrence
- `validate` is the public gate other tools script against; what authoring
  rejects, `validate` must also reject
- Checks must be deterministic and cheap so FakeProvider tests pin them in
  CI without a model

## Considered Options

- **Mechanical quality gates in the pipeline + `validate`**: pure functions
  over the parsed assessment (placeholder-echo detection pinned to the
  prompt's literal scaffold strings, normalized practice-name dedupe,
  banned-token leakage checks derived from the context files), enforced
  inside the bounded retry loop with corrective feedback, and re-enforced
  by `validate`
- **Prompt engineering only**: strengthen the instructions (do-not-cite,
  uniqueness, "use a real name") and trust the model
- **Human review only**: accept the warts and rely on the web-flow refining
  phase to fix names, duplicates, and leaks

## Decision Outcome

Chosen: **mechanical quality gates in the pipeline + `validate`**, because
every AI-output consumer needs a mechanical normalization layer — prompt
phrasing alone is not a contract (the same principle that put
`sanitize_draft` on the Prescribe side). Prompt improvements are kept too
(context is now framed as background signal with an explicit never-cite
instruction), but the gates are what make the property hold.

Each gate follows the same shape — corrective retry first, then a bounded
resolution:

- **Degeneracy** (`quality::degenerate_fields`): load-bearing fields
  (assessment name/description/goal, domain and practice
  name/context/value/risk) must not be empty, ellipsis stand-ins, or
  case-insensitive echoes of the structure prompt's scaffold strings; the
  list is pinned to the prompt text by a unit test so it cannot drift. A
  degenerate structure retries with feedback naming the echoes; if all
  attempts stay degenerate, authoring fails. `validate` applies the same
  check and exits non-zero (the first run's artifact now fails it).
- **Dedupe** (`quality::duplicate_practice_names` /
  `drop_duplicate_practices`): practice names are unique across all domains
  after case/whitespace normalization. Duplicates retry with feedback
  naming the practice and its domains; if every attempt keeps the
  duplicate, the later occurrences are dropped mechanically (first
  occurrence wins, emptied domains removed) with a surfaced warning —
  mirroring the importer's guard.
- **Leakage** (`quality::forbidden_context_tokens` /
  `leaky_assessment_fields`): each `--context` file contributes banned
  tokens — its basename plus, for JSON, every object key at any depth that
  reads as data-shape jargon (≥ 4 chars, contains `_`). No banned token may
  appear in any authored text field, case-insensitively. Leaks retry with
  feedback naming the tokens; a still-leaky run fails (a leak cannot be
  normalized away). The dogfood recipes re-assert dedupe (`dedupe-assert`,
  via jq) and leakage (`leakage-assert`, via grep with the same token rule)
  externally over the written YAML.

Prompt-only was rejected as already falsified; review-only was rejected
because the headless path is exactly the one with no human in the loop.

### Positive Consequences

- A schema-valid but semantically empty assessment can no longer leave the
  Assess stage or pass `validate`; the dogfood loop's quality bar is
  machine-checked at the seam
- The three run-1 warts are pinned as regressions: the placeholder-echo
  artifact fails `validate`, the duplicated practice triggers
  retry-then-drop, the leaky guidance triggers retry naming the tokens
- Gates are pure functions in `services::quality`, deterministic under
  FakeProvider, and shared verbatim between authoring and `validate`
- Corrective feedback is specific (field paths, offending values, leaked
  tokens), giving the bounded retries a real chance to repair the output

### Negative Consequences

- The placeholder list and the jargon-key heuristic (≥ 4 chars with `_`)
  are heuristics: novel placeholder phrasings pass, and a context JSON
  whose keys are plain words is only protected by its filename token
- Leakage banning is substring-based; a legitimate question that genuinely
  needed a banned phrase would be impossible to author with that context
  attached
- Worst-case model calls per step stay 3x, and now more failure classes
  consume those attempts
- The external recipe checks (jq/grep) duplicate the in-pipeline token rule
  in shell; the two implementations could drift (mitigated by both being
  exercised in the dogfood recipes)

## Implementation

Landed with this decision (iteration-2 learnings fixes): the
`services::quality` module (placeholder constants + degeneracy, dedupe, and
leakage checks with unit tests off the actual prompt scaffold), gate
enforcement in `author_assessment`'s structure and per-practice retry loops
with `Progress::DuplicatePracticesDropped` as the warning surface, the
degeneracy gate in `validate`, `AuthorContext` (framed background-signal
context text + derived banned tokens) replacing verbatim context injection,
and the `dedupe-assert` / `leakage-assert` / `dogfood-with-context` just
recipes. FakeProvider tests cover every retry/fallback path; the
[Headless Authoring](../../authoring.md) book page documents the
gates.

# The lifecycle

An assessment moves through three acts: **Authoring**, **Responding**,
**Analysis**. Each is a distinct mode of work with its own UI, AI role,
and outputs — and each is its own binary (`amaker-author`,
`amaker-assess`, `amaker-analyze`).

## Act 1: Authoring

**Goal:** produce a high-quality, scoped, polarity-correct assessment.

**Who:** the SME author, working with the AI.

**Shape:** a chat-driven workflow with a live preview. The SME describes
the domain; the AI builds structure; both iterate.

Authoring has four **substates** — Scoping, Structuring, Questions,
Refining. They are *advisory*: each steers which conversational framing
the AI uses, but none gates a tool. The surgical edit tools, publish, and
reset all work from any substate.

1. **Scoping** — "what are we assessing?" The SME describes the domain,
   audience, and goals. The AI asks clarifying questions, can take
   uploaded documents as context, and tailors the assessment's evidence
   and blocker vocabularies to the domain.
2. **Structuring** — the AI generates a draft hierarchy: domains with
   CVR, practices with CVR. No questions yet. The SME reviews the shape.
3. **Questions** — questions get generated per practice, iteratively.
   The SME sees each batch and pushes back. Polarity and guidance are set
   here.
4. **Refining** — collaborative polish: reword, reorder, edit CVR, tune
   question counts — all through surgical edit tools.

The AI moves between substates with the `switch_focus` tool, but mostly
just infers the right framing from the conversation. An SME can revisit
any part at any time; nothing is gated.

### What the AI does during authoring

- Asks clarifying questions before committing to structure.
- Proposes domains and practices within the 3–7 / 2–5 / 3–12 sanity rails.
- Enforces the metamodel shape: no compound questions, no subjective
  wording, no Likert options masquerading as binary.
- Tailors vocabulary: "Temperature logs" for a restaurant; "SBOM" for a
  software supply-chain audit.
- Explains its reasoning when pushed.

### What the SME does

- Brings domain expertise.
- Makes the final calls on scope, naming, and emphasis.
- Uploads reference material the AI can draw on.
- Refuses the AI's suggestions when they're wrong.

### Publishing

When the assessment is ready, the author **publishes a version** — an
immutable, named snapshot of the current draft. Publishing doesn't lock
anything: the author keeps editing the draft afterward, and a later
publish produces another version. Responses always bind to a specific
published version, so authoring can continue freely without disturbing
anyone mid-response. The mechanics are in
[Storage, versioning & the draft/publish model](../architecture/draft-publish.md).

## Act 2: Responding

**Goal:** capture answers with enough supporting metadata to enable
meaningful analysis.

**Who:** the respondent (may or may not be the author).

**Shape:** a focused form, served by `amaker-assess` against a published
version. No chat sidebar, no LLM — just the form.

Each question presents:

- Yes / No / Unknown radios.
- Conditional evidence checkboxes when Yes is selected (from the
  assessment's evidence vocabulary).
- Conditional blocker checkboxes + a "planned?" flag when No is selected
  (from the blocker vocabulary).
- A free-text notes field, always available.

Each answer persists as it's entered (a short debounce), via a per-answer
`PATCH`. The respondent can leave and return; progress shows as a chip at
the top (`12 of 47 answered`).

### Why the form isn't just yes/no

A bare yes/no tells you the score. The metadata tells you what to do
about it:

- **Evidence** feeds the compliance narrative. "Audited" gaps and
  "process in place" gaps are different conversations with a regulator.
- **Blockers** feed the roadmap. A gap blocked by "People" is a hiring
  conversation; one blocked by "Technology" is an engineering investment.
- **Planned?** separates known-accepted risk from known-unaddressed risk.
- **Notes** catch everything the structured fields miss.

Without this metadata the analysis layer has nothing to say beyond a
percentage. With it, the report reads like someone thought about the
answers.

### Why there's no "freeze"

A response binds to the *published version* it was administered against,
not to the live draft. The author can keep editing the draft — even
publish new versions — and a response in progress is untouched, because
it keeps reading its bound version. There is no freeze step and no
archive-and-revise dance: editing and responding simply operate on
different objects.

## Act 3: Analysis

**Goal:** turn answers into action.

**Who:** the author, the respondent, any stakeholder downstream.

**Shape:** a four-tab view served by `amaker-analyze` — Scorecard, Gaps,
Roadmap, Narrative.

The four tabs are layered from deterministic to generative:

1. **Scorecard** — percentages, per-practice / per-domain / overall, with
   counts of yes / no / unknown / unanswered. Polarity-aware aggregation.
2. **Gap Inventory** — every question that didn't resolve to a pass,
   enriched with inherited CVR context, blockers, planned flag, owner
   roles, and effort range. The raw list a stakeholder would want.
3. **Roadmap** — gaps pivoted by owner role and ordered by a priority
   heuristic (risk × inverse effort).
4. **Narrative** — an LLM-generated Markdown report: Executive Summary,
   Strengths, Key Gaps, Priority Actions, By Role. It reads the
   scorecard, gap inventory, roadmap, and CVR text — it does not invent.

The first three tabs are pure functions of `(Assessment, Response)` and
recompute live as answers change. The narrative is cached and regenerates
on demand — the "Regenerate" button POSTs to `amaker-author`, which holds
the LLM transport.

### Why the analysis is layered this way

Different stakeholders want different things — a CTO wants the exec
summary, a security engineer wants the role-grouped roadmap, an auditor
wants the gap inventory with evidence, a procurement lead wants the score.
One tool, four views, all from the same structured data. The narrative
sits on top — readable, persuasive — but never replaces the deterministic
layers below it.

## After Analysis

Today the user exports (YAML / JSON / TOML) and that's the end of the
arc. The [roadmap](./roadmap.md) covers what comes next — snapshots and
trend over time, multi-respondent aggregation, PDF report generation,
assisted answering.

# Core concepts

The metamodel is the spine of the product. Everything else — authoring UX, collection forms, analysis outputs — is shaped by it. This page describes the concepts as a team reference.

## The hierarchy

Assessments are a four-level tree:

```
Assessment
  └─ Domain (3–7)
       └─ Practice (2–5 per domain)
            └─ Question (3–12 per practice)
```

The level counts aren't hard limits; they're sanity rails. Too few domains and the assessment isn't diagnostic. Too many questions per practice and you've built a survey no one will complete. The AI enforces the ranges softly, and the SME can override.

**Terminology is flexible.** A "Domain" might be called a Stage, Pillar, Area, Category, or Control Area depending on the assessment. A "Practice" might be a Capability, Control, Activity, or Process. The metamodel uses "Domain" and "Practice" internally but renders the author's chosen terms everywhere.

## The Context-Value-Risk triad

At every non-leaf level (Domain and Practice), three narrative fields sit alongside the name:

- **Context** — what this is and why it's a category worth separating out.
- **Value** — the benefit of doing this well (positive motivation).
- **Risk** — the consequence of ignoring it (negative motivation).

This triad is the single most important design decision in the metamodel. Three reasons:

1. **It makes the author think.** You cannot write a good Value without understanding the domain; you cannot write a good Risk without understanding what failure looks like.
2. **It gives respondents orientation.** Before they see a question, they see why the category of questions matters. "Compliance" is abstract; "unpatched production dependencies expose us to known CVEs and can trigger an audit finding under SOC 2" is concrete.
3. **It becomes the narrative spine of the final report.** The LLM narrative layer reads CVR text back out during analysis: a 30% score on a high-risk domain produces a different paragraph than a 30% score on a low-risk one.

CVR is cheap to author (three sentences) and pays compounding interest. We invest in it during authoring so analysis has something to work with.

## Questions: binary, polarity-aware

Every question is answerable as **Yes**, **No**, or **Unknown**. This is a deliberate constraint:

- Binary answers are unambiguous and scoreable.
- "Unknown" preserves epistemic honesty — a respondent who doesn't know shouldn't be forced to guess.
- Scales, Likert ratings, and multi-choice options are encoded *into the question text*, not the answer shape: "Do deploys happen at least daily?" is a binary question that captures a frequency threshold.

**Polarity** flips the scoring direction for a question:

- **Positive polarity** (default): "Yes" means the practice is in place. Scored as a pass.
- **Negative polarity**: "Yes" means a problem exists. Scored as a gap.

Polarity lets us ask "Do you run automated dependency scans?" (positive) and "Do production incidents frequently recur?" (negative) without contorting the language. It's a small feature with outsized expressive value.

A question also carries three optional authoring fields:

- **Guidance** — how to verify. "Check CI configuration for a scheduled Snyk or Dependabot job."
- **Evidence** — what would prove a "yes" answer. "Build logs showing dependency-scan output; Snyk/Dependabot dashboard."
- **Remediation** — what to do if the answer is "no." "Add `snyk test` as a required CI step."

These are author-facing. They feed into the respondent's collection experience and the analysis narrative.

## The answer model

An answer is more than a yes/no. Each answer captures the *why*:

- **Value** — Yes / No / Unknown.
- **Evidence** (when Yes) — which types of evidence support the answer, chosen from the assessment's evidence vocabulary. "Audited/Certified", "Tested periodically", "Process/documentation in place", etc.
- **Blockers** (when No) — what's preventing implementation, chosen from the assessment's blocker vocabulary. "People", "Time", "Technology", "Training", etc.
- **Planned?** (when No) — is remediation scheduled?
- **Notes** — free-text override for anything the structured fields can't capture.

The evidence and blocker vocabularies are **per-assessment, customizable**. Defaults seed a sensible starting set, but a restaurant assessment can add "Temperature logs" and "Health inspection report" to evidence; a compliance assessment can add "External audit" and "Penetration test." This is the metamodel's extension point for domain-specific semantics.

## Roles and effort

Two question-level fields drive the roadmap:

- **Roles** — which disciplines own this question. Freeform descriptive labels (`security-engineer`, `product-manager`, …), chosen per assessment. Multiple roles are allowed; many practices are cross-functional.
- **Effort** — the estimated range of hours to remediate if the answer is "no." A two-number tuple `{min_hours, max_hours}`.

These live on the question (not the practice) because remediation reality is per-question. A single practice might contain a 2-hour config change and a 3-week migration; aggregating them into a single number would lie.

## Respondents

For v1, a single implicit respondent fills out the assessment. The data model carries an explicit `respondent_id` from the start so multi-respondent aggregation (multiple SMEs, agreement scoring) can land as an additive feature later without schema churn.

## What the metamodel is not

- **Not a scale / Likert system.** We bake scales into question text, not answer options.
- **Not weighted by question.** Every question counts equally within its practice. If the SME wants to emphasize a question, they split it or elevate it to its own practice.
- **Not dependency-aware.** Questions don't currently gate each other. A "no" on one question doesn't skip others. We may add this later; it's not in the v1 model.
- **Not a maturity model out of the box.** The scorecard reports percentages. Labels like "Level 3" or "Advanced" are an analysis-layer concern, not a metamodel one.

Each of these is a deliberate simplification, and each might get revisited if a real assessment can't express what it needs to.

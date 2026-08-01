# Analysis philosophy

The analysis layer is where amaker earns its keep. Every other part of the tool exists to make this layer produce something useful. This page articulates how we think about it.

## What makes a good assessment report

A good report answers three questions in order:

1. **Where are we?** — The scorecard. A factual snapshot.
2. **What's wrong?** — The gap inventory. Specific and enumerable.
3. **What should we do?** — The roadmap and narrative. Prioritized, role-assigned, grounded in effort estimates.

Most tools stop at #1 and call it a day. A percentage score is the easiest output to generate and the least useful one to receive. "74% mature" tells you nothing you can act on. We treat the score as the cover page, not the report.

## Deterministic first, narrative second

Four outputs, produced in a strict order:

```
Response ──┐
           ├──► Scorecard    (deterministic aggregation)
Assessment ┤
           ├──► Gap Inventory (deterministic enrichment)
           │
           └──► Roadmap       (deterministic pivoting)
                    │
                    ▼
              LLM Narrative
          (grounded generation)
```

The first three layers are pure functions of `(Assessment, Response)`. They have no LLM dependency, they're fast, they're testable, and they're always correct in the sense that "they compute exactly what they claim to compute." The LLM narrative sits on top, reading those three outputs as its only ground truth.

This ordering is load-bearing. If the LLM ever disagreed with the scorecard, the scorecard wins. The LLM writes prose; it does not rewrite math.

## Polarity-aware scoring

Legacy assessment tools often get this wrong. They count "yes" answers as good and move on. But a question like "Do incidents recur frequently?" has Yes = bad, and scoring it naively inflates the number.

Our scorecard is polarity-aware from the ground up:

- **Positive polarity + Yes** → pass.
- **Positive polarity + No / Unknown / Unanswered** → not a pass (distinct sub-categories).
- **Negative polarity + No** → pass (the problem is absent).
- **Negative polarity + Yes** → gap (the problem is present).

The scorecard tracks yes / no / unknown / unanswered counts separately because they tell different stories:

- A "no" means the practice isn't in place.
- An "unknown" means the respondent doesn't have visibility. This is a *different* gap — a visibility gap — and deserves different remediation (usually: instrument something).
- An "unanswered" means the assessment isn't done. It's noise in the score.

Mixing these three into a single "failing" bucket loses information we've paid to collect.

## Aggregation: weighted by question count

Rolling up from question to practice to domain to assessment, we weight by **question count** at every level:

```
domain_percent = (sum of passing questions in domain)
               / (sum of answerable questions in domain)
```

This is deliberately different from the naive approach — an unweighted
mean of child percents — which treats a practice with 3 questions as
equivalent to a practice with 10. If one practice has ten questions and
nine pass, and a sibling practice has two questions and both fail, the
domain score reflects the 9/12 proportion, not the (90% + 0%) / 2 = 45%
artifact.

## The Gap Inventory is the core artifact

Of the four analysis outputs, the gap inventory is the one we treat as canonical. Everything else is a view of it:

- The scorecard is an aggregation.
- The roadmap is a pivot.
- The narrative is a synthesis.

If the tool produced only one thing, it would produce the gap inventory. Every non-passing question, enriched with:

- The question text and its polarity.
- Inherited CVR from the practice and domain.
- The respondent's blockers and planned flag (for No answers).
- The question's owner roles.
- The question's effort range.
- The question's remediation text, if the author wrote one.

With this in hand, downstream stakeholders can slice and dice. Security team wants the security-owned gaps? Filter by role. Auditor wants every "no" with no planned remediation? Filter by planned=false. Board wants the top 10 highest-risk / lowest-effort? Sort by the priority heuristic.

The gap inventory is also our safety net against LLM hallucination. When the narrative says "the authentication domain has a gap in MFA enforcement," the gap inventory is the check: there had better be a gap whose question mentions MFA. If there isn't, the narrative is wrong.

## The narrative layer is prose generation, not fact generation

The LLM narrative does one job: turn a structured gap inventory into a readable report. It has three jobs it does not do:

1. **It does not invent gaps.** Every name (domain, practice, question) in the narrative must be a subset of the assessment's actual names. We test this with a regex check.
2. **It does not recompute scores.** If the LLM wants to say "this domain is strong," it must justify that claim from the scorecard we fed it.
3. **It does not prioritize differently than the roadmap.** It may add narrative commentary to the roadmap's ordering, but the ordering itself is deterministic.

What the LLM is good for, which we lean on:

- **Executive summary prose.** A paragraph that says "your infrastructure security is strong but your human-process gaps are substantial" is genuinely hard to write algorithmically.
- **Role-targeted narratives.** "For the security engineering team, your three biggest near-term wins are..." — grounded in the roadmap, but phrased as a recommendation.
- **Connective tissue.** Linking related gaps across domains ("both the MFA gap in Authentication and the audit-log gap in Monitoring point to a missing identity-governance capability...") is exactly the kind of pattern an LLM sees well.

## Priority: a heuristic, not a truth

The roadmap's priority ordering is a v1 heuristic: `risk_weight(practice) / effort_midpoint`. The risk weight is derived from the length of the practice's Risk text as a proxy for "how much the author thought the risk mattered." The effort midpoint is the midpoint of the question's `[min, max]` hour range.

This is a placeholder. It will be wrong in specific cases. It's a starting point that's better than arbitrary order, and it gives the LLM narrative layer something concrete to ratify or overrule.

A future version will likely:

- Ask the author to rank domains by importance during authoring (a one-time ranking is a much better signal than text length).
- Let the LLM re-rank gaps based on semantic understanding, subject to the respondent's declared priorities.
- Incorporate "planned" and "blocker type" into the ordering (a People-blocked gap is a different priority than a Time-blocked one).

But for v1, the heuristic is honest about being a heuristic.

## What analysis is not

A few explicit non-goals:

- **Not a forecasting tool.** We don't predict "when will you hit 90%?" We don't know your team's velocity, and we shouldn't guess.
- **Not a comparison tool (yet).** We don't compare your assessment to industry averages. We may one day compare two of your own assessments over time.
- **Not a prescriptive tool beyond your own content.** The LLM narrative doesn't bring in recommendations from outside the assessment's own text. If the practice's `remediation` field says "hire a DBA," we'll say that. If it doesn't, we won't invent one.
- **Not a scoring competition.** There's no leaderboard. There's no "certification." We are producing a report, not awarding a badge.

The goal is to be the tool that makes next week clearer, not the tool that makes a wall plaque.

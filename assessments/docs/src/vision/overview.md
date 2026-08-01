# Vision

## The problem we're solving

Every organization has lists of things they ought to be doing — security controls, operational practices, compliance checks, hiring rubrics, food-safety procedures — and no shortage of frameworks telling them what "good" looks like. What they lack is a practical way to answer two questions that actually move the needle:

1. **Where are we right now?**
2. **What should we do next, and who should do it?**

The gap between "here's a framework" and "here's a Tuesday-morning action list" is usually filled by a consultant with a spreadsheet. That's slow, expensive, and the output decays the day it's delivered. We think this is a job an LLM-assisted tool can do well — if the tool takes the structured-assessment craft seriously and doesn't try to be a chatbot pretending to understand your business.

## What amaker is

**amaker is a collaborative tool for building, administering, and interpreting structured assessments.** A senior SME works with an AI partner to author an assessment tailored to their domain; a respondent fills it out; the tool produces a grounded, actionable report.

The shape of the tool follows the shape of the problem:

- **Authoring** is a conversation. The SME brings the domain knowledge; the AI brings structural discipline (hierarchical decomposition, binary questions, Context/Value/Risk framing). Neither alone produces a good assessment.
- **Responding** is a form — but a form designed to capture *why*, not just *what*. Evidence for "yes" answers, blockers for "no" answers, a "planned?" flag, free-text notes.
- **Analysis** is deterministic first, narrative second. Every insight in the final report traces back to a specific question and answer; the LLM writes the prose, but it does not invent the gaps.

## Who it's for

Two personas, roughly:

- **The SME / assessment author.** Someone who knows a domain deeply and wants to codify their judgment into something repeatable. A security lead who has done this exercise eight times; a food-safety inspector; a compliance officer; an engineering director building a team-maturity rubric.
- **The respondent.** The person (or team) being assessed. They care about three things: clarity on what's being asked, a way to explain themselves (evidence, blockers), and a report they can actually hand to a stakeholder.

These are often different people. The tool should respect that — the author's craft and the respondent's experience both matter, and neither should be sacrificed for the other.

## What we're deliberately not building

- **A generic chatbot that happens to have assessment templates.** The structure is the product. The metamodel (Assessment → Domain → Practice → Question with CVR triads and binary polarity-aware questions) isn't an implementation detail; it's what makes the output useful.
- **A dashboard tool.** We are not trying to be Tableau for compliance. Charts are a downstream artifact; the primary outputs are the structured gap inventory and the narrative report.
- **A one-size-fits-all maturity framework.** The tool is domain-agnostic; it adapts to what the SME brings. The same engine that produces a cloud-security assessment should be able to produce a lemonade-stand readiness check.
- **A benchmarking service.** We may compare an org to itself over time. We are not building a database of industry averages.

## Why an LLM is load-bearing

Three capabilities the LLM contributes that a traditional tool can't:

1. **Conversational authoring.** Producing well-scoped, non-compound, polarity-correct questions is hard. The AI partner catches anti-patterns in real time and rephrases them into the metamodel's shape, while still deferring to the SME on what's worth asking.
2. **Domain-appropriate vocabulary.** Evidence for a cloud security audit is "pen-test report" and "SIEM coverage"; evidence for a restaurant is "temperature logs" and "training certificates." The tool customizes its vocabulary per assessment because the AI can generalize across domains.
3. **Narrative synthesis.** Turning a structured gap inventory into an executive summary, a role-ordered action list, and a prioritized roadmap is genuinely writing. The deterministic layer produces the facts; the LLM produces the prose. Both are necessary.

## Guiding principles

These show up throughout the design:

- **Structure first, prose second.** Every narrative sentence is backed by a structured fact. The gap list is always visible alongside the narrative.
- **Grounded outputs.** The LLM works from the assessment's actual content — domain/practice names, CVR text, answers — and does not invent gaps, scores, or recommendations.
- **The "why" is a first-class citizen.** The Context-Value-Risk triad at every level of the hierarchy is the narrative spine. It's captured during authoring and paid forward during analysis.
- **Answers are sacred.** A response binds to the published version it was administered against. The author can keep editing the draft and publishing new versions; a response in progress always reads its bound version, so it's never disturbed. We never silently orphan someone's work.
- **Single-user today, team-aware tomorrow.** The data model carries respondent identity from day one, even though v1 is single-user. Multi-SME aggregation will be an additive change, not a migration.
- **Greenfield until it ships.** Until we have real users with real assessments, nothing is load-bearing. We delete freely and move fast.

The rest of this section fleshes out the pieces: the [core concepts and metamodel](./concepts.md), the [lifecycle](./lifecycle.md) from authoring to analysis, our [analysis philosophy](./analysis.md), and the [roadmap](./roadmap.md) for where this is going.

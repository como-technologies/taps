# Where we are, where we're going

A team-facing snapshot of state and direction. This page drifts as we
ship; treat the GitHub issue tracker as authoritative for the
week-to-week.

## Where we are

**All three acts work end-to-end.** An SME authors an assessment with the
AI, publishes a version, hands a respondent a link to fill it out, and
everyone can view the scorecard / gaps / roadmap / narrative. The
metamodel, conversational authoring tools, surgical CRUD edits, the live
preview, the collection form, the deterministic analysis layer, and the
LLM narrative are all shipped.

**The system is three binaries** — `amaker-author`, `amaker-assess`,
`amaker-analyze` — over a shared `amaker-core`. They hold no local state:
storage is a blob store (`object_store`, with filesystem / S3-compatible
/ GCS / in-memory backends), versioning is immutable version objects, and
concurrency is conditional writes. That makes each one a stateless
container.

**It deploys to Cloud Run.** Containerized, structured logging, health
checks, graceful shutdown, native GCS storage — see the
[Operations](../operations/gcp-setup.md) section.

**What's still thin:**

- Single respondent only — no multi-respondent aggregation yet.
- No snapshots / trend-over-time.
- Export is YAML / JSON / TOML; no PDF.
- Auth is all-or-nothing: IAP gates the whole deployment to a Google
  allowlist, so respondents must be allowlisted accounts — there's no
  per-assessment respondent invite link yet.
- LLM rate-limit handling is unpolished (a 429 can surface as a 502).

## Near-term

Quality and robustness on what's already shipped:

- **Graceful LLM rate-limit / overload handling**
  ([taps#14](https://github.com/como-technologies/taps/issues/14)) —
  a 429/529 currently leaks a 502 to the user and can strand partial
  work. Categorize the failure, back off, show a chat-shaped message,
  keep the user's input.
- **Shrink per-turn input tokens**
  ([taps#15](https://github.com/como-technologies/taps/issues/15)) —
  the agent loop resends a growing history each round-trip; trimming it
  keeps turns under provider rate limits.
- **Suggested-reply pills**
  ([taps#16](https://github.com/como-technologies/taps/issues/16)) —
  occasionally missing when the assistant ends a turn with a yes/no
  question.

## Later, but likely

- **Multi-respondent.** The schema already carries `respondent_id`. The
  work is UI + aggregation: per-respondent views, agreement scoring,
  role-scoped invitations.
- **Snapshots and trend.** Freeze an analysis as a dated snapshot; re-run
  the assessment later and show the delta — the "assessment as repeatable
  ritual" use case.
- **PDF report generation**
  ([taps#12](https://github.com/como-technologies/taps/issues/12)) —
  Typst-based, taking the deterministic analysis data and the LLM
  narrative as inputs.
- **Assisted answering.** The respondent uploads internal documentation;
  the AI proposes answers with citations; the respondent approves or
  overrides. The respondent-side analog of authoring's AI partnership.
- **Multi-choice / scale question kinds**
  ([taps#13](https://github.com/como-technologies/taps/issues/13)) —
  if real users genuinely need non-binary answer shapes, extend the
  metamodel rather than contort question text.

## Horizons we're watching but not committing to

- **Cross-assessment analytics.** Comparing one assessment over time, or
  the same assessment across two teams.
- **Assessment library / marketplace.** A curated public set (SOC 2
  readiness, WCAG compliance, OWASP ASVS) as a distribution channel.
- **Domain-specific integrations.** An assessment that can call a
  provider's APIs to check facts directly — semi-automated answering.
- **Facilitated multi-stakeholder sessions.** A live workshop where
  several people answer collaboratively.
- **Custom scoring rubrics.** Weighted questions, maturity levels,
  dependency-aware question flow — the metamodel flags all of these as
  later extensions.

## Decisions still on the table

- **Where does the priority heuristic evolve?** The `risk_weight / effort`
  placeholder will be wrong often enough to bother someone. Ask the
  author to rank domains during authoring? Let the LLM re-rank with
  justification? Both?
- **How opinionated should vocabulary tailoring be?** The AI can propose
  domain-specific evidence/blocker types during Scoping — proactively, or
  only when the SME notices the defaults don't fit?
- **PDF vs. HTML vs. both for export?** Typst ([taps#12](https://github.com/como-technologies/taps/issues/12))
  produces PDF; an interactive HTML report might serve some audiences
  better.
- **Respondent identity and multi-tenancy.** The deployment is gated by
  IAP today ([Authentication](../operations/auth.md)) — fine for an
  internal allowlist, but "send an outside respondent a link" needs
  per-assessment invite tokens, and a multi-org future needs a
  `tenants/{id}/…` storage prefix. Both deferred; the storage layout
  leaves a clear path to the tenant prefix.

## How to use this section

Treat these pages as the north star. When a small task comes up, check it
against the principles in [Vision](./overview.md) and the
[Core concepts](./concepts.md). When a big task comes up, check it
against the [Lifecycle](./lifecycle.md) and [Analysis](./analysis.md)
pages.

If you find yourself doing work that doesn't fit, either the work is
wrong (pause and discuss) or the vision is out of date (edit these
pages). What isn't fine is the work and the vision silently diverging.

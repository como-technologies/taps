# Team Effort Analysis from GitHub Activity

## Philosophy: Team-Level Effort Analysis

This tool analyzes **team capacity investment** rather than individual time tracking.

**Why team-level?**

- **Focus on outcomes**: "Is backend architecture worth 35% of capacity?" not
  "How many hours did Bob spend?"
- **Prevent dysfunction**: Individual metrics create perverse incentives and
  bikeshedding conversations
- **Enable value mapping**: Connect burn rate to business outcomes and
  strategic decisions
- **Respect flow**: Developers work without surveillance overhead

**The result**: Leadership understands where capacity flows and can make
informed trade-offs about feature investment, tech debt, and business value.

**What we measure:**

- Team capacity allocated to work categories (features, bugs, tech debt, etc.)
- Cost of different outcomes (architecture changes, integrations, refactors)
- Effort distribution across the portfolio

**What we don't measure:**

- Individual developer productivity
- Time spent per person
- Performance comparisons between team members

## Assumptions

- All development work can be tied back to artifacts in GitHub
- PR-based workflow will remain our standard practice
- Team members will provide honest relative effort scores
- Work categories will be consistently labeled on GitHub PRs
- This approach can be extended to other work tracking systems (out of scope
  for this proposal)

## Problem Statement

We need to understand how team capacity is invested across
actual work performed, enabling leadership to map burn rate to business outcomes.
The challenge: a 5-minute typo fix and a 5-day architectural change look similar
in git history. Traditional metrics (lines of code, commit count, individual time
tracking) are unreliable indicators of effort investment and often lead to
dysfunctional behavior.

## Proposed Solution

Implement a simple effort scoring system at the PR level using a 1-5 duration
scale that developers self-report:

**Duration Scale (Relative Effort):**

1. Super Quick
2. Not Long
3. Average
4. A While
5. Felt Like Forever

_Note: These are relative measures. Over time, a "1" might normalize to 2 hours
while a "3" could represent 3 days of work, depending on team patterns._

**Implementation:**
Add to PR template:

```markdown
## Effort Score

How long did this PR take to complete?

- [ ] 1 - Super Quick
- [ ] 2 - Not Long
- [ ] 3 - Average
- [ ] 4 - A While
- [ ] 5 - Felt Like Forever
```

**Monthly Processing Example:**

_Month's PRs:_

- PR #101 (Mike): "Migrate to domain architecture" - Score: 5
- PR #102 (Brett): "Fix cart calculations" - Score: 2
- PR #103 (Mike): "Update documentation" - Score: 1
- PR #104 (Matt): "Add payment integration" - Score: 4
- PR #105 (Brett): "Refactor auth module" - Score: 3

_Normalization:_

- Total effort points: 15
- Allocate `MONTHLY_HOURS` proportionally: Each point = `MONTHLY_HOURS/15`
- PR #101 = 33% of hours, PR #102 = 13% of hours, etc.

_Category Allocation:_
If `MONTHLY_HOURS = 360`:

- PR #101 = 120 hours → GitHub labels: "backend-architecture", "testing"
- PR #102 = 48 hours → GitHub labels: "bug-fix"
- PR #103 = 24 hours → GitHub labels: "documentation"
- PR #104 = 96 hours → GitHub labels: "feature", "integration"
- PR #105 = 72 hours → GitHub labels: "refactoring", "security"

Client receives breakdown: Feature (96 hrs), Backend Architecture (120 hrs),
Bug Fixes (48 hrs), etc.

**Note on Category Allocation:** Categories are derived from GitHub PR labels
(excluding the effort score and ADR labels). All other labels on a PR are
treated as category labels, and the PR's allocated hours are distributed
across these categories equally.

**Note on ADR Attribution:** When work is prompted by an Architecture
Decision Record, PRs arrive tagged with the ADR reference and the report
attributes the measured effort back to that decision:

- `adr:<reference>` label (e.g. `adr:ADR-0012`) — primary source. If a PR
  carries several ADR labels, the first one wins and a warning is logged.
- `Adr-Reference: <reference>` trailer line in the PR body — fallback used
  only when no `adr:` label is present.
- `[ADR-0012] ` title prefix — a redundant carrier for human readers; it is
  not parsed.

A PR has at most one ADR, so its **full** allocated hours are credited to
that ADR (`adr_totals` in the report) — unlike categories, there is no
splitting. `adr:*` labels are excluded from work-type categories so ADR ids
do not dilute the category split.

**Benefits:**

- No separate time tracking or individual surveillance
- Integrated into existing PR workflow
- Self-normalizing effort scores over time
- Categories pulled directly from GitHub PR labels
- Enables "cost of outcomes" conversations for leadership
- Respects developer autonomy and flow
- Simple to implement and adjust

## Machine-Readable Export

The monthly report can be fetched as JSON without the dashboard UI. The
server exposes it at a stable endpoint, `POST /api/export_report`:

```sh
curl -sS -X POST http://127.0.0.1:8080/api/export_report \
  -H 'Content-Type: application/json' \
  -d '{
    "config": {
      "monthly_hours": 360.0,
      "repositories": ["my-repo"],
      "organization": "my-org",
      "github_token": "<token>",
      "year": 2026,
      "month": 5,
      "scaling_series": "Linear"
    }
  }'
```

`scaling_series` is one of `Linear`, `Doubling`, `Fibonacci`, `Exponential`,
`TShirtSizes`, or `Square`. The response body is the serialized
`MonthlyReport`: summary figures, `allocations` (each with its `adr_id`),
`category_totals`, `adr_totals` (full allocated hours per ADR reference),
and the unallocated-PR quality-control list.

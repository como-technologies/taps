# ADR-0003: Ingest merged PRs through a read-only PrSource trait with GitHub and Gitea providers

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner; tuesday maintainers; conduit maintainers
(the dogfood forge tuesday will read is conduit's demo Gitea).

## Context and Problem Statement

tuesday's ingestion is GitHub-only, with `api.github.com` hardcoded at three
call sites (REST org/repo listing, GraphQL merged-PR search), and the
calculator consumes GitHub-GraphQL-shaped types directly. The portfolio sells
forge-neutrality, and the Measure stage is the one place that promise is
silently broken. The dogfood loop additionally requires reading conduit's
local Gitea forge — offline, free, and the only path exercisable end-to-end
without external accounts. The ingestion boundary needs a seam.

## Decision Drivers

- Forge-neutrality at the Measure stage (the portfolio's stated pitch).
- Measure is **read-only** — tuesday must never mutate a forge.
- The local dogfood path (Gitea on `localhost:3000`) must be the primary,
  tested path; the GitHub web app remains the partner-facing head.
- GitHub-specific GraphQL shapes must not leak into the calculator.
- The shared artifact between portfolio apps is the label/title/trailer
  contract, never code — tuesday must not import conduit's `Forge` trait.

## Considered Options

- **Define tuesday's own minimal read-side `PrSource` trait**:
  `list_orgs` / `list_repos` / `fetch_merged_prs(owner, repo, month window)
  -> Vec<MergedPr>`, with a neutral domain type
  `MergedPr { number, title, body, url, merged_at, labels: Vec<String> }`;
  GitHub and Gitea providers implement it.
- **Reuse conduit's `Forge` trait** — already proven across two forges, but
  it is mutation-heavy (issues, PRs, labels out) and sharing it couples the
  apps; Measure needs none of its write surface.
- **Keep ingestion GitHub-only** and convert Gitea data offline — abandons
  the forge-neutral pitch where it is cheapest to honor.

## Decision Outcome

Chosen: **tuesday's own read-only `PrSource` trait with a neutral `MergedPr`
type and GitHub + Gitea providers**, because Measure's needs are three read
operations, a read-only trait makes forge mutation unrepresentable in
tuesday, and a neutral domain type quarantines GraphQL shapes inside the
GitHub provider (cutting the `calculator.rs` → `github::PullRequest` import
is the seam). The Gitea provider is plain REST v1 + token: paginated
`GET /api/v1/repos/{owner}/{repo}/pulls?state=closed`, client-side
`merged_at` window filter (Gitea has no merged-date search), labels inline,
`Authorization: token <tok>` read from a gitignored token file. No OAuth for
Gitea — OAuth stays a GitHub-web-head concern.

### Positive Consequences

- Forge-neutral Measure: the same calculator runs on GitHub and Gitea data.
- The dogfood loop closes locally and free of external accounts.
- The calculator becomes testable against hand-built `MergedPr` fixtures with
  no provider in sight.
- Read-only by construction — no mutation method exists to misuse.

### Negative Consequences

- Two providers to maintain and test; Gitea API quirks (client-side merge
  window, label payload shape, pagination caps) demand recorded-fixture tests.
- A trait with two implementations invites premature generalization; the
  trait must stay at exactly the three read operations Measure needs.
- Client-side month filtering on Gitea reads more pages than the GitHub
  GraphQL search does for the same window.

## Implementation

Milestones M2 (trait + neutral types + `GithubSource` wrapper, with unit
tests) and M3 (GiteaSource: base_url + token constructor, `/api/v1`
pagination, merged-window filter, label mapping; recorded-fixture tests plus
a live test behind `TUESDAY_E2E_GITEA=1` against conduit's demo forge).

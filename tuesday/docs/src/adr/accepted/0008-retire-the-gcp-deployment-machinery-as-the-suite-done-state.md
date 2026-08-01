# ADR-0008: Retire the GCP deployment machinery as the suite-done state

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies portfolio owner (the only party authorized to publish or
host anything); tuesday maintainers; SMEs self-hosting tuesday against
their own forge.

## Context and Problem Statement

tuesday carries a complete Google-Cloud deployment pipeline from its
pre-portfolio life: `cloudbuild.yaml` (Cloud Build → Artifact Registry),
`.gcloudignore`, and three scripts (`new-gcp-project.sh`,
`delete-gcp-project.sh`, `deploy-to-cloud-run.sh`). The iteration-1
direction froze these paths — parked, unmaintained, awaiting an explicit
decision — and the portfolio referee's matrix has carried "GCP machinery
absent, retired by ADR" as an open FAIL cell since. The forces: the
portfolio's local-first mandate; the owner-only-publishing rule (remote or
cloud deployment is not something the loop may operate); tuesday's target
rung is SME-usable, which means an external engineering lead points tuesday
at THEIR forge from a local build — not a Como-hosted service. Frozen
half-dead deploy paths are worse than absent ones: they imply a hosted
story nobody maintains, and `cloudbuild.yaml` already references a
`tools/dx` binary that does not exist in the repo.

## Decision Drivers

- Local-first mandate: SME-usable requires a local build and the book's
  quickstart, no Como-machine or Como-cloud assumptions.
- Owner-only publishing: the loop must not own (or appear to own) a cloud
  deployment path it can never exercise.
- Honest-maturity rule: the repo should carry no machinery its CI never
  builds and its docs cannot truthfully document as working.
- A *generic* self-host story must survive: SMEs who want a long-running
  instance need a documented, forge-agnostic path.
- Recovery must stay cheap if the decision is reopened.

## Considered Options

- **Retire and delete the GCP-specific machinery; keep the generic
  self-host path**: delete `cloudbuild.yaml`, `.gcloudignore`, and the
  three GCP scripts; keep `Containerfile` and
  `scripts/build-static-release.sh` documented in the book as the generic
  self-host story.
- **Keep the files frozen** (status quo): zero deletion risk, but the
  matrix cell stays red forever, the docs keep hedging, and the repo ships
  scripts with hardcoded Como billing/organization IDs that no one runs.
- **Maintain the GCP path** (un-freeze, add CI): invests exactly where the
  direction says not to — Cloud Run hosting serves no rung below self-serve
  and violates owner-only publishing.
- **Delete everything including the Containerfile**: simplest tree, but it
  kills the generic self-host story SMEs may legitimately want and the
  static-release script the book documents.

## Decision Outcome

Chosen: **retire and delete the GCP-specific machinery, keeping the
generic self-host path**, because the GCP files are provider-specific
plumbing for a hosting model the portfolio explicitly does not operate,
while `Containerfile` + `scripts/build-static-release.sh` are
provider-neutral and serve any SME on any container host.

This is the **suite-done state** for tuesday's deployment story: no further
deployment work is scheduled or implied by any milestone. Concretely:

- Deleted from `main`: `cloudbuild.yaml`, `.gcloudignore`,
  `scripts/new-gcp-project.sh`, `scripts/delete-gcp-project.sh`,
  `scripts/deploy-to-cloud-run.sh`.
- Kept and documented (the book's "Running tuesday" page):
  `Containerfile` (generic OCI build of the fullstack server) and
  `scripts/build-static-release.sh` (the `dx build --fullstack --release`
  static bundle).
- Recovery is git history: the deleted files remain reachable at this
  commit's parent.

**Reopen criteria** — this decision is revisited only if one of these
becomes true:

1. The portfolio owner explicitly decides Como will operate a hosted
   tuesday (a self-serve-rung investment, owner action, not loop work).
2. A paying engagement requires a Como-operated instance.
3. The suite mandate changes such that some app's rung requires hosted
   multi-tenant deployment.

Reopening means a fresh ADR superseding this one; the recovered GCP files
would be a starting point, not a contract.

### Positive Consequences

- The matrix cell closes truthfully: no frozen machinery, one documented
  self-host story.
- The repo stops shipping scripts with Como-specific GCP organization and
  billing IDs.
- The book can describe every committed file honestly.

### Negative Consequences

- Re-establishing a GCP pipeline later costs real work: the recovered
  files will have drifted from current GCP/Cloud Build reality.
- The `Containerfile`'s `tools/dx` COPY remains aspirational until someone
  exercises a container build (it is documented as the generic recipe, not
  CI-verified).

## Implementation

Carried out in the same change that records this ADR: the five files
deleted; the "frozen deploy machinery" comments in
`crates/tuesday-web/Cargo.toml`, `crates/tuesday-cli/Cargo.toml`, and the
CLI sources rewritten to name the surviving self-host story; the book's
introduction drops the frozen-machinery hedge; the "Running tuesday" page
documents CLI build, `dx serve`, the static release bundle, and the
container path. Scheduling and Gitea-OAuth retirements are recorded
separately (ADR-0009, ADR-0010).

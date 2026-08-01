# ADR-0010: Retire Gitea OAuth: token auth is the web head's only Gitea path

> State: Accepted

## Status

Accepted

## Stakeholders

tuesday maintainers; SMEs connecting the web head to a self-hosted Gitea;
the portfolio owner (the SME-usable rung definition: token auth on both
forges).

## Context and Problem Statement

The web head's GitHub integration has two ways in: paste a personal access
token, or the GitHub-App OAuth flow under `src/auth/`. When the Gitea
provider landed (ADR-0003), the question followed: does Gitea get an OAuth
flow too? The iteration-1 direction already drew the line — OAuth is a
GitHub-app-head-only concern — and the iteration-2 direction lists Gitea
OAuth among the retirements to record by ADR so the line is a decision,
not an accident of what got built first. Forces: every self-hosted Gitea
instance would need an OAuth application registered against tuesday's
redirect URL (per instance, per origin — exactly the kind of Como-side
operational coupling the local-first mandate avoids); the CLI's
contract-pinned token handling (`--token-file` → `TUESDAY_GITEA_TOKEN` →
the documented conduit secrets fallback) already defines tuesday's Gitea
auth story; and Measure is read-only, so the scopes OAuth would broker are
a single read-scoped token anyway.

## Decision Drivers

- One Gitea auth story across both heads: the web card must match the
  CLI's token contract, not fork it.
- Self-hosted Gitea means per-instance OAuth app registration — an
  operational burden on the SME with no payoff at a read-only rung.
- Measure never writes: a read-scoped API token is the entire required
  capability.
- The SME-usable rung requires "token scopes documented for both forges",
  not interactive identity flows.
- The existing GitHub OAuth flow stays: it predates the portfolio, works,
  and binds to the one fixed-origin forge where app registration is done
  once, centrally.

## Considered Options

- **Token-only for Gitea (retire OAuth)**: the Settings card takes
  instance URL + API token; anonymous read allowed where the instance
  permits it. OAuth remains a GitHub-head-only concern.
- **Build Gitea OAuth**: symmetric with GitHub on paper, but each SME must
  register an OAuth application on their instance before the first report
  — a setup wall in front of a read-only tool.
- **Retire OAuth on both forges**: maximal symmetry the other way; killing
  a working GitHub flow gains nothing and breaks existing GitHub-app
  users.

## Decision Outcome

Chosen: **token-only for Gitea**, because the web card then matches the
CLI's contract-pinned token handling exactly (one documented auth story per
forge), and the only thing OAuth would add at a read-only rung is a
per-instance registration step the SME has to perform before tuesday works
at all. Gitea tokens are generated in the instance UI (Settings →
Applications → Generate New Token, read scope) — a one-minute, no-redirect
path that works identically on every self-hosted instance.

This closes the Gitea-OAuth option as a decided non-feature: the
`src/auth/` OAuth flow is and stays GitHub-only.

### Positive Consequences

- One Gitea credential story across CLI, export endpoint, and web card —
  documented once (token scopes in the book), tested once.
- No per-instance OAuth app registration for SMEs; the quickstart's Gitea
  leg is "generate token, paste".
- No OAuth callback/redirect surface against arbitrary self-hosted
  origins.

### Negative Consequences

- Pasted tokens live in browser storage rather than behind a brokered
  flow; the documented mitigation is read-scope-only tokens.
- No identity-based ergonomics on Gitea (auto-discovery of the user's
  orgs still works, but via the token, not an OAuth identity).
- If a future rung demands Gitea SSO/OIDC, this ADR must be superseded
  rather than quietly contradicted.

## Implementation

Already true in code — the Gitea Settings card is URL + token with no
OAuth wiring; this ADR records the line so it cannot drift. The book's
Configuration page keeps documenting Gitea as "exactly one way in"
(instance URL + API token, read scope), and the quickstart documents the
token scopes for both forges.

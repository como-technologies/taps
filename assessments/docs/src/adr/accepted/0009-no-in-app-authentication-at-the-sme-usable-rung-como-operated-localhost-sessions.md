# ADR-0009: No in-app authentication at the SME-usable rung: Como-operated localhost sessions

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies engineering (the assessments maintainers own this
decision); the facilitators who run SME sessions against this app; the
portfolio badge process, whose SME-usable criteria require the auth story
to be decided by an accepted ADR.

## Context and Problem Statement

assessments is moving to the SME-usable rung: an external subject-matter
expert drives the web co-authoring flow with Como alongside. That raises
the question every web app eventually faces — who may reach it, and as
whom? The app currently has no authentication, no user accounts, and no
tenancy: every project under `DATA_DIR` is readable and writable by
whoever can reach the port. Building login, sessions, and multi-tenant
isolation now would be a large investment serving no current user — but
shipping an unauthenticated web app without a *recorded operating model*
would be negligence, not minimalism. The auth story has to be decided
explicitly: build it, or scope it out by design with named boundaries.

## Decision Drivers

- The product thesis at this rung is *facilitated* co-authoring: a Como
  engineer operates the deployment and sits in the session — there is no
  unattended, internet-facing use case to defend
- The default posture must be safe out of the box: nothing listens beyond
  the operator's machine unless deliberately changed
- Hosted pilots (a remote SME) must have a workable, documented path that
  does not require building auth into the app
- In-app authn/authz and multi-tenant accounts are real products of work;
  half-building them (a hardcoded password, a shared token) would create
  false confidence and review burden without real isolation
- The boundary must be auditable: where auth lives, and what rung would
  force the decision to be revisited

## Considered Options

- **No in-app auth, by design**: Como-operated deployments only; the app
  binds `127.0.0.1` by default; remote SMEs reach a facilitated session
  through a trusted channel (SSH tunnel, or a TLS reverse proxy that
  carries the authentication for hosted pilots); authn/authz and tenancy
  are recorded as self-serve preconditions
- **Build in-app authentication now**: accounts, sessions, per-project
  authorization, and the multi-tenancy to make them meaningful
- **A stopgap shared secret**: a single password or bearer token in front
  of the existing app

## Decision Outcome

Chosen: **no in-app auth, by design**, because at this rung the operator
*is* the access control — a Como engineer runs the binary, on their
machine, for a session they facilitate — and the deployment boundary can
carry the rest.

The operating model, concretely:

- **Local by default.** The server binds `HOST=127.0.0.1` (the shipped
  default in `src/config.rs`); nothing is reachable off the machine
  unless the operator explicitly rebinds.
- **Facilitated sessions.** An SME either sits with the facilitator
  (localhost), or — for a remote/hosted pilot — reaches the app through a
  channel the operator controls: an SSH tunnel
  (`ssh -L 3000:127.0.0.1:3000 …`) or a TLS-terminating reverse proxy
  that performs the authentication itself (basic auth, OIDC at the proxy,
  or a private-network/VPN exposure). The app behind it stays
  single-tenant and auth-free; the proxy is the auth story for hosted
  pilots.
- **The boundary is a precondition, not a debt.** In-app authn/authz and
  multi-tenant accounts are *self-serve preconditions*: the rung where
  strangers operate the app unattended is exactly the rung where this
  decision must be superseded. Until then, building auth would gold-plate
  past the suite-done bar.

The stopgap shared secret was rejected as the worst of both worlds: it
looks like security, isolates nothing (one secret, every project), and
would still have to be torn out for real accounts. Building full auth now
was rejected as serving no user this rung has.

### Positive Consequences

- The default deployment is safe without configuration: loopback-only
- Zero auth code to maintain, audit, or get wrong at a rung where the
  operator controls every access path
- The hosted-pilot path uses boring, auditable infrastructure (SSH/TLS
  proxy) instead of bespoke in-app auth
- The self-serve boundary is recorded: the next rung knows exactly what
  it must build before strangers touch the app

### Negative Consequences

- An operator who rebinds `HOST` to a public interface exposes every
  project with no second factor — the docs must say so plainly, and do
- Hosted pilots inherit the operational burden of the tunnel/proxy
  (certificates, accounts at the proxy) instead of a turnkey login page
- No per-SME identity exists: chat history and projects carry no author
  attribution beyond the session itself
- A future self-serve push must supersede this ADR and build authn/authz
  and tenancy from scratch — deliberately deferred, not avoided

## Implementation

Already true in code: `Config::from_env` defaults `HOST` to `127.0.0.1`
(overriding requires an explicit env change). Recorded in the book:
[Configuration](../../configuration.md) documents the operating
model and the rebind warning, and the facilitated-session walkthrough
page documents the tunnel path for remote SMEs. Revisit trigger: any move
toward the self-serve rung (unattended external users) supersedes this
decision.

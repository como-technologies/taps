# ADR-0014: Incus replaces Docker as the suite's container substrate

> State: Accepted

## Status

Accepted

## Stakeholders

Suite owner (runs the dogfood walks on incus); conduit maintainer (the
demo forge is the one product Docker dependency); guide readers (the
prerequisites they install); CI (the Gitea conformance lane's runtime).

## Context and Problem Statement

The suite used Docker in one place — conduit's throwaway Gitea forge
(compose in the demo kit, `forge-up`/`forge-down`) — while the
onboarding dogfood effort (portfolio ADR-0013) began using incus system
containers as its clean-room: walk the Getting Started guide in a fresh
container, throw it away, repeat. Running both runtimes on one host
promptly demonstrated why that is a bad idea: Docker sets the kernel's
iptables `FORWARD` policy to DROP, silently killing routed traffic for
every other bridge on the machine — the incus container got an address
and local DNS but no outbound connectivity, a classic, documented,
endlessly re-discovered conflict.

## Decision Drivers

- One container runtime on a developer machine, not two fighting over
  the same netfilter tables.
- The clean-room testing model (fresh system container per walk) is
  central to how the suite is now dogfooded — incus is doing that job
  well; Docker's job is one throwaway Gitea.
- The team runs Linux; incus's Linux-only nature costs nothing today
  (the same posture as the root justfile's watcher — cross-platform
  concerns are tracked, not preemptively engineered).
- Incus runs OCI images natively, so "retool the forge" is a small
  translation, not a rewrite.

## Considered Options

- Keep both runtimes and firewall around the conflict (`DOCKER-USER`
  accept rules for the incus bridge, persisted across reboots).
- **Go all-in on incus**: retire Docker from the host, the guide's
  prerequisites, and — as the walk reaches them — the products.
- Move clean-room testing into Docker instead (containers-in-Docker
  for system testing is exactly what system containers do better).

## Decision Outcome

Chosen: **all-in on incus**. Docker is retired suite-wide.

- Host and guide: the Getting Started prerequisites install `incus`,
  never `docker.io`; the clean-room walk containers are incus system
  containers.
- conduit's throwaway Gitea forge retools from compose to incus's OCI
  support (taps issue 48) **when the walk's M3 reaches Adopt** — per
  the walk protocol, not before. Until then the demo kit still says
  Docker and says so honestly.
- Anything else that reaches for Docker in the future reaches for
  incus instead.

### Positive Consequences

- One runtime, one set of firewall expectations; the bridge-networking
  failure mode is gone at the root.
- The dogfood clean-room and the product tooling share one substrate —
  the walks exercise the same runtime the products use.
- Throwaway system containers (full init, real users, real sshd if
  needed) are a better fidelity match for "a new user's fresh machine"
  than app containers ever were.

### Negative Consequences

- conduit's demo and the `CONDUIT_E2E_GITEA` lane are Docker-shaped
  until issue 48 lands (accepted: M3 is the deadline and the walk
  protocol enforces it); CI runners will need incus installed.
- Contributors on macOS/Windows lose the Docker path entirely —
  accepted: the team is Linux, and the platform question is already
  tracked in the watcher/prereq issues (45, 47).

## Implementation

Landed with this decision: the host de-dockered; the Getting Started
prerequisites block installs incus; taps issue 48 filed to carry the
conduit retool at M3. The demo kit, OPERATIONS.md, and the conduit book
change with issue 48, not with this ADR.

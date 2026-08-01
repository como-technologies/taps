# Adopt

This is where decisions meet your teams, your code, and your platforms —
and historically where modernization programs stall: the decisions are
accepted, and then nothing changes. The portfolio's answer is an engine
plus the [services](../services.md) that wrap it.

## conduit

An agentic delivery engine. conduit reads an accepted decision and its
stored implementation plan from the knowledge base and drives a coding
agent to turn it into issues and reviewable pull requests — inside your
*own* forge (GitHub, GitLab, or self-hosted Gitea), with your own model
and cloud. It is not another place your code goes; it is a worker inside
the workflow you already have.

Humans hold every gate, by construction:

- **Scope** — nothing runs until a reviewer reads the posted plan and
  explicitly labels it to go.
- **Review** — every change arrives as a pull request under your own
  review tooling; review rounds are the designed path.
- **Merge** — only a human can merge. conduit has no merge capability at
  all, so the gate isn't policy, it's physics.

Every merged PR is tagged with the decision that prompted it, which is
what lets the [Measure](./measure.md) stage answer *what did this
decision cost?* later — the thread, carried across the loop's weakest
seam.

Como runs conduit on its own work every iteration — the engine is
exercised the same way it's offered. Demos, evidence, and internals live
in the [conduit repo](https://github.com/como-technologies/conduit).

# ADR-0015: Conduit narrows to a harness-first execution store

> State: Accepted

## Status

Accepted

## Stakeholders

Suite owner (called the redesign at the walk's M3 frontier); conduit
maintainer (the product this reshapes); tuesday (Measure's attribution
carrier changes); guide readers (Step 5 rewrites from the new shape);
the PM harness role this decision names.

## Context and Problem Statement

The dogfood walk (#46) reached Step 5 — Adopt — and found it
double-blocked: the throwaway Gitea forge is still Docker (#48, held
for exactly this moment by ADR-0014), and conduit still shells out to
an adroit surface deleted by the greenfield rebuild (#112). Fixing
both as scoped would restore conduit as it is: a forge-integrator
that reads decisions through a subprocess seam, files issues on a
forge, drives a coding engine, and opens PRs through per-forge
adapters — most of which are dry-run by design because they cannot be
trusted with a real forge.

Meanwhile every other product the walk has touched came out the other
side in the same shape: a store plus doors, the harness doing the
reasoning, humans at the seats. Issue 53 made llm-wiki an appliance
behind transports; #85 made amaker harness-first; #93 rebuilt adroit
as the sole writer of its KB classes with three doors off one clap
definition. Conduit is the last pre-lesson product, and the walk is
standing at its door. The suite's stated priority is simplicity
toward MVP.

## Decision Drivers

- Harness-first is the plan of record (ADR-0010); conduit is the last
  product still built as an orchestrator rather than doors for one.
- Forge adapters are the largest surface in conduit and the least
  trustworthy — GitHub/GitLab mutations are permanently dry-run;
  only a throwaway Gitea is driven for real.
- The class-ownership machinery (#65), the transport-client pattern
  (#64), and the one-clap-three-doors pattern (#85) already exist;
  work tracking can reuse all three instead of growing a store.
- Integration and replication are solved problems in git — mirroring
  an internal repo to any forge is existing tooling, not product code,
  and can happen later without blocking the loop.
- The walk protocol: work what blocks, when it blocks, in the
  simplest shape that lets the step walk clean.

## Considered Options

- **Repair the current conduit**: retool the Gitea forge to incus
  (#48 as scoped), replace the adroit shell-out with a KB read
  (#112 as scoped), keep the adapter architecture.
- **Narrow conduit and add a fourth product** — an AI project-manager
  binary that plans work between adroit and conduit.
- **Narrow conduit, keep the human gate at the diff**: internal git,
  the human reviews every branch at the terminal and merges by hand.
  Rejected: diff-review vigilance decays with volume into a rubber
  stamp, and a rubber stamp launders accountability — the gate must
  not depend on a human staying awake.
- **Narrow conduit to a harness-first execution store; the PM is a
  role, not a product; humans gate intent, not diffs.**

## Decision Outcome

Chosen: **conduit narrows to a harness-first execution store**. It
stops being a forge-integrator that drives a coding engine and
becomes the Adopt-stage store plus doors: the skills and tools a
harness session uses to track and execute shovel-ready work,
enforcing the workflow and keeping the KB clean. The harness is the
coding engine.

1. **Work items are conduit-owned KB classes.** Projects, epics,
   stories, tasks — the exact taxonomy is schema-design work — live
   in the space behind the appliance, registered on first contact
   per the class-ownership rule. Each level is a goal at an
   altitude plus its verification form: project goals in terms a
   business executive understands, verified by Measure; story goals
   as behavior specifications (BDD); task goals narrow and concrete,
   measured by test sets — unit, integration, performance (TDD).
   Frontmatter carries the metrics tuesday needs: Measure reads
   pages and the graph's task-to-decision edges, not forge PRs.
2. **The PM is a role, not a product.** A harness session wearing
   the PM posture reads accepted `decision` pages and the landscape
   through the engine's doors, self-verifies its proposals for
   consistency against the KB, and presents goal deltas for
   confirmation — then writes work items through conduit's doors,
   the mirror of the authoring session writing decisions through
   adroit's. If the role ever needs its own binary, use will show
   it.
3. **Version control is internal git.** conduit provisions bare
   repos and branches as local remotes; the workspace clones and
   pushes to them. Mirroring to external forges — and syncing work
   items to external trackers — are later integrations built on
   existing tooling, behind deliberately tight touchpoints, and
   never block the loop. This is what forge-neutral now means.
4. **Humans gate intent, not diffs.** The human seats are goal
   confirmation at the upper altitudes and test-set sign-off at the
   lower ones (is this the right set? what's missing?). Sign-off
   precedes implementation — an item cannot start until its
   verification is signed off — and signed-off content is locked:
   changing it bounces the item back through the gate.
5. **Approval is door-only and provable.** Approval metadata in a
   schema's frontmatter is a suite convention, not a conduit
   special: its presence triggers the sign-off process in code.
   Only the owning tool's doors write approval fields, and approved
   content is hash-pinned — any change to an approved page breaks
   the hash and forces re-approval. The harness can neither write
   nor grant approval, by construction, and that is checkable.
6. **Merging is mechanical and door-enforced.** Nothing merges
   except through the door that proves the gate: the signed-off
   test sets green plus the standing quality gates. Qualities the
   tests don't measure — security, dependency hygiene, conformance —
   attach as standing project-level goals: CI floors, and
   specialized out-of-band reviewer agents exposed to the harness
   as tools (the rig crate makes these cheap to build). Human
   eyeballs on any diff remain available at the seat — an operator
   habit, never a designed-in dependency.
7. **The forge adapters die** — GitHub/GitLab dry-run and the live
   Gitea path are deleted with the motion they served, along with
   the adroit subprocess seam (#112's standing half).

### Positive Consequences

- The two M3 blockers dissolve rather than get fixed: #48 closes
  obsolete (no forge in the MVP loop, so nothing to retool), and
  #112 reduces to deleting dead code.
- One store for the whole loop: assessments, decisions, and now work
  items share the substrate's gates, graph, search, and git history —
  provenance from decision to task to merged commit is graph edges.
- The largest and least-trusted code surface in the suite (forge
  adapters, subprocess seams, the demo's Docker path) is deleted,
  not maintained.
- The gate becomes executable and standing: a signed-off test set
  is a contract enforced on every future change, where a diff
  approval was a one-time opinion whose quality decayed with
  reviewer fatigue. And the approval mechanism is provable — the
  hash-pinned, door-only frontmatter makes "the harness never
  approves its own work" a property you can check, not a policy
  you hope holds.

### Negative Consequences

- The packaged engagement demo is orphaned until rebuilt on the new
  surface (accepted: it runs as-is from the old tree in the
  meantime, and #1's port question waits for the rebuild).
- Code quality beyond the signed-off tests rides on standing gates,
  not human review (accepted: CI floors and reviewer-agent tools
  carry it; any diff stays inspectable at the seat).
- The sign-off seat can inherit the rubber-stamp disease one level
  up if the altitude is wrong (accepted, with the mitigation
  designed in: humans contract at story-level behavior and the
  shape of test sets — coverage, deliberate gaps, riskiest miss —
  presented as intent, never as walls of test source).
- conduit's existing adapter investment is written off (accepted:
  the same call adroit's standalone era took, for the same reason).
- Teams wanting native forge/tracker integration wait for the
  mirroring touchpoints (accepted: replication is deferred by
  design, not foreclosed).

## Implementation

The rebuild issue (taps #113) carries the work at the walk's
frontier, per the walk protocol: delete the dead adroit surface (#112's standing
half), design the work-item schemas with their goal/sign-off
lifecycle and the hash-pinned approval mechanics, build conduit's
doors (one clap definition — terminal and MCP) including the
mechanical merge door, ship the kit skills for the PM and execution
postures, walk Step 5 on the new shape, and rewrite the guide's
Adopt and Measure pages from that walk. #48 closes as obsoleted by
this decision; #112 rescopes to its deletion half; #1 stays parked
pending the demo's rebuild. ADR-0014's conduit bullet (the forge
retool at M3) is mooted — the suite remains all-in on incus
everywhere a container is actually needed.

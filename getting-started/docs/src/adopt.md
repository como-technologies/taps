# Step 5 — Adopt

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

Accepted decisions become working code as reviewable pull requests on a
forge, with a human holding every gate. The engine is
[conduit](../portfolio/loop/adopt.html): it reads accepted ADRs through adroit's
machine-readable seam, files issues, drives a coding engine, and opens
PRs — it never merges. You do.

## Where this stands today (honestly)

conduit runs **live against a local Gitea forge**. Its GitHub and GitLab
adapters exist but their mutations are **dry-run by design** — conduit
shows you exactly what it *would* do on your real forge, and does it for
real only on the throwaway one. This tutorial walks the real path: a
local Gitea, your sample project pushed to it, real issues and real PRs
you review and merge.

## Stand up the forge

```sh
cd ~/taps/conduit
demo/kit/preflight        # verifies Docker (+ pulls the local model if ollama is up)
just forge-up             # throwaway Gitea on localhost:3000
```

> Gitea and amaker's authoring UI both default to port `3000` — stop
> amaker first (Ctrl-C from Step 3), or start the forge on another port
> with `FORGE_PORT`.

## Point conduit at your project and your decisions

```sh
just init-adroit          # builds the in-tree adroit where conduit expects it
```

Push your project to the local forge, then configure conduit with the
repo and your KB so it can read the accepted decisions from Step 4.

> 🚧 **Unverified — the biggest open seam in this guide.** Two things
> are moving here. First: today the polished path is the packaged
> engagement demo (`demo/kit/demo-up` … `beat-4-adopt`), which runs
> against a fictional client corpus; the walk will establish the
> first-party path for *your own* project and wiki — what
> configuration conduit needs, which engine drives the changes, and
> where the human gates sit. Second: the throwaway forge currently runs
> in Docker, and the suite has decided to retire Docker for incus
> (portfolio ADR-0014,
> [issue 48](https://github.com/como-technologies/taps/issues/48)) —
> the commands on this page change with it. The page gets rewritten
> from that walk; until then, the
> [conduit book's demo walkthrough](../conduit/)
> shows the full Adopt motion end to end.

## Review and merge — the human gates

conduit opens a draft PR per decision task; nothing proceeds without you.
Review the diff the way you review your team's work, request changes,
and merge when it's right. The merged PR is the artifact Step 6 prices.

## Tear down

```sh
just forge-down           # destroys the throwaway forge; leaves nothing
```

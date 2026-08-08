# Step 4 — Prescribe

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

An assessment that stays a scorecard changes nothing. This step turns it
into a backlog of **proposed decisions** in your knowledge base —
Architecture Decision Records your team refines, debates, and accepts.
The tool is [adroit](../portfolio/loop/prescribe.html).

## Seed the backlog from the assessment

Step 3 left the assessment and its report as typed pages in your space;
this step's decisions start from what those pages say.

> 🚧 **Unverified — and expected to change.** This page still describes
> adroit's pre-appliance, filesystem-bound shape (`--dir` at a local
> path). When the walk reaches this step, adroit becomes a transport
> client of the appliance like every other tool, consuming the
> assessment pages directly, and this page gets rewritten from the
> walk. Read the commands below as the old shape, not a promise.

## Refine the decisions

Work the backlog however you prefer — all three surfaces sit on the same
store:

```sh
adroit --dir ~/myproject-kb                # the TUI: browse, triage, edit
adroit show 1 --dir ~/myproject-kb        # or the CLI
adroit lint 1 --dir ~/myproject-kb        # authoring-quality gate, no AI needed
```

From your harness (Step 2's kit), decisions route through adroit's
guarded MCP surface — ask "what have we decided about testing?", draft a
revision in conversation, and every write still lands behind the same
gates.

## Accept what you mean to do

```sh
adroit set-status 1 accepted --dir ~/myproject-kb
adroit check --dir ~/myproject-kb          # the corpus stays sound
```

Accepted decisions are the contract with the next stage: Step 5 reads
**only** accepted ADRs. Everything still `proposed` waits, visible but
inert.

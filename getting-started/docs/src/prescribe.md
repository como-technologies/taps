# Step 5 — Prescribe

> 🚧 **Not yet walked.** This page hasn't survived a clean walkthrough
> yet — commands and claims may change as the dogfood walk reaches it.

An assessment that stays a scorecard changes nothing. This step turns it
into a backlog of **proposed decisions** in your knowledge base —
Architecture Decision Records your team refines, debates, and accepts.
The tool is [adroit](../portfolio/loop/prescribe.html).

## Seed the backlog from the assessment

Point adroit at Step 4's export:

```sh
adroit import path/to/assessment-export.yaml --dir ~/myproject-kb
adroit list --dir ~/myproject-kb
```

`import` seeds one proposed ADR per prescription and reports exactly what
it seeded and what it skipped (`-o json` for the machine view). With an
AI provider configured, `--ai` drafts fuller bodies — every draft passes
through a mechanical sanitizer before it touches a page.

> 🚧 **Unverified.** The import path expects the assessments exporter's
> schema (the pinned cross-product contract). The walk confirms the
> export from Step 4 imports without hand-editing.

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

Accepted decisions are the contract with the next stage: Step 6 reads
**only** accepted ADRs. Everything still `proposed` waits, visible but
inert.

---
name: walk-the-guide
description: Use when the user asks to walk, resume, or continue the Getting Started guide (/walk-the-guide). Drives the guide's steps for them — clean room up, commands run, prompts executed in the real workspace — bringing the human in at the human seats. State lives in the walk container's snapshot list.
user-invocable: true
---

# Walk the Getting Started guide with the user

Drive the guide at `getting-started/docs/src/` (SUMMARY.md is the
order), starting from the overview and Step 0 on a fresh machine. **The
pages are the program**: read the current text of each step and execute
exactly what it says, where it says (host vs. container — the page is
explicit). Never improvise around a page that doesn't work — a
divergence between the page's claim and reality is a finding to report,
and whether to work around it or file it upstream is the user's call.
Names (container, incus project, paths) come from the pages, not from
this skill.

Walks spend real tokens and real time. Work **one step at a time**:
say what the step will do, get a go, drive it, show the evidence.

## Finding where the walk stands

The **snapshot list is the authority** — no other state exists:

```sh
incus project switch <project-from-step-0>   # Step 0 names it
incus snapshot list <container-from-step-0>
```

The highest `step-N-done` snapshot is the last step the user signed off;
the walk continues at step N+1. No container or no snapshots → propose
starting at the overview and Step 0. Report position and the proposed
next move before driving anything.

## Driving a step

- Run the page's blocks in order, from wherever the page runs them.
- Where a page says *"paste this into your workspace session"*, run it
  as a real workspace session, headless:

  ```sh
  incus exec <container> -- su - <user> -c 'cd <workspace> && claude -p "<the page's prompt, verbatim>"'
  ```

  That loads the workspace's `.mcp.json`, CLAUDE.md, skills, and
  settings — the exact session a reader gets. Headless sessions cannot
  answer permission prompts: a stall marks exactly where a reader would
  be prompted. Surface it and let the user run that part interactively;
  don't loosen permissions to push past it.
- Run the page's own **Verify** section and show the output. A step
  isn't done until its page says it is.

## Human seats — hand over and wait

Some seats are the user's, always: harness sign-in (headless login
prints a URL to open elsewhere), API keys and `.env` values, anything
in a browser (the respond app, `walk.local` checks), and any approval a
workspace session stalls on. Hand over with exact instructions ("open
http://…, answer the assessment, tell me when done") and wait.

## Checkpoints — sign-off, then snapshot

When a step's verify passes, ask the user to confirm the step is
complete. **Only on their agreement**:

```sh
incus snapshot create <container> step-N-done
```

A checkpoint is a sign-off, not an autosave. To redo a step, restore
the previous checkpoint first (`incus snapshot restore`). When a change
invalidates already-completed steps (the guide or the products moved
under them), say which snapshots that invalidates and why, delete them
back to the last still-good step, and continue from there — the list
must always tell the truth.

## Never

- Edit the guide to match reality unless the user asks — report first.
- Touch a space's filesystem directly; spaces are reached through their
  appliance's transport, like every page says.
- Batch multiple steps without a go between them.

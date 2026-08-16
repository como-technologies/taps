# The conduit execution kit

Everything needed to point an AI harness at conduit's doors and run the
Adopt loop: plan work from accepted decisions (the PM posture) and
execute signed tasks test-first (the execution posture). The decision
behind the model is portfolio ADR-0015; the build record is taps
issue 113.

The human seats are not in this kit, on purpose: **sign-off and project
close happen at the terminal** (`conduit signoff`, `conduit close`),
where the operator sits. Through the harness's MCP door, `signoff` does
not exist at all, and the lifecycle refuses a harness closing a project.
The kit's sessions prepare, present, execute, and knock — they never
approve.

## The posture: one room, two hats

A conduit workspace is a working directory holding harness config, the
conduit store (`.conduit/` — internal bare repos live under it), and
scratch clones. It wires two MCP servers:

- **`kb`** — the llm-wiki appliance. Work items, decisions, and every
  other page live behind it; the session reads the landscape through
  `wiki_*` tools.
- **`conduit`** — `conduit mcp`, the work-item doors: `new`, `list`,
  `show`, `bounce`, `claim`, `complete`, `close`, `cancel`.

One session wears one hat at a time: the **PM posture** (skill
`plan-work`) turns accepted decisions into a draft work-item tree and
presents goals for human sign-off; the **execution posture** (skill
`execute-task`) takes a signed task from claim to the merge door; the
**review posture** (skill `review-task`) is the standing gate for
qualities the signed tests don't measure — run it in a *separate*
session from the one that implemented, because a session reviewing its
own diff is a mirror, not a gate. (That separation is the future
out-of-band reviewer agent's whole design; the skill is its job
description, run interactively until the walk proves the steps.)

## Layout

```
kit/
  README.md            this file
  workspace/           the execution workspace template — copy it whole
    .mcp.json          kb appliance + conduit door wiring
    CLAUDE.md          the session rules — doors are the only door
    .claude/
      settings.json    pre-authorized lanes + deny rules
  skills/              Como skills (Claude Code format; the procedures)
    plan-work/         the PM posture
    execute-task/      the execution posture
    review-task/       the standing-gate review posture (a separate session)
```

## Setup — Claude Code

1. Have a KB appliance running with a wiki (the llm-wiki kit walks
   this), and the suite pair discoverable: `KB_URL` (+ optional
   `KB_WIKI`) in the environment, a `.env` in the workspace, or
   `~/.config/taps/env`. `conduit mcp` finds the KB the same way every
   taps tool does.
2. Build conduit and put it on `PATH` (`cargo install --path conduit`
   from the taps checkout, or point `.mcp.json` at the binary).
3. Make a workspace: copy `kit/workspace/` (the whole directory,
   dotfiles included) to a fresh directory, and `kit/skills/` to
   `<workspace>/.claude/skills/`. Edit `.mcp.json` if your appliance
   isn't the incus-container default.
4. Open Claude Code in the workspace and talk. The internal bare repos
   provision themselves under `.conduit/repos/` on the first claim.

## The human seats (terminal, same workspace)

```sh
conduit list                    # the tree, with seal states
conduit show <item>             # read what you are about to sign
conduit signoff <item>          # seal the contract; item goes ready
conduit close <project>         # confirm the outcome; needs every child terminal
conduit bounce <item>           # reopen a signed contract for revision
```

Sign-off flows downhill (project before story before task); done flows
uphill (the merge door lands tasks, stories close over terminal
children, you close the project). A signed body is hash-pinned: any
edit breaks the seal and every door will bounce the item back to draft
rather than act on it.

# This directory is a conduit execution workspace

You are in the Adopt room: work items live in the knowledge base behind
the `kb` appliance, and conduit's doors (the `conduit` MCP server) are
the only way work-item state ever changes. The internal git repos live
under `.conduit/repos/` here; task clones are scratch directories you
create and remove.

## The rules that bind every session here

- **Work items move through conduit's doors only.** `project`, `story`,
  and `task` pages are conduit-owned classes (`x-owner: conduit`):
  never author or edit them with the `wiki_*` tools or the filesystem —
  a harness writing one isn't authoring, it's forging. Read them freely
  (`show`, `list`, or any wiki read tool); change them only through
  `new`, `bounce`, `claim`, `complete`, `close`, `cancel`.
- **The body is the contract and it is sealed.** A signed item's body
  is hash-pinned; any edit breaks the seal, and every door bounces a
  broken item back to draft instead of acting on it. If a signed
  contract is wrong, `bounce` it and say why — never edit around it,
  never implement around it.
- **You cannot approve, and that is the design.** Sign-off and project
  close are human seats at the terminal; `signoff` does not exist on
  your door. Prepare drafts, present goals at the right altitude, and
  hand the human a list of what awaits their signature. Never present
  walls of test source for approval — present intent: shape, coverage,
  deliberate gaps.
- **The merge door decides, not you.** Work lands only through
  `complete`: seal intact, the project's gate green, one squash commit.
  Never push to `main`, never merge by hand, never weaken a test to get
  green. If the door refuses, fix the work and knock again; if the
  *contract* is what's wrong, bounce it.
- **Test-first is the execution order**: write the signed test set as
  failing tests, then implement until the gate is green. The tests
  realize the contract the human signed — deviating from it silently is
  the one unforgivable move.
- **Keep the KB clean.** Every door call reports the admission gate's
  result — surface failures, don't swallow them. A tree inconsistency
  (dangling parent, orphaned item) is a finding to report, not repair
  by improvisation.
- **Answer from the KB**: decisions, work items, and their history are
  pages — search and cite them. "The wiki doesn't record this" is a
  valid answer; silent invention is not.

## Skills

The skills in `.claude/skills/` carry the procedures: `plan-work` (the
PM posture: accepted decisions → a draft work-item tree → goals
presented for sign-off), `execute-task` (the execution posture: a
signed task from claim to the merge door), and `review-task` (the
standing-gate review — only ever in a session that did not implement
the work). Prefer them over ad-hoc flows, and wear one hat at a time:
in particular, a session that executed a task never reviews it.

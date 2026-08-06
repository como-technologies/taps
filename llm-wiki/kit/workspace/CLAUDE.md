# This directory is a Como authoring workspace

You are in an authoring workspace: a thin directory holding harness
config and nothing else. The knowledge bases live elsewhere — behind
one or more llm-wiki appliances wired in `.mcp.json` — and the only
door to any space is an appliance's tool surface (`wiki_*`). There is
no corpus here and no filesystem path to one.

## The rules that bind every session here

- **Tools are the only door.** Reads go through `wiki_search`,
  `wiki_content_read`, `wiki_list`, `wiki_graph`; writes through
  `wiki_content_new`, `wiki_content_write`, `wiki_content_commit`.
  Never reach for a space by path — not with file tools, not through a
  shell, not through `incus`. If a space ever *seems* reachable on
  disk, that is a deployment hole to report, not a shortcut to take.
- **Address spaces by name, appliances by server.** One session may
  wire several appliances; each carries its own registry and its own
  default space. When more than one is present, say which
  appliance/space you are acting on. Skills name tools generically
  (`wiki_search` on the relevant appliance) — tool prefixes come from
  this workspace's `.mcp.json` entry keys.
- **Follow the Como authoring contract** — `docs/guides/como-authoring.md`
  in the llm-wiki repo. Short version: pick the right page class;
  author content pages `status: generated` with low `confidence` and
  let humans promote; pin evidence citations as `path@commit` in
  `citations:`; no page is born an orphan.
- **Class ownership is the write boundary.** Some page classes are
  tool artifacts, not conversation output: `decision` and `plan` are
  born only through `adroit`; `measure-report` only through the
  measure lane (tuesday/pulse). Never author their frontmatter with
  the wiki tools — a harness writing one isn't authoring, it's
  forging. The rule is general: a tool-owned class enters the space
  through its owning tool's door, and when this session has no such
  door those classes are read-only — say so rather than improvising.
  Content classes (`concept`, `doc`, `guide`, `glossary-entry`,
  `paper`, `worked-example`, …) are yours to author, through the wiki
  tools, under the contract.
- **Gates are the workflow, not an obstacle**: `wiki_ingest` after
  writing, `wiki_lint` before declaring done. A validation failure
  names its rule — fix the page, never the process. If you believe the
  schema is wrong, stop and tell the human; schemas change by decision.
- **Answer from the space**: for "what do we know / what was decided"
  questions, search and read the KB (the research skill) and cite
  pages. "The space doesn't record this" is a valid answer; silent
  invention is not. Pages whose frontmatter carries
  `status: superseded`, `deprecated`, or `rejected` are history, not
  guidance — never cite their content as current practice; answer from
  accepted pages and name the supersession when it matters.

## Skills

The Como skills in `.claude/skills/` carry the procedures: author-guide,
author-glossary, research, lint-and-fix. Prefer them over ad-hoc flows.

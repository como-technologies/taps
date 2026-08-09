# adroit

Decision records for the taps loop — the Prescribe tool, rebuilt
greenfield per [taps #93](https://github.com/como-technologies/taps/issues/93)
(the decision record and the plan).

The shape of the world, in five rules:

- The KB is the loop's state store; adroit is a thin set of verbs and
  gates over it, reached by `KB_URL`/`KB_WIKI` like every taps tool
  (`just kb-dev` serves a local one), resolved by the suite's one
  discovery order — process env, cwd `.env`, `~/.config/taps/env`
  (`como-kb-client`). Config is that pair plus the naming default
  (`ADROIT_NAMING` / `--naming`). Nothing else.
- adroit is the **only writer** of `decision` and `plan` pages
  (`schemas/`, registered on first contact, idempotently). Ownership is
  a **write** boundary — anyone reads decision pages through the
  engine's tools with no adroit dependency.
- Prescribing is judgment: agents (via `adroit mcp`) or humans (via the
  terminal) decide what's worth deciding; adroit never parses another
  tool's shapes.
- Provenance is graph edges (`new --relates <page>`), enforced by the
  engine's `broken-link` lint — not import metadata.
- One clap definition serves terminal, automation (`-o json`), and MCP
  (`src/surface.rs` is the whole surface; `mcp.rs` wraps it verbatim).

The surface:

| Verb | Does |
|---|---|
| `new` | create a proposed decision (`--relates` provenance; title, body from the caller's judgment) |
| `list` / `show` | corpus and single-decision reads, reference-resolved |
| `lint` | authoring-quality gate on one decision's body (mechanical, CI-safe) |
| `edit` | replace a proposed decision's body — the refine seat (decided records are superseded, not edited) |
| `set-status` | lifecycle in place: `proposed`→`accepted`/`rejected`; `accepted`→`deprecated` |
| `set-review` | set/clear a review-by date |
| `supersede` | link new over old, both sides' frontmatter, statuses consistent |
| `plan` | stored-plan contract: `--save` splices the marked `## Implementation` section; reads are provider-free |
| `check` | corpus invariants: supersession integrity, duplicate references/ids/titles |
| `mcp` | the same surface over stdio |

The frontmatter round-trip is sacred: keys adroit does not own ride a
flattened extra mapping and survive every rewrite untouched.

The standalone-era product (TUI, forge integrations, publish adapters,
web dashboard) lives on at
[como-technologies/adroit](https://github.com/como-technologies/adroit).

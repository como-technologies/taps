# adroit

Decision records for the taps loop — being rebuilt greenfield as the
loop's Prescribe tool ([taps #93](https://github.com/como-technologies/taps/issues/93)
is the decision record and the plan).

The shape of the new world, in five rules:

- The KB is the loop's state store; adroit is a thin set of verbs and
  gates over it, reached by `KB_URL`/`KB_WIKI` like every taps tool.
- adroit is the **only writer** of `decision` and `plan` pages
  (`schemas/`, registered on first contact). Ownership is a **write**
  boundary — anyone reads decision pages through the engine's tools.
- Prescribing is judgment: agents (via `adroit mcp`) or humans (via the
  terminal) decide what's worth deciding; adroit never parses another
  tool's shapes.
- Provenance is graph edges (`--relates`), enforced by the engine's
  lint — not import metadata.
- One clap definition serves terminal, automation (`-o json`), and MCP.

The standalone-era product (TUI, forge integrations, publish adapters,
web dashboard) lives on at
[como-technologies/adroit](https://github.com/como-technologies/adroit).

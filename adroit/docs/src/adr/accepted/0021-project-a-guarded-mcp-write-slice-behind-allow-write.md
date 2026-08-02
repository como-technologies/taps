# ADR-0021: Project a guarded MCP write slice behind --allow-write

> State: Accepted

## Status

Accepted
Created: 2026-07-28

## Stakeholders

- Maintainer — the MCP surface and its conformance tests
- Portfolio (ADR-0010, portfolio#7 wave 2) — MCP-only harnesses are the
  declared consumer
- The kit's Claude Desktop users — the first client with its own
  tool-call confirmation UX

## Context and Problem Statement

An earlier decision retired MCP write-verb exposure and named its own reopen
criterion: *"an MCP **consumer** (not a hypothetical) that needs write
verbs over the protocol and brings its own confirmation UX — at which
point the exposure is the documented additive change: an explicit opt-in
flag, `destructiveHint` annotations, and conformance tests proving the
default remains read-only."*

Portfolio ADR-0010 (harness-first: the user's AI harness is the primary
human UI, over MCP) delivers that consumer. Claude Desktop and other
MCP-only harnesses cannot shell the CLI; without a write path over the
protocol they cannot author decisions at all — exactly the negative
consequence that decision accepted, now no longer acceptable.

## Decision Drivers

- The retirement's reopen criterion is met literally, including the
  confirmation UX (MCP clients gate destructive tool calls on the human).
- The exposure was designed to be additive: the server projects the
  manifest's classifications mechanically, so the change is a filter +
  its own conformance tests, not a redesign.
- The read-only default is load-bearing (portfolio security posture) and
  must survive byte-identically.
- Interactive surfaces (the editor, `draft`'s stdin interview) cannot
  exist over the protocol and must not pretend to.

## Considered Options

1. **A guarded, owned write slice behind `adroit mcp --allow-write`.**
2. Project every `writes` verb mechanically behind the flag — no slice.
3. Keep the write-verb retirement as is; MCP-only harnesses hand decisions to a human or
   a shell-capable session indefinitely.

## Decision Outcome

Chosen: **option 1 — the guarded slice**, per the retirement's own blueprint.
Option 2 fails on its face: `edit` and `draft` are interactive, `import`
/ `seed` are bulk bootstraps, and the maintenance writers (`relink`,
`renumber`, `index`, `publish`, `config`) have process-level semantics an
MCP call shouldn't carry. Option 3 leaves the harness-first decision
unimplementable on MCP-only clients.

Concretely:

- **The slice is an owned table** (`manifest::mcp_write_slice`), beside
  `classified()` and `escalation()`: `new` (forced `--no-edit`;
  `--force` / `--interview` denied), `compose` (the instruction-driven
  body-write path, forced `--no-edit`), `set-status`, and `plan`
  re-admitting `--save` / `--dry-run` (never `--force` /
  `--regenerate`). `draft` is deliberately absent — its stdin interview
  has no protocol representation.
- **Forge and file-output escalations stay stripped categorically in
  both modes.** Only declared `"writes"` escalations named by a slice
  entry's `admit` list survive. The write verbs' forge control surface
  (`new` / `set-status` / `supersede` / `set-review` × `--forge` /
  `--yes`, `set-status --quorum`) is now escalation-classified so the
  strip is mechanical — the conformance test caught the `new --forge`
  leak before this classification existed, which is the system working.
- **Write tools announce themselves**: `readOnlyHint: false`,
  `destructiveHint: true` — the client confirmation signal the reopen
  criterion required. Acceptance remains a human act: a transition
  happens because a human instructed it and approved the tool call.
- **The gates don't move**: an MCP write lands behind the space's
  admission hooks and `adroit check`, exactly like a CLI write. The MCP
  layer adds no bypass and no new write primitives — every `tools/call`
  re-runs the classified CLI verb as a subprocess.
- **The default is untouched**: `adroit mcp` without the flag projects
  the identical read-only surface, pinned by
  `default_mode_is_byte_identical_to_pre_slice_projection` and the
  pre-existing conformance tests, unchanged.

This executes the write-verb retirement's reopen clause; its analysis of
why read-only-by-default matters is carried forward, not overturned.

### Positive Consequences

- MCP-only harnesses author and transition decisions end to end —
  portfolio#7 wave 2's acceptance — with the human approving each
  destructive call.
- The security audit stays one sentence long, now with a qualifier:
  "read-only unless started with `--allow-write`, and never forge or
  filesystem either way."
- The escalation table grew truer: write verbs' forge flags are
  classified, benefiting every downstream allowlist, not just MCP.

### Negative Consequences

- "Cannot write" is no longer categorical across all invocations — an
  auditor must check the server's argv (mitigated: the flag is
  escalation-classified in the manifest, and the default is pinned).
- `compose` over MCP still requires adroit's own AI provider at runtime
  (`ai.enabled`) — an MCP-only client without a configured provider can
  scaffold and transition but not AI-revise bodies; said plainly in the
  kit docs.
- The write slice's argv mapping faces hostile input — mitigated by the
  `fuzz_mcp_request_allow_write` twin target.

## Implementation

`src/manifest.rs`: the `mcp_write_slice` table + the write-verb forge
escalations + `("mcp", "allow_write") => "writes"`. `src/mcp/tools.rs`:
`Server::allow_write`, forced argv on `tools/call`, destructive
annotations. `src/cli.rs` / `src/main.rs`: the flag. Conformance +
e2e + fuzz tests as named in CLAUDE.md; docs synced (automation, CLI
reference, testing, roadmap, CLAUDE.md) in the same change. The llm-wiki
kit's Claude Desktop page and the portfolio's Prescribe chapter update in
their own repos, same wave (portfolio#7).

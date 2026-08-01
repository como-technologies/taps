# Facilitated SME Walkthrough

This page is the quickstart for a **facilitated SME session**: an external
subject-matter expert co-authors an assessment with a Como facilitator
alongside. It covers both lanes — the interactive web flow and the headless
CLI — and every command on it was run live against a local ollama
(llama3.2 3B, 2026-06-12) before being written down.

**The operating model first** (ADR-0009): the app has no in-app login by
design at this rung. The facilitator runs it, it binds `127.0.0.1` by
default, and a remote SME reaches it through a channel the facilitator
controls — see [Remote SMEs](#remote-smes-the-tunnel) below. Do not rebind
`HOST` to a public interface.

## Prerequisites

- The `amaker` binary (`cargo build --release`, or run via
  `cargo run --`).
- An AI provider:
  - **Fully local** — an [ollama](https://ollama.com) server with a model
    pulled (`ollama pull llama3.2`). No API key, no network beyond
    localhost.
  - **Hosted** — `AI_PROVIDER=anthropic` with an `ANTHROPIC_API_KEY`.
    Recommended for the interactive web lane: tool-calling on a 3B local
    model is a documented degraded mode (the workspace says so in a
    banner).

## Start the app

```bash
AI_PROVIDER=ollama DATA_DIR=./data PORT=3000 cargo run -p amaker-author
```

Open `http://127.0.0.1:3000`. On the ollama provider the workspace shows
an amber **local model** banner — that is the honest signal that the
interactive chat's tool calls are best-effort on small local models, while
the actual generation steps (structure, questions) run tool-free with the
same mechanical quality gates as the headless pipeline.

## The facilitated web session

The session below is the rehearsed shape; the SME drives, the facilitator
narrates and unblocks.

1. **Create a project.** Home page → project name and a one-line
   description (e.g. "Release Readiness Pilot").

2. **Upload raw material.** In the workspace, *Upload* takes the SME's
   existing documents. Text formats (`.md`, `.txt`, `.json`, `.yaml`,
   plain text) are extracted at upload time and reach **every** chat and
   generation prompt as background signal about the organization — framed
   with an explicit never-cite instruction, and mechanically gated: an
   assessment that cites an uploaded file's name or its JSON keys is never
   saved (see [Authoring Workflow](./workflow.md#documents)). Binary
   formats (PDF/DOCX) are stored but not read at this rung — paste the
   relevant text instead.

3. **Scoping chat.** The SME describes what to assess, for whom, and why
   — e.g. *"We want to assess our release process maturity: how code
   ships, how releases are verified, and how incidents feed back."* The
   model asks clarifying questions as clickable option cards; answering
   them is the fastest way to a sharp scope.

4. **Generate the structure.** Advance the phase stepper to *Building the
   structure* and ask: *"Please generate the assessment structure now."*
   The model calls the structure generator (tool-free, temperature 0,
   schema-validated, degeneracy/leakage-gated). A failed generation is fed
   back to the model as corrective feedback rather than an error page; on
   a small local model it may take a nudge or two — say *"try again"*, or
   constrain the scope ("about 3 domains with 2 practices each"). Success
   lands the structure in the preview panel and advances the project to
   *Adding questions*.

5. **Questions, practice by practice.** Ask for questions one practice at
   a time (*"Generate questions for Automated Testing and Validation"*),
   review them in the preview with the SME, then move to the next
   practice. Placeholder questions and context leakage are gated here
   too — never saved.

6. **Refine.** In *Making it better*, the SME talks through edits against
   the live preview.

7. **Export.** The *Export YAML* button (or
   `GET /api/projects/<id>/export?format=yaml`) downloads the assessment.
   It is byte-identical to the CLI export of the same project:

   ```bash
   DATA_DIR=./data amaker export <project-id> --format yaml --out assessment.yaml
   amaker validate assessment.yaml
   ```

   `validate` re-applies the schema **and** the degeneracy gate, so the
   artifact that leaves the session is provably non-placeholder.

In the live rehearsal of exactly this session (llama3.2, fully local),
the model asked two clarifying questions during scoping, generated a
3-domain / 6-practice "Release Process Maturity Assessment" citing none
of the uploaded material, produced 10 questions for the first practice,
and the export validated:

```text
valid: 'Release Process Maturity Assessment' — 3 domains, 6 practices, 10 questions
```

## The headless lane

When the SME's input already exists as a written brief — or the session
is local-first end to end — skip the chat and author directly
(see [Headless Authoring](./authoring.md) for the pipeline and gates):

```bash
AI_PROVIDER=ollama amaker author \
  --brief brief.md \
  --context team-notes.md \
  --jobs 2 \
  --out assessment.yaml
amaker validate assessment.yaml
```

`--context` files get the same background-signal framing and leakage gate
as web uploads. `--jobs 2` runs question generation on two concurrent
lanes — a real speedup only when the ollama server runs with
`OLLAMA_NUM_PARALLEL >= 2` (see
[Configuration](./configuration.md#parallel-authoring-and-ollama_num_parallel);
measured timings in [Dogfood](./dogfood.md#timing-serial-vs---jobs)).

From here the assessment feeds the Assess→Prescribe seam —
`adroit import --from-assessment assessment.yaml` seeds one proposed ADR
per practice; [Dogfood](./dogfood.md) walks that handoff.

## Remote SMEs: the tunnel

Keep the app on loopback and put access control in the channel
(ADR-0009). The simplest: the SME opens an SSH tunnel to the
facilitator's host and uses their own browser —

```bash
ssh -L 3000:127.0.0.1:3000 facilitator-host
# then open http://127.0.0.1:3000 locally
```

For hosted pilots, a TLS-terminating reverse proxy that performs the
authentication itself (basic auth/OIDC at the proxy, or VPN-only
exposure) fronts the same loopback app. In-app accounts are a self-serve
precondition, deliberately not built at this rung.

## What can go wrong (honestly)

- **The model narrates a tool call instead of making one** (you see JSON
  in the chat). That is the degraded mode the banner warns about — say
  *"use the generate_structure tool"* or just ask again; the structured
  generation itself is tool-free and reliable once triggered.
- **A generation fails its gates** (placeholder echoes, cited uploads,
  schema misses). The failure goes back to the model as feedback and
  nothing defective is saved; re-ask, optionally with tighter scope.
- **Draft quality tracks the model.** A 3B local model yields a solid
  first draft to refine with the SME; `AI_PROVIDER=anthropic` produces
  stronger drafts from the same session.

# ADR-0004: OllamaProvider talks to ollama's native /api/chat endpoint

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies engineering — the assessments maintainers own this decision;
the portfolio's local-first dogfood loop (running the Assess stage with no
hosted API key) depends on it.

## Context and Problem Statement

ADR-0003 put the AI layer behind the `LlmProvider` seam and left the
`AI_PROVIDER=ollama` arm failing with "not implemented". The local backend now
has to be real: one module implementing `chat(system, messages, tools,
max_tokens, model_override) -> ChatResponse` against a local Ollama server.
The open questions are which Ollama API surface to target (the native
`/api/chat` or the OpenAI-compatibility layer at `/v1/chat/completions`), how
to map our `ToolDef` JSON schemas and normalize Ollama's response shape into
`ChatResponse`, and what sampling to use given that the plain-YAML generation
paths must be reliable on a small (3B) local model.

## Decision Drivers

- The provider must normalize into the exact `ChatResponse` shape the chat
  loop and generation helpers already consume — no caller changes
- `ToolDef.input_schema` is already plain JSON Schema; conversion layers are
  where the anthropic path silently dropped tools, so avoid them
- The tool-free generation paths (`generate_structure`,
  `generate_questions_for_practice`) need deterministic, parseable YAML from
  a 3B model — sampling variance is the enemy there
- Older and newer Ollama versions differ (tool-call `id` is only returned by
  newer ones); normalization must tolerate both
- A live smoke test must prove the path end to end against the real server,
  without making CI depend on a running Ollama

## Considered Options

- Ollama's native `/api/chat`: non-streaming POST, system prompt as a
  `system`-role message, `tools[].function.parameters` taking JSON Schema
  directly, `options.num_predict`/`options.temperature` for budget/sampling
- Ollama's OpenAI-compatibility endpoint (`/v1/chat/completions`), reusing
  the OpenAI wire shape
- An ollama client crate (e.g. ollama-rs) instead of hand-rolled reqwest

## Decision Outcome

Chosen: **the native `/api/chat` endpoint, hand-rolled over reqwest**,
because it is Ollama's first-class surface: `ToolDef` JSON schemas pass
straight through as `tools[].function.parameters` (no conversion layer to
silently drop tools), tool-call arguments come back as structured JSON (the
OpenAI layer returns them as strings to re-parse), and native `options` give
direct control of `num_predict` and `temperature`. A client crate was
rejected for the same reason as in ADR-0003: the seam is one small POST and a
normalization function, and a dependency would just add another shape to map.

Normalization rules (ollama → `ChatResponse`):

- `message.content` → `text`
- `message.tool_calls[].function` → `tool_uses`; Ollama-provided ids are kept
  when present, otherwise `ollama_tool_<index>` is synthesized (older Ollama
  versions return no id)
- `stop_reason` is `"ToolUse"` whenever tool calls are present — Ollama
  reports `done_reason: "stop"` even for tool calls — otherwise
  `done_reason` maps `"stop"` → `"EndTurn"`, `"length"` → `"MaxTokens"`,
  anything else passes through verbatim
- `was_truncated` is true exactly when `done_reason` is `"length"`
- a response that does not match the expected shape is a clear provider
  error, never a panic or a silent empty response

Sampling: tool-free turns (the plain-YAML generation paths) pin
`temperature: 0` so a small model's output is deterministic and parseable;
tool-bearing interactive turns keep the model's default sampling.
Configuration is `OLLAMA_HOST` (default `http://localhost:11434`) and
`OLLAMA_MODEL` (default `llama3.2`), with no API key required — completing
the per-provider key gating from ADR-0003.

### Positive Consequences

- The app boots and authors fully locally: `AI_PROVIDER=ollama` needs no
  hosted API key and no network beyond localhost
- Direct JSON-schema passthrough means tools cannot be silently dropped by a
  conversion layer
- Temperature-0 generation makes the YAML paths reproducible — the live
  smoke test (`just smoke-ollama`, gated on `ASSESSMENTS_E2E_OLLAMA=1`)
  proves structure generation parses and validates against the real schema
- The normalization is pure and unit-tested against captured response shapes,
  independent of any running server

### Negative Consequences

- The native endpoint is Ollama-specific: pointing the provider at any other
  OpenAI-compatible server is not possible; that would be a new provider
- Tool-call reliability on small local models (llama3.2 3B) is markedly
  weaker than Claude's — the interactive chat loop degrades on ollama, and a
  3B model needs explicit prompting to keep required fields like
  `questions: []` in generated YAML (the headless authoring milestone adds
  bounded retries on top)
- We track Ollama's wire format ourselves; a breaking change in `/api/chat`
  surfaces as a normalization error rather than a client-library upgrade

## Implementation

Done in milestone M3: `OllamaProvider` in `src/services/ollama.rs` (request
building and response normalization as pure, unit-tested functions),
`AppError::OllamaApi` for provider errors, `OLLAMA_HOST`/`OLLAMA_MODEL` in
`src/config.rs`, the `AI_PROVIDER=ollama` arm of `build_provider()` replacing
the M2 "not implemented" error, and the env-gated live smoke test wired as
`just smoke-ollama`.

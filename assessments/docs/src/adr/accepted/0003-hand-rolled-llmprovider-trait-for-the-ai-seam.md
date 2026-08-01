# ADR-0003: Hand-rolled LlmProvider trait for the AI seam

> State: Accepted

## Status

Accepted

## Stakeholders

Como Technologies engineering — the assessments maintainers own this decision;
the portfolio's local-first dogfood loop (running the Assess stage on a local
model) depends on it.

## Context and Problem Statement

assessments was hardwired to Anthropic: the chat handler, the tool-execution
loop, and the generation helpers all called a concrete `ClaudeService`, and
`ANTHROPIC_API_KEY` was required just to boot. The roadmap needs a second,
local backend (Ollama) so the app can run without a hosted API key, which
means the AI surface has to sit behind an abstraction. The question is what
that abstraction is: a hand-rolled trait of our own, or an existing agent
framework such as rig (which the sibling adroit tool already uses for prose
completion).

## Decision Drivers

- assessments owns its server-side tool-execution loop (execute tool calls,
  make follow-up turns, filter interactive tools) and must keep owning it —
  identical loop behavior across backends is the point of the seam
- A second backend (Ollama, milestone M3) must be addable without touching
  handlers, the tool loop, or the generation prompts
- The app must boot with no Anthropic key when another provider is selected
- The unofficial anthropic-sdk-rust 0.1 client is immature; its blast radius
  should be contained in one module so it can be swapped later
- Tests need a scriptable fake at the seam so the chat loop and prompt
  assembly are testable without any network

## Considered Options

- A hand-rolled async trait: `chat(system, messages, tools, max_tokens,
  model_override) -> ChatResponse`, with `ChatResponse` normalizing text,
  tool calls, stop reason, and truncation
- rig's agent abstraction (as adroit uses for prose completion)
- Keep the concrete ClaudeService and add a parallel OllamaService with
  duplicated call sites

## Decision Outcome

Chosen: **a hand-rolled `LlmProvider` trait**, because assessments' server-side
tool-execution loop is the app's core behavior and must stay in our code; rig's
agent abstraction owns the loop itself and would hide exactly the part we need
to control. adroit's use of rig is a different shape — one-shot prose
completion with no tool loop. The trait mirrors the one chat surface the app
actually needs, every provider returns the same normalized `ChatResponse`, and
the loop on our side of the seam behaves identically for all backends. A
parallel second service was rejected because duplicated call sites drift.

Provider selection is by environment (`AI_PROVIDER=anthropic|ollama`, default
anthropic) with per-provider key gating: `ANTHROPIC_API_KEY` is required only
when the anthropic provider is selected, so the app boots fully locally once
another provider is chosen. Until the Ollama implementation lands (M3), the
ollama arm fails at provider construction with a clear "not implemented yet"
error.

### Positive Consequences

- The Ollama backend (M3) is one new module implementing one trait; handlers,
  tool loop, and prompts are untouched
- anthropic-sdk-rust specifics (tool-schema conversion, stop-reason mapping)
  are contained in `src/services/anthropic.rs` and can be swapped for a
  maintained client behind the same trait
- `FakeProvider` scripts responses and records calls, making the chat loop
  and prompt assembly unit-testable offline
- The app boots without `ANTHROPIC_API_KEY` when `AI_PROVIDER` selects a
  non-Anthropic provider

### Negative Consequences

- We maintain our own provider abstraction instead of inheriting one from a
  framework — every new backend is our code to write and normalize
- The trait is shaped by the Anthropic message model (system + role/content
  pairs + tools); future providers must map their native shapes onto it,
  which may lose provider-specific features (e.g. streaming) until the trait
  grows
- Behavior differences between backends (tool-call reliability on small local
  models) are not hidden by the seam and still need product-level handling

## Implementation

Done in milestone M2: `LlmProvider` + `ChatResponse` in
`src/services/provider.rs`, `AnthropicProvider` in `src/services/anthropic.rs`,
provider-agnostic prompt/generation helpers in `src/services/generation.rs`,
`AppState` holding `Arc<dyn LlmProvider>`, `build_provider()` selecting by
`AI_PROVIDER` with per-provider key validation in `src/config.rs`, and
`FakeProvider` for tests. Follow-up: implement `OllamaProvider` against
ollama's native `/api/chat` (milestone M3) and record its specifics in a
separate ADR.

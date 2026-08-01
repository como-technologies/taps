# Configuration

All configuration comes from environment variables (12-factor style). The
shared AI/storage config is loaded in `crates/amaker-core/src/config.rs`;
each web binary adds a small config of its own (`crates/amaker-*/src/config.rs`).
A `.env` file in the working directory is read at startup via dotenvy —
copy `.env.example` to `.env` to get started.

| Variable            | Required                       | Default                  | Description                                  |
| ------------------- | ------------------------------ | ------------------------ | -------------------------------------------- |
| `AI_PROVIDER`       | No                             | `anthropic`              | AI backend: `anthropic` or `ollama`          |
| `ANTHROPIC_API_KEY` | When `AI_PROVIDER=anthropic`   | —                        | Anthropic API key                            |
| `HOST`              | No                             | `127.0.0.1`              | Server bind host                             |
| `PORT`              | No                             | `3000`                   | Server bind port                             |
| `CLAUDE_MODEL`      | No                             | `claude-sonnet-4-5`      | Claude model alias (anthropic provider)      |
| `OLLAMA_HOST`       | No                             | `http://localhost:11434` | Ollama server URL (ollama provider)          |
| `OLLAMA_MODEL`      | No                             | `llama3.2`               | Ollama model name (ollama provider)          |
| `DATA_DIR`          | No                             | `./data`                 | Root directory for project storage           |
| `RUST_LOG`          | No                             | `info`                   | Log filter (CLI config; the web binaries read `*_LOG_LEVEL` — see Logging) |

## Operating model: local-first, no in-app auth (ADR-0009)

The app has **no in-app authentication by design** at this rung: it is
operated by a facilitator, for facilitated SME sessions. The shipped
default binds `HOST=127.0.0.1` — nothing is reachable off the operator's
machine unless `HOST` is explicitly rebound. **Rebinding to a public
interface exposes every project with no second factor; don't.**

For a remote SME (hosted pilot), keep the app on loopback and put the
access control in the channel instead:

- an SSH tunnel: `ssh -L 3000:127.0.0.1:3000 operator-host`, then the SME
  opens `http://127.0.0.1:3000` locally; or
- a TLS-terminating reverse proxy that performs the authentication itself
  (basic auth or OIDC at the proxy, or private-network/VPN exposure).

In-app authn/authz and multi-tenancy are recorded as **self-serve
preconditions** — the unattended-strangers rung supersedes ADR-0009 and
builds them; until then the operator is the access control.

## AI providers

The AI layer sits behind an `LlmProvider` seam
(`crates/amaker-core/src/services/provider.rs`, ADR-0003) and `AI_PROVIDER`
selects the backend at startup (case-insensitive):

- `anthropic` (default) — Claude via the Anthropic API. `ANTHROPIC_API_KEY`
  must be set, or startup fails with a configuration error.
- `ollama` — a local Ollama server, via its native `/api/chat` endpoint
  (`crates/amaker-core/src/services/ollama.rs`, ADR-0004). No API key
  needed; `OLLAMA_HOST`
  points at the server and `OLLAMA_MODEL` picks the model. The app boots and
  runs fully locally with `AI_PROVIDER=ollama`.

The API key rule is per-provider: a provider's credentials are required only
when that provider is selected. With `AI_PROVIDER=ollama` the app boots
without `ANTHROPIC_API_KEY` set.

On the ollama provider, tool-free requests (the plain-YAML generation paths:
structure and question generation) are pinned to temperature 0 for
deterministic output; tool-bearing interactive turns keep the model's default
sampling. Note that tool-call reliability on small local models (the default
`llama3.2` is 3B) is markedly weaker than Claude's — the interactive chat
loop works best on the anthropic provider.

`just smoke-ollama` runs an env-gated live smoke test against the local
server: it generates an assessment structure with the configured model and
asserts the YAML passes schema validation. Plain `just test`/`just ci` skip
it (it only runs with `ASSESSMENTS_E2E_OLLAMA=1`).

## Models

`CLAUDE_MODEL` sets the default model for the anthropic provider; the
workspace also has a per-project model picker
(`crates/amaker-core/src/models/model.rs`) offering:

- `claude-sonnet-4-5` (Sonnet 4.5) — the default
- `claude-haiku-4-5` (Haiku 4.5)
- `claude-opus-4-5` (Opus 4.5)

`OLLAMA_MODEL` sets the model for the ollama provider (default `llama3.2`;
any model your Ollama server has pulled works, e.g. `qwen2.5` or
`llama3.1:8b` for stronger output).

### Parallel authoring and `OLLAMA_NUM_PARALLEL`

`amaker author --jobs N` dispatches up to N per-practice question
generations concurrently (see [Headless Authoring](./authoring.md)). Two
server-side facts govern whether that helps:

- **Ollama serves as many requests in parallel as it has slots.** With
  the default `OLLAMA_NUM_PARALLEL=1` the extra lanes just queue — safe,
  but no faster. Run the server with `OLLAMA_NUM_PARALLEL >= N` to let
  the lanes actually overlap.
- **Every parallel slot multiplies the KV cache.** This app pins
  `num_ctx=8192` on every request (ADR-0004's silent-clipping fix), so a
  server with `OLLAMA_NUM_PARALLEL=2` allocates two 8192-token KV caches
  for the model. Budget memory accordingly before raising either knob.

On a CPU-bound host the decode throughput is shared across lanes, so even
with real server-side parallelism the wall-clock gain is sub-linear —
measure before assuming (the [Dogfood](./dogfood.md) page records a
measured comparison).

## Logging

Logging uses `tracing` (setup shared in
`crates/amaker-core/src/observability.rs`). Each web binary reads its own
filter variable — `AUTHOR_LOG_LEVEL`, `ASSESS_LOG_LEVEL`,
`ANALYZE_LOG_LEVEL` (default `info`) — in standard `RUST_LOG` filter
syntax, e.g. `debug`, or `amaker_author=debug` to scope to one crate.

Output goes to the console only (no log files). `LOG_FORMAT` picks the
formatter: `json` emits Google Cloud Logging structured lines (a real
`severity` field per line), `text` the human-readable formatter. Unset, it
defaults to JSON when `K_SERVICE` is present (Cloud Run injects it) and
text everywhere else.

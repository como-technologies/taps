# Amaker — AI-assisted assessment authoring

**Status:** Active development

> New to the TAPS suite? Start with the
> [Getting Started guide](https://como-technologies.github.io/taps/getting-started/).

Amaker helps domain Subject Matter Experts (SMEs) create structured assessments
through AI-assisted conversation, then collect responses against published
versions and analyze the results. A split-view authoring UI pairs chat with a
live preview of the assessment tree (domain → practice → question).

## Deployments

The live services run on Google Cloud Run, gated by Identity-Aware Proxy —
Google sign-in, restricted to the `comotechnologies.io` allowlist (see
[Authentication](docs/src/operations/auth.md)):

| Service | URL |
| ------- | --- |
| Authoring (`amaker-author`)  | <https://amaker-author-1027417513916.us-central1.run.app> |
| Responding (`amaker-assess`) | <https://amaker-assess-1027417513916.us-central1.run.app> |
| Analysis (`amaker-analyze`)  | <https://amaker-analyze-1027417513916.us-central1.run.app> |
| User manual (`amaker-docs`)  | <https://amaker-docs-1027417513916.us-central1.run.app> |

`amaker-author` is the entry point; `amaker-assess` and `amaker-analyze` are
reached via in-app links to a specific project.

## The system

The system is three binaries sharing a common core (all crates are members
of the taps workspace):

| Binary | Port | Role |
| ------ | ---- | ---- |
| `amaker-author`  | 3000 | SME + Claude build and refine the assessment. Requires `ANTHROPIC_API_KEY`. |
| `amaker-assess`  | 3001 | A respondent fills out a published version. No LLM. |
| `amaker-analyze` | 3002 | Scorecard, gaps, roadmap, narrative. No LLM at request time; narrative regen calls back into author. |

## Tech stack

| Layer     | Technology                                     |
| --------- | ---------------------------------------------- |
| Backend   | Rust + Axum                                    |
| Templates | Askama (compile-time)                          |
| Frontend  | HTMX + Tailwind CSS                            |
| LLM       | Anthropic Claude via [`rig-core`](https://github.com/0xPlaygrounds/rig) |
| Storage   | [`object_store`](https://docs.rs/object_store) — filesystem (default), S3-compatible (rustfs / R2 / B2 / …), GCS, or in-memory |

## Getting started

### Prerequisites

- Rust 1.95+ ([rustup](https://rustup.rs))
- [`just`](https://github.com/casey/just) for the dev recipes
- An Anthropic API key (only needed for `amaker-author`)

### Setup

```bash
git clone https://github.com/como-technologies/taps.git
cd taps/assessments
cp .env.example .env             # then put your key in ANTHROPIC_API_KEY
just run                         # builds Tailwind, starts all three binaries, opens the browser
```

`just run` brings up author + assess + analyze on ports 3000/3001/3002 with
the **filesystem backend** (each binary writes to `./data`), opens
`http://localhost:3000`, and prefixes each binary's logs (`[author] …`,
`[assess] …`, `[analyze] …`) in a single terminal. Ctrl-C stops all three.
For the S3-compatible backend, see *Running against an S3-compatible backend*
below.

If you'd rather start one at a time, the per-binary recipes still exist:

```bash
just run-author      # http://localhost:3000
just run-assess      # http://localhost:3001
just run-analyze     # http://localhost:3002
```

### Common recipes

```bash
just --list          # all recipes
just test            # cargo test
just check           # cargo check
just lint            # clippy
just book-serve      # mdbook — architecture docs on http://localhost:4000
```

## Workflow

Authoring → Responding → Analyzing — one binary per act:

1. **Authoring** — In `amaker-author`, the SME chats with Claude through four
   substates (Scoping → Structuring → Questions → Refining) to build the
   assessment. Every edit is a surgical tool call that preserves entity
   UUIDs. When ready, the SME (or the orchestrator) publishes a version.
2. **Responding** — A respondent opens `amaker-assess`, hits the project URL,
   and fills out the form. Their answers bind to the published version they
   were administered against.
3. **Analyzing** — `amaker-analyze` resolves responses against their bound
   version and renders scorecard, gaps, roadmap, and an LLM-generated
   narrative (regenerable on demand).

## Storage layout

Each project is a key prefix in the blob store at `projects/{project_id}/`:

```
projects/{ProjectId}/
├── project.json                # mutable; the project envelope
├── draft.yaml                  # mutable; the live working copy
├── versions/
│   ├── v1.yaml                 # immutable; published snapshot of draft
│   └── v1.meta.json            # immutable; { published_at, notes? }
├── responses/{respondent}.yaml # mutable; per-respondent answers + version binding
├── chat.json                   # mutable; conversation transcript
├── uploads/{doc_id}_{name}     # opaque uploaded blobs
└── analysis/                   # regenerable caches: report.md, scorecard.json, gaps.json
```

Versioning is immutable version blobs (no git tags); concurrency is ETag
conditional writes (no per-process mutexes); deployment is stateless
containers (no persistent local disk required). See
[docs/src/architecture/draft-publish.md](docs/src/architecture/draft-publish.md)
for the full design.

## Metamodel

```
Assessment
  └─ Domain    (3–7 per assessment)
      └─ Practice  (2–5 per domain)
          └─ Question  (3–12 per practice)
```

Each level carries the CVR triad (**C**ontext / **V**alue / **R**isk). Answers
include evidence, blockers, planned action, and free-text notes.

## Project structure

```
assessments/                    # a product in the taps workspace (crates are
│                               # members of the root Cargo.toml)
├── Dockerfile, Dockerfile.docs # container builds — the apps / the docs site
├── crates/
│   ├── amaker-core/            # shared: models, storage, services, analysis
│   ├── amaker-author/          # authoring binary  (+ its own templates/)
│   ├── amaker-assess/          # respondent binary (+ its own templates/)
│   └── amaker-analyze/         # results binary    (+ its own templates/)
├── templates-shared/           # Askama templates shared across the binaries
├── assets/                     # Tailwind input + vendored HTMX
├── deploy/                     # IAP allowlist, docs nginx config
├── docs/                       # mdBook user manual (vision, architecture, operations)
└── data/                       # local-dev storage (gitignored)
```

## Configuration

All configuration via environment variables (12-factor). See
[.env.example](.env.example) for the full list. The shape:

- `ANTHROPIC_API_KEY` — unprefixed (third-party provider); required by author.
- Per-binary settings under `AUTHOR_*` / `ASSESS_*` / `ANALYZE_*`.
- Storage backend selected via `<PREFIX>_STORAGE_BACKEND` (one of
  `filesystem` / `s3_compatible` / `gcs` / `in_memory`) and provider-specific
  vars (see `.env.example`).

## Running against an S3-compatible backend (rustfs, local dev)

The default backend is `filesystem`. To validate the S3-protocol code path
without paying any cloud provider, point the binaries at a local
[rustfs](https://github.com/rustfs/rustfs) server.

One-time install:

```bash
# rustfs server itself (cargo install, brew, or release binary — see project README)
# rc (rustfs's S3-compatible CLI client) — used by `just rustfs-init`:
brew install rustfs/tap/rc       # or: cargo install rustfs-cli
```

Daily workflow:

```bash
just rustfs-serve                # terminal 1: rustfs on http://localhost:9000
just rustfs-init                 # terminal 2, once: creates the `amaker` bucket
just run-author-rustfs           # terminal 2: author against rustfs
just run-assess-rustfs           # terminal 3
just run-analyze-rustfs          # terminal 4
```

Each `run-*-rustfs` recipe just sets `<PREFIX>_STORAGE_BACKEND=s3_compatible`
plus endpoint/bucket/credential vars before invoking the binary — the same
config you'd set in production against R2, B2, Hetzner, GCS-via-S3-interop,
or any other S3-protocol provider.

## Development

```bash
just test            # workspace tests
just lint            # clippy --deny-warnings
just fmt             # rustfmt
just book            # build the architecture mdBook
just book-serve      # serve it with live reload on :4000
```

## License

MIT

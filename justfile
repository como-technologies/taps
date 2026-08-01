# Default: list available recipes
default:
    @just --list

# Run all CI checks — the whole suite gate, one command. crate-audit is
# deliberately NOT a leg: it runs as a separate CI job (plus a weekly
# schedule), so a fresh advisory can't mask the code gates.
ci: fmt-check lint test lanes adr-check books

# House vocabulary for the full local gate
alias gate := ci

# Format code
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt --check

# Lint the whole workspace including tests and examples
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Run the workspace test suite
test:
    cargo test --workspace

# Validate every product's ADR corpus with the in-tree adroit — the suite's
# self-hosted dogfood gate, one recipe (portfolio#8; this replaced six
# per-repo recipes and their binary-resolution chains). KB-only adroit
# (adroit ADR-0020): each committed corpus is seeded into an ephemeral KB
# space and validated there; docs/src/adr stays the repo of record.
adr-check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    cargo build -q -p adroit
    for corpus in adroit assessments conduit portfolio pulse tuesday; do
        echo "adr-check: $corpus"
        tmp="$(mktemp -d)"
        printf 'name = "adrs"\n' > "$tmp/wiki.toml"
        mkdir -p "$tmp/wiki/decisions"
        target/debug/adroit seed --from "$corpus/docs/src/adr" --dir "$tmp"
        target/debug/adroit check --dir "$tmp"
        rm -rf "$tmp"
    done

# The per-product invariant lanes a blanket --workspace build can't see
# (portfolio#8). Each guards an accepted decision:
lanes:
    # conduit is fully synchronous (conduit ADR-0001, poll-tick loop): its
    # dependency graph must stay tokio-free.
    bash -c "! cargo tree -p conduit -e normal | grep -qw tokio"
    # adroit's feature layering: the bare core (no tui/ai/forge) builds,
    # lints, and tests clean — it never pulls the TUI/async stacks.
    cargo clippy -p adroit --no-default-features -- -D warnings
    cargo test -q -p adroit --no-default-features
    # adroit's web feature, Rust side (the Vue bundle build stays in
    # adroit's own justfile: `just web-build`).
    cargo clippy -p adroit --features web -- -D warnings
    # tuesday-core stays wasm32-compatible, no tokio (tuesday ADR-0002).
    rustup target add wasm32-unknown-unknown
    cargo check -q -p tuesday-core --target wasm32-unknown-unknown
    # tuesday-web's per-renderer feature checks (dx builds stay per-product).
    cargo check -q -p tuesday-web --features web
    cargo check -q -p tuesday-web --no-default-features --features server

# Build every product's mdBook (portfolio needs mdbook-mermaid; the gruvbox
# theme's shared assets live in docs-theme/).
books:
    mdbook build adroit/docs
    mdbook build assessments/docs
    mdbook build conduit/docs
    mdbook build portfolio/docs
    mdbook build pulse/docs
    mdbook build tuesday/docs

# Audit dependencies for known vulnerabilities against the workspace's one
# .cargo/audit.toml (skipped if cargo-audit isn't installed; CI always runs it)
crate-audit:
    @if command -v cargo-audit >/dev/null 2>&1; then cargo audit; else echo "skip: cargo-audit not installed (cargo install cargo-audit)"; fi

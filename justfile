# Default: list available recipes
default:
    @just --list

# Run all CI checks. This local gate is the source of truth; later waves of
# the workspace migration (portfolio#8) add books, the per-product invariant
# lanes, and the dependency audit.
ci: fmt-check lint test adr-check

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

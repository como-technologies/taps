# Default: list available recipes
default:
    @just --list

# Run all CI checks. This local gate is the source of truth; later waves of
# the workspace migration (portfolio#8) add books, ADR validation, the
# per-product invariant lanes, and the dependency audit.
ci: fmt-check lint test

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

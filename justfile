# Default: list available recipes
default:
    @just --list

# Run all CI checks. Placeholder: cargo refuses to run against a virtual
# workspace with no members, so until wave 1 (portfolio#8) copies the
# products in, the gate has nothing to check. Wave 1 replaces this with the
# real fmt/lint/test recipes.
ci:
    @echo "wave 0 skeleton: no workspace members yet; gate is a no-op"

# House vocabulary for the full local gate
alias gate := ci

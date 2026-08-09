# Default: list available recipes
default:
    @just --list

# mdbook + preprocessors for `books`, cargo-audit for `crate-audit`,
# live-server + inotify-tools (apt only for now — a fully cross-platform
# watcher is issue 45) for `books-serve`'s rebuild-and-reload loop.
# One-time setup: install the tools the root recipes need
init:
    #!/usr/bin/env bash
    set -euo pipefail
    # --locked: build each tool from its own committed lockfile, so a
    # dep's fresh MSRV bump can't break the install under the repo's
    # pinned toolchain (cargo-audit hit exactly this on 1.95).
    cargo install --locked mdbook mdbook-mermaid mdbook-gruvbox cargo-audit live-server
    if ! command -v inotifywait >/dev/null 2>&1; then
        if command -v apt-get >/dev/null 2>&1; then
            sudo apt-get install -y inotify-tools
        else
            echo "no apt: install inotify-tools with your package manager for instant books-serve rebuilds"
        fi
    fi

# The Getting Started guide's Step 1. One release build of the whole
# workspace, then every product binary lands in ~/.cargo/bin — already
# on PATH via rustup, so no per-shell exports. The set is derived from
# the build output itself (top-level executables in target/release), so
# a new binary ships automatically; feature-gated tools that don't build
# by default (pulse-simulate) stay with their product's justfile.
# Build the whole suite and put every product binary on PATH
install:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    cargo build --release
    for f in target/release/*; do
        [ -f "$f" ] && [ -x "$f" ] || continue
        case "$f" in *.so|*.d|*.rlib) continue ;; esac
        install -m 755 "$f" ~/.cargo/bin/
        echo "installed: $(basename "$f")"
    done

# Every product binary answers --version (suite convention) and the set
# derives from the release build output, so this doubles as the guide's
# Step 1 verify. Errors if a binary is missing from PATH or misbehaves.
# Show the version of every installed product binary
versions:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    shopt -s nullglob
    found=0
    for f in target/release/*; do
        [ -f "$f" ] && [ -x "$f" ] || continue
        case "$f" in *.so|*.d|*.rlib) continue ;; esac
        found=1
        "$(basename "$f")" --version
    done
    [ "$found" = 1 ] || { echo "no release binaries found — run 'just install' first"; exit 1; }

# The taps-level seams: live in-tree binaries driven at each other over
# their transports (taps-tests crate), env-gated so the plain workspace
# test run stays hermetic. Builds what the tests spawn, then runs them.
# Run the cross-product integration tests
integration:
    cargo build -p llm-wiki -p amaker-cli -p adroit
    TAPS_INTEGRATION=1 cargo test -p taps-tests

# A KB for developing tools against: registers ~/spaces/NAME in your
# registry (idempotent — spaces create no-ops if it exists) and serves
# the streamable-HTTP transport in the foreground, Ctrl-C to stop.
# Harness authoring doesn't need this (a workspace's .mcp.json spawns
# its own stdio serve); this is the stable endpoint for a tool under
# development, extra sessions, or curl.
# Serve a local dev KB over HTTP (localhost:8080/mcp)
kb-dev name="devkb":
    #!/usr/bin/env bash
    set -euo pipefail
    command -v llm-wiki >/dev/null || { echo "llm-wiki not on PATH — run 'just install' first" >&2; exit 1; }
    llm-wiki spaces create "$HOME/spaces/{{name}}" --name "{{name}}" --set-default
    exec llm-wiki serve --http

# Stamps the kit template (posture CLAUDE.md, settings, skills) into DIR
# and rewrites .mcp.json for local use — the shipped one is the guide's
# incus bridge. transport=stdio (default) spawns a private serve per
# session; transport=http points at a running `just kb-dev` (prefer this
# when kb-dev is up: two engines on one space contend for index locks).
# Create a local authoring workspace (cd there and run your harness)
kb-workspace dir="$HOME/kb-workspace" transport="stdio":
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    dir="{{dir}}"
    mkdir -p "$dir"
    cp -r llm-wiki/kit/workspace/. "$dir"/
    mkdir -p "$dir/.claude/skills"
    cp -r llm-wiki/kit/skills/. "$dir/.claude/skills/"
    case "{{transport}}" in
        stdio) printf '%s\n' \
            '{' \
            '  "mcpServers": {' \
            '    "kb": { "command": "llm-wiki", "args": ["serve"] }' \
            '  }' \
            '}' > "$dir/.mcp.json" ;;
        http) printf '%s\n' \
            '{' \
            '  "mcpServers": {' \
            '    "kb": { "type": "http", "url": "http://localhost:8080/mcp" }' \
            '  }' \
            '}' > "$dir/.mcp.json" ;;
        *) echo "unknown transport: {{transport}} (stdio|http)" >&2; exit 1 ;;
    esac
    echo "workspace ready: $dir ({{transport}})"

# Run all CI checks — the whole suite gate, one command. crate-audit is
# deliberately NOT a leg: it runs as a separate CI job (plus a weekly
# schedule), so a fresh advisory can't mask the code gates.
ci: fmt-check lint test lanes books

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

# The per-product invariant lanes a blanket --workspace build can't see.
# Each guards an accepted decision:
lanes:
    # conduit is fully synchronous (conduit ADR-0001, poll-tick loop): its
    # dependency graph must stay tokio-free.
    bash -c "! cargo tree -p conduit -e normal | grep -qw tokio"
    # adroit is greenfield (taps #93) — its lanes return as the new core
    # earns them.
    # tuesday-core stays wasm32-compatible, no tokio (tuesday ADR-0002).
    rustup target add wasm32-unknown-unknown
    cargo check -q -p tuesday-core --target wasm32-unknown-unknown
    # tuesday-web's per-renderer feature checks (dx builds stay per-product).
    cargo check -q -p tuesday-web --features web
    cargo check -q -p tuesday-web --no-default-features --features server

# (portfolio needs mdbook-mermaid; the gruvbox theme's shared assets
# live in docs-theme/)
# Build every book — the six products' plus the Getting Started guide
books:
    mdbook build assessments/docs
    mdbook build conduit/docs
    mdbook build getting-started/docs
    mdbook build llm-wiki/docs
    mdbook build portfolio/docs
    mdbook build pulse/docs
    mdbook build tuesday/docs

# The one definition of the site layout: .github/workflows/pages.yml
# publishes exactly this recipe's output.
# Assemble the published site into target/site — every book under one root
site: books
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    # Clear contents but keep the directory itself: books-serve's
    # live-server watches this root, and deleting it would orphan the watch.
    mkdir -p target/site && rm -rf target/site/*
    cp -r assessments/target/book   target/site/assessments
    cp -r conduit/docs/book         target/site/conduit
    cp -r getting-started/target/book target/site/getting-started
    cp -r llm-wiki/docs/book        target/site/llm-wiki
    cp -r portfolio/target/book     target/site/portfolio
    cp -r pulse/docs/book           target/site/pulse
    cp -r tuesday/docs/book         target/site/tuesday
    cat > target/site/index.html <<'EOF'
    <!DOCTYPE html>
    <html lang="en"><head><meta charset="utf-8">
    <title>Como Technologies — TAPS suite</title>
    <style>body{font-family:sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;background:#282828;color:#ebdbb2}a{color:#83a598}</style>
    </head><body>
    <h1>The TAPS suite</h1>
    <p><a href="portfolio/">Start with the portfolio book</a> — the reader-facing story.</p>
    <p><a href="getting-started/">New here? The Getting Started guide</a> — from a fresh machine to a first trip around the loop.</p>
    <ul>
      <li>adroit — decision records (Prescribe; book returns with taps #93's docs pass)</li>
      <li><a href="assessments/">assessments</a> — amaker (Assess)</li>
      <li><a href="conduit/">conduit</a> — the Adopt engine</li>
      <li><a href="llm-wiki/">llm-wiki</a> — the knowledge-base engine</li>
      <li><a href="pulse/">pulse</a> — anonymous signal (Measure)</li>
      <li><a href="tuesday/">tuesday</a> — effort attribution (Measure)</li>
      <li><a href="portfolio/">portfolio</a> — the book</li>
      <li><a href="getting-started/">getting started</a> — the tutorial</li>
    </ul>
    </body></html>
    EOF
    echo "site assembled at target/site/"

# Fail fast before building anything if the watch/serve tools are missing.
# Linux-only, deliberately (the team is): cross-platform is issue 45.
_need-watch-tools:
    @command -v inotifywait >/dev/null 2>&1 || { echo "inotifywait not found — run 'just init' (installs inotify-tools)"; exit 1; }
    @command -v live-server >/dev/null 2>&1 || { echo "live-server not found — run 'just init' (installs it)"; exit 1; }

# A faithful mirror of the published Pages layout, so cross-book links
# resolve. live-server watches the assembled site and pushes a reload
# to the browser when the rebuild lands — no manual refresh.
# Serve every book on one port, rebuilding + reloading on source changes
books-serve port="8000": _need-watch-tools site
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{justfile_directory()}}"
    live-server --hard -p {{port}} target/site >/dev/null 2>&1 &
    server=$!
    trap 'kill $server 2>/dev/null' EXIT
    echo "all books at http://localhost:{{port}}/ — watching book sources; Ctrl-C stops"
    # Block until a source file changes; the exclude keeps the books that
    # build into docs/book from re-triggering.
    while inotifywait -qq -r -e modify,create,delete,move \
          --exclude '/docs/book/' \
          assessments/docs conduit/docs getting-started/docs \
          llm-wiki/docs portfolio/docs pulse/docs tuesday/docs docs-theme; do
        sleep 0.3       # coalesce editor save bursts
        echo "change detected — rebuilding site…"
        if just site >/dev/null 2>&1; then
            echo "rebuilt — browser reloads itself"
        else
            echo "site rebuild FAILED — run 'just site' to see why; still watching"
        fi
    done

# Audit dependencies for known vulnerabilities against the workspace's one
# .cargo/audit.toml (skipped if cargo-audit isn't installed; CI always runs it)
crate-audit:
    @if command -v cargo-audit >/dev/null 2>&1; then cargo audit; else echo "skip: cargo-audit not installed (cargo install --locked cargo-audit)"; fi

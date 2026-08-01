#!/usr/bin/env bash
# Create a per-run UNIQUE demo workdir for the client-corpus demo.
#
# Run-1 learning: the repo's shared .conduit/ store is not single-writer —
# two demo flows interleaving in one store stomp each other's cursors and
# task records. Every demo run therefore gets its own workdir carrying
# everything `conduit` resolves from its cwd:
#
#   conduit.toml   demo/client-corpus.conduit.toml with @ADROIT_DIR@ resolved
#                  to the per-run corpus SPACE (see corpus-space below)
#   corpus-space   a per-run KB space (wiki.toml + wiki/decisions), seeded
#                  from the client corpus's legacy docs/src/adr with the
#                  pinned adroit's `seed` — the KB-only adroit (its ADR-0020)
#                  reads spaces, never bare directories. The forge repo stays
#                  legacy-format: repo of record canonical, space derived
#                  (portfolio ADR-0009). Wiped and reseeded like every other
#                  workdir artifact.
#   .secrets       symlink to this repo's .secrets/ (the pinned token
#                  filenames gitea-init.sh mints)
#   .conduit/bin   symlink to this repo's pinned adroit install
#
# All run state (tasks/plans/cursor/cache/workspaces) lands under
# <workdir>/.conduit — unique per run, inspectable, disposable.
#
# Usage:
#   RUN_DIR=$(bash demo/client-corpus-init.sh)   # then: cd "$RUN_DIR"
# Env:
#   RUN_DIR            workdir to create (default demo/runs/<UTC timestamp>;
#                      must not already exist — unique per run)
#   CLIENT_CORPUS_DIR  an already-built client corpus repo (must contain
#                      docs/src/adr). When unset, demo/client-corpus-build.sh
#                      constructs one from llm-wiki's starter content
#                      (kit/starter/decisions), resolving llm-wiki per the
#                      suite convention: COMO_LLM_WIKI_DIR -> sibling
#                      ../llm-wiki -> the gitignored .como/deps/llm-wiki
#                      clone cache (COMO_LLM_WIKI_GIT / COMO_GIT_BASE) -> a
#                      hard, actionable error. COMO_OFFLINE=1 uses a
#                      populated cache as-is and never clones.
# Prints the created workdir's absolute path on stdout; everything else
# goes to stderr.
set -euo pipefail

cd "$(dirname "$0")/.." # conduit repo root
ROOT="$PWD"
RUN_DIR="${RUN_DIR:-demo/runs/$(date -u +%Y%m%dT%H%M%SZ)}"

# Resolve the client corpus: an explicit prebuilt repo via CLIENT_CORPUS_DIR
# (demo-up and the tests pass one), else construct it from llm-wiki's
# starter content.
CLIENT_CORPUS_DIR="${CLIENT_CORPUS_DIR:-}"
if [ -z "$CLIENT_CORPUS_DIR" ]; then
  CLIENT_CORPUS_DIR="$(bash demo/client-corpus-build.sh)"
fi
if [ ! -d "${CLIENT_CORPUS_DIR:-}/docs/src/adr" ]; then
  echo "ERROR: no client corpus found (need a repo containing docs/src/adr)." >&2
  echo "  Knobs: CLIENT_CORPUS_DIR (a prebuilt corpus repo), or let demo/client-corpus-build.sh" >&2
  echo "  construct one — it needs an llm-wiki checkout (COMO_LLM_WIKI_DIR, sibling ../llm-wiki," >&2
  echo "  or COMO_LLM_WIKI_GIT / COMO_GIT_BASE for the .como/deps/llm-wiki clone cache)." >&2
  exit 1
fi
ADRS="$(cd "$CLIENT_CORPUS_DIR/docs/src/adr" && pwd)"

# Forge repo name the demo seeds + drives. Knob (default "client-corpus") so
# the machinery can target a different corpus repo without hand-editing the
# generated conduit.toml — the third sighting of the hardcoded-target lesson
# (gitea-init.sh / demo-trigger.sh already take REPO_NAME; this seam was missed).
REPO_NAME="${REPO_NAME:-client-corpus}"

if [ -e "$RUN_DIR" ]; then
  echo "ERROR: RUN_DIR=$RUN_DIR already exists — each run gets a fresh workdir" >&2
  exit 1
fi
mkdir -p "$RUN_DIR/.conduit"

# Symlinks, not copies: tokens stay in one gitignored place (and may be
# re-minted by a later forge-up); the pinned adroit is shared read-only.
ln -s "$ROOT/.secrets" "$RUN_DIR/.secrets"
ln -s "$ROOT/.conduit/bin" "$RUN_DIR/.conduit/bin"

# The per-run corpus SPACE: the pinned adroit is KB-only (its ADR-0020), so
# the workdir carries a KB space seeded from the client corpus's legacy
# docs/src/adr. Hand-scaffold (wiki.toml + wiki/decisions — no llm-wiki
# binary required, the same shape the suite gates use), then seed with the
# pinned adroit. Wipe-and-reseed keeps re-runs idempotent, like every other
# workdir artifact. The forge repo itself stays legacy-format — repo of
# record canonical, space derived.
ADROIT_BIN="${ADROIT_BIN:-$ROOT/.conduit/bin/adroit}"
if [ ! -x "$ADROIT_BIN" ]; then
  echo "ERROR: pinned adroit not found at $ADROIT_BIN — run 'just init-adroit'" >&2
  echo "  (or point ADROIT_BIN at an adroit binary carrying the KB-only contract)." >&2
  exit 1
fi
SPACE="$RUN_DIR/corpus-space"
rm -rf "$SPACE"
mkdir -p "$SPACE/wiki/decisions"
printf 'name = "client"\n' >"$SPACE/wiki.toml"
"$ADROIT_BIN" seed --from "$ADRS" --dir "$SPACE" >&2
# KB spaces are git-backed by convention (llm-wiki's ingest commits into the
# space); identity pinned locally so the demo never depends on — or leaks —
# the operator's git config. Content commits stay with the consumers.
git -C "$SPACE" init --quiet --initial-branch=main
git -C "$SPACE" config user.name "como-demo"
git -C "$SPACE" config user.email "demo@localhost"
git -C "$SPACE" config commit.gpgsign false

sed -e "s|@ADROIT_DIR@|$(cd "$SPACE" && pwd)|" -e "s|@REPO_NAME@|$REPO_NAME|" \
  -e "s|@FORGE_PORT@|${FORGE_PORT:-3000}|" \
  demo/client-corpus.conduit.toml >"$RUN_DIR/conduit.toml"

ABS="$(cd "$RUN_DIR" && pwd)"
echo "demo workdir ready: $ABS (corpus space: $ABS/corpus-space, seeded from $ADRS)" >&2
echo "$ABS"

#!/usr/bin/env bash
# Build the fictional client's decision-corpus repo from llm-wiki's starter
# content. The demo needs a client repo to seed onto the throwaway forge and
# an ADR corpus for the pinned adroit to read; llm-wiki ships that starter
# content at kit/starter/decisions/ (5 accepted + 11 proposed decisions,
# ADR-0005 carrying a stored plan). This script resolves the llm-wiki
# checkout, copies kit/starter/decisions into docs/src/adr of a freshly
# constructed single-commit git repo, and prints that repo's absolute path
# on stdout (provenance notes go to stderr).
#
# Idempotent by reconstruction: the output dir is wiped and rebuilt on every
# run — it is a generated artifact, never hand-edited, and the llm-wiki
# checkout is only ever read.
#
# Env:
#   LLM_WIKI_DIR  an llm-wiki checkout (must contain kit/starter/decisions).
#                 When unset: COMO_LLM_WIKI_DIR -> the in-tree ../llm-wiki
#                 (the workspace product) -> a hard, actionable error.
#   CLIENT_CORPUS_BUILD_DIR  where to construct the repo
#                            (default .como/build/client-corpus; gitignored)
set -euo pipefail

cd "$(dirname "$0")/.." # conduit repo root

# Resolve llm-wiki: explicit LLM_WIKI_DIR -> COMO_LLM_WIKI_DIR -> the
# in-tree ../llm-wiki (the workspace product) -> hard error (the demo needs
# the starter content).
LLM_WIKI_DIR="${LLM_WIKI_DIR:-${COMO_LLM_WIKI_DIR:-}}"
if [ -z "$LLM_WIKI_DIR" ] && [ -d ../llm-wiki/kit/starter/decisions ]; then
  LLM_WIKI_DIR=../llm-wiki
  echo "client-corpus-build: llm-wiki -> ../llm-wiki (in-tree)" >&2
fi
if [ ! -d "${LLM_WIKI_DIR:-}/kit/starter/decisions" ]; then
  echo "ERROR: no llm-wiki starter content found (need a checkout containing kit/starter/decisions)." >&2
  echo "  Knobs: LLM_WIKI_DIR or COMO_LLM_WIKI_DIR (an llm-wiki checkout; the in-tree ../llm-wiki" >&2
  echo "  is the default)." >&2
  exit 1
fi
SRC="$(cd "$LLM_WIKI_DIR/kit/starter/decisions" && pwd)"

OUT="${CLIENT_CORPUS_BUILD_DIR:-.como/build/client-corpus}"
rm -rf "$OUT"
mkdir -p "$OUT/docs/src/adr"
cp -R "$SRC/." "$OUT/docs/src/adr/"

# One commit on main — gitea-init.sh seeds the forge by pushing this repo's
# main. Identity is pinned (and signing disabled) locally so the build never
# depends on — or leaks — the operator's git config.
git -C "$OUT" init --quiet --initial-branch=main
git -C "$OUT" add -A
git -C "$OUT" -c user.name="como-demo" -c user.email="demo@localhost" \
  -c commit.gpgsign=false \
  commit --quiet -m "Seed the fictional client corpus from llm-wiki kit/starter/decisions"

ABS="$(cd "$OUT" && pwd)"
echo "client-corpus-build: built $ABS (corpus: docs/src/adr, from $SRC)" >&2
echo "$ABS"

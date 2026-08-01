#!/usr/bin/env bash
# Cross-check the Adopt→Measure loop closure (tuesday M5).
#
# Takes the two machine reports produced on either side of the loop —
#   1. `conduit verify <address> -o json` — Adopt: the executable tagging
#      contract, asserted live against the forge,
#   2. `tuesday-report ... -o json --strict` — Measure: the canonical
#      MonthlyReport over the same forge,
# — and asserts both tools agree on the SAME ground truth for the verified
# PR: its number, its effort:* label, and its ADR reference (including a
# positive adr_totals entry). Exits nonzero on the first disagreement.
#
# Usage: scripts/cross-check.sh <conduit-verify.json> <tuesday-report.json>
#
# Extraction notes: conduit's verify JSON carries the PR number structurally
# (.pr) but the effort label and ADR reference only inside check `detail`
# strings. The six check NAMES are fixed identifiers (asserted in conduit's
# tests) precisely so Measure-side consumers can key on them — which is what
# this script does.
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: $0 <conduit-verify.json> <tuesday-report.json>" >&2
  exit 2
fi
verify_json="$1"
report_json="$2"

fail() {
  echo "CROSS-CHECK FAIL: $*" >&2
  exit 1
}

# ── Adopt side: conduit verify ─────────────────────────────────────────────
jq -e '.pass == true' "$verify_json" >/dev/null ||
  fail "conduit verify did not pass — fix Adopt before cross-checking"

conduit_pr=$(jq -r '.pr' "$verify_json")
conduit_effort=$(jq -r '.checks[] | select(.name == "exactly_one_effort_label")
  | .detail | capture("\\[\"(?<l>effort:[^\"]+)\"\\]").l' "$verify_json")
conduit_adr=$(jq -r '.checks[] | select(.name == "adr_label_present")
  | .detail | capture("want \"adr:(?<r>ADR-[0-9]{4})\"").r' "$verify_json")

[ -n "$conduit_pr" ] && [ -n "$conduit_effort" ] && [ -n "$conduit_adr" ] ||
  fail "could not extract pr/effort/adr from $verify_json"

# ── Measure side: the allocation for that PR number ────────────────────────
alloc=$(jq --argjson pr "$conduit_pr" \
  '[.allocations[] | select(.pr_number == $pr)]' "$report_json")
count=$(jq 'length' <<<"$alloc")
[ "$count" = 1 ] ||
  fail "tuesday report has $count allocations for PR $conduit_pr (want exactly 1)"

tuesday_adr=$(jq -r '.[0].adr_id // ""' <<<"$alloc")
tuesday_score=$(jq -r '.[0].effort_score' <<<"$alloc")

# MonthlyReport serializes the effort as the enum variant name; map it back
# to the contract label (the closed five-value set both tools share).
case "$tuesday_score" in
SuperQuick) tuesday_effort="effort:1-super-quick" ;;
NotLong) tuesday_effort="effort:2-not-long" ;;
Average) tuesday_effort="effort:3-average" ;;
AWhile) tuesday_effort="effort:4-a-while" ;;
FeltLikeForever) tuesday_effort="effort:5-felt-like-forever" ;;
*) fail "unknown tuesday effort_score \"$tuesday_score\"" ;;
esac

# ── The three agreements ───────────────────────────────────────────────────
echo "pr:     conduit=$conduit_pr tuesday=$(jq -r '.[0].pr_number' <<<"$alloc")"

[ "$conduit_effort" = "$tuesday_effort" ] ||
  fail "effort disagrees: conduit=$conduit_effort tuesday=$tuesday_effort ($tuesday_score)"
echo "effort: conduit=$conduit_effort tuesday=$tuesday_effort ($tuesday_score)"

[ "$conduit_adr" = "$tuesday_adr" ] ||
  fail "adr disagrees: conduit=$conduit_adr tuesday=${tuesday_adr:-null}"
jq -e --arg adr "$conduit_adr" '.adr_totals[$adr] > 0' "$report_json" >/dev/null ||
  fail "adr_totals[$conduit_adr] is missing or zero in $report_json"
adr_hours=$(jq -r --arg adr "$conduit_adr" '.adr_totals[$adr]' "$report_json")
echo "adr:    conduit=$conduit_adr tuesday=$tuesday_adr (adr_totals: ${adr_hours}h)"

echo "CROSS-CHECK PASS: PR $conduit_pr, $conduit_effort, $conduit_adr — Adopt and Measure agree"

//! tuesday-cli: the headless CLI head over tuesday-core (ADR-0004).
//!
//! Drives a [`tuesday_core::PrSource`] through the effort calculator and
//! serializes THE canonical serde [`tuesday_core::MonthlyReport`] — the same
//! byte-compatibility-pinned schema the web head's export endpoint emits.
//! `--strict` enforces the ADR-0005 allocation ruling with a nonzero exit.
//! `--from/--to` (ADR-0007) widens the window to an inclusive multi-month
//! range: one unchanged canonical report per month inside an additive
//! envelope carrying a cross-month `adr_totals` rollup; strict is checked
//! month by month.
//!
//! # Adopt→Measure loop closure (M5), captured 2026-06-12
//!
//! The loop closed with machine evidence on both sides against the
//! throwaway forge. Adopt: conduit drove ADR-0002 through its full
//! lifecycle on como/conduit-dogfood (plan → `conduit:run` → InReview →
//! reviewer approve/merge → Merged), and `conduit verify 2 -o json` passed
//! all six tagging-contract checks, exit 0, on PR 2. Measure: `just
//! dogfood-report` (this binary, `--strict`) read the same forge for June
//! 2026 — exit 0, PR 2 allocated with `effort_score: SuperQuick` from
//! `effort:1-super-quick`, and `adr_totals` carrying ADR-0002's 360.0h.
//! `scripts/cross-check.sh` then asserted both reports agree on the same
//! ground truth — PR number 2, label `effort:1-super-quick`, reference
//! ADR-0002 — and exits nonzero when any of the three is tampered (all
//! three negative legs exercised in the captured run). The full
//! dogfood-contract narrative lives in the house-stack branch's book,
//! `docs/src/dogfood-contract.md`.

pub mod cli;
pub mod measure;
pub mod render;
pub mod report;
pub mod run;
pub mod strict;
pub mod token;
pub mod window;

#[cfg(test)]
pub(crate) mod fake;

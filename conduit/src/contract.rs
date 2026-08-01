//! ALL tuesday-contract emission (spec §The tuesday contract). Pure — no I/O.
//! tuesday (the Measure stage) reads these labels/titles/trailers at merge
//! time.
//!
//! The contract itself lives in `como-contract` (the suite's shared-seam
//! crate, portfolio ADR-0012): conduit emits and tuesday validates the SAME
//! constants, so the seam cannot drift (the pre-workspace twins were this
//! module's literals and tuesday's private `VALID_EFFORT_LABELS` —
//! tuesday#65). Re-exported here so `crate::contract::*` stays the in-crate
//! path.

pub use como_contract::tuesday::*;

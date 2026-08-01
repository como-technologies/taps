//! The suite's cross-product seams, as types — truth by construction
//! (portfolio ADR-0012). Before the workspace these were maintained as
//! byte-identical twins (conduit's `contract.rs` vs tuesday's private label
//! array; conduit's hand-written tolerant mirrors of adroit's view types)
//! with no gate tying them together.
//!
//! - [`tuesday`]: everything conduit emits that tuesday (the Measure stage)
//!   reads back at merge time — labels, titles, trailers, branch shapes.
//! - [`adroit`]: the read slice of adroit's `-o json` surface that conduit
//!   (the Adopt stage) consumes, plus the manifest handshake.

pub mod adroit;
pub mod tuesday;

pub use tuesday::{
    CHECK_ADR_LABEL_PRESENT, CHECK_BRANCH_SHAPE, CHECK_EXACTLY_ONE_EFFORT,
    CHECK_NEVER_ADR_NAMESPACE, CHECK_TITLE_PREFIX, CHECK_TRAILER_FINAL_LINE, EFFORT_LABELS,
    EffortBucket, EffortThresholds, LABEL_FAILED, LABEL_RUN,
};

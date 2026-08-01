//! Deterministic analysis layer.
//!
//! Three pure functions of `(Assessment, AssessmentResponse)` produce
//! everything the UI and the LLM narrative consume:
//!
//! - [`compute_scorecard`] — polarity-aware pass/fail rollups at
//!   practice, domain, and assessment levels.
//! - [`compute_gaps`] — every non-passing question with inherited
//!   narrative + operational metadata.
//! - [`compute_roadmap`] — gaps grouped by owner role and
//!   priority-ordered (inverse-effort heuristic for v1).
//!
//! The LLM-driven narrative report (which composes these three plus the
//! assessment into a Markdown findings document) lives in the binary
//! crate alongside `AiService`, since it depends on the LLM transport.

pub mod gaps;
pub mod roadmap;
pub mod scorecard;

pub use gaps::{GapInventory, compute_gaps};
pub use roadmap::{Roadmap, compute_roadmap};
pub use scorecard::{Scorecard, compute_scorecard};

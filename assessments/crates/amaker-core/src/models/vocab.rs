//! Per-assessment controlled vocabularies.
//!
//! Evidence types describe what supports a "yes" answer; blocker types
//! describe what's preventing a "no" answer from becoming "yes". Each
//! assessment carries its own list so the AI can tailor the vocabulary
//! to the domain (e.g. a restaurant assessment might add "Temperature
//! logs" as an evidence type).
//!
//! Staging infrastructure for M2 (Collection phase): most helpers here
//! are wired through by answer input handlers and the analysis layer.

#![allow(dead_code)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ids::{BlockerTypeId, EvidenceTypeId};

/// Estimated effort range (in hours) to remediate a failing question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "Estimated effort in hours to remediate, as a [min, max] range")]
pub struct EffortRange {
    pub min_hours: u32,
    pub max_hours: u32,
}

impl EffortRange {
    pub fn new(min_hours: u32, max_hours: u32) -> Self {
        Self {
            min_hours,
            max_hours,
        }
    }

    /// Midpoint of the range, useful for rough prioritization math.
    pub fn midpoint(&self) -> f32 {
        (self.min_hours as f32 + self.max_hours as f32) / 2.0
    }
}

/// A type of evidence that can support a "yes" answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceType {
    pub id: EvidenceTypeId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl EvidenceType {
    pub fn new(id: impl Into<EvidenceTypeId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
        }
    }
}

/// A type of blocker that can explain a "no" answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlockerType {
    pub id: BlockerTypeId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl BlockerType {
    pub fn new(id: impl Into<BlockerTypeId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
        }
    }
}

/// Default evidence vocabulary seeded for every new assessment.
///
/// The AI can tailor this per assessment via `tailor_vocabulary`; these
/// are the sensible starting set borrowed from the legacy assessment tool.
pub fn default_evidence_types() -> Vec<EvidenceType> {
    vec![
        EvidenceType::new("none", "None"),
        EvidenceType::new("process_doc", "Process / documentation / policy in place"),
        EvidenceType::new("tested_periodically", "Tested periodically"),
        EvidenceType::new("audited_certified", "Audited / certified"),
        EvidenceType::new("other", "Other"),
    ]
}

/// Default blocker vocabulary seeded for every new assessment.
pub fn default_blocker_types() -> Vec<BlockerType> {
    vec![
        BlockerType::new("people", "People"),
        BlockerType::new("time", "Time"),
        BlockerType::new("technology", "Technology"),
        BlockerType::new("training", "Training"),
        BlockerType::new("other", "Other"),
        BlockerType::new("unknown", "Unknown"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_evidence_types_have_stable_slugs() {
        let types = default_evidence_types();
        assert_eq!(types.len(), 5);
        assert_eq!(types[0].id.as_str(), "none");
        assert_eq!(types[3].id.as_str(), "audited_certified");
    }

    #[test]
    fn default_blocker_types_have_stable_slugs() {
        let types = default_blocker_types();
        assert_eq!(types.len(), 6);
        assert_eq!(types[0].id.as_str(), "people");
        assert_eq!(types[5].id.as_str(), "unknown");
    }

    #[test]
    fn effort_range_midpoint() {
        let e = EffortRange::new(2, 8);
        assert_eq!(e.midpoint(), 5.0);
    }
}

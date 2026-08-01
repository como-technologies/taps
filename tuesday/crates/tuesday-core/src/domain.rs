//! Neutral domain types shared by the calculator and all PR sources.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A merged pull request, reduced to exactly what the Measure stage needs.
///
/// Provider-specific shapes (GitHub GraphQL nodes, Gitea REST payloads)
/// are converted to this type at the provider boundary and never leak into
/// the calculator (ADR-0003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergedPr {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub url: String,
    pub merged_at: DateTime<Utc>,
    pub labels: Vec<String>,
}

//! The read-only ingestion seam (ADR-0003).

use crate::domain::MergedPr;
use serde::{Deserialize, Serialize};

/// Errors surfaced by a [`PrSource`].
pub type SourceError = Box<dyn std::error::Error + Send + Sync>;

/// Which forge provider backs a report — the dispatch key carried by
/// [`crate::ReportConfig`]. The CLI's `--source` flag is the same closed
/// set; the serde form is lowercase (`"github"` | `"gitea"`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    #[default]
    Github,
    Gitea,
}

/// A read-only source of merged pull requests.
///
/// Measure never mutates a forge: this trait has no write surface by
/// construction, and it stays at exactly the three read operations the
/// Measure stage needs (ADR-0003). Async-fn-in-trait is deliberate
/// (ADR-0002): core must compile to `wasm32-unknown-unknown`, where futures
/// are not `Send`; callers that need `Send` futures hold a concrete
/// provider type rather than a trait object.
#[allow(async_fn_in_trait)]
pub trait PrSource {
    /// List organization logins visible to the authenticated user.
    async fn list_orgs(&self) -> Result<Vec<String>, SourceError>;

    /// List repository names within an organization.
    async fn list_repos(&self, org: &str) -> Result<Vec<String>, SourceError>;

    /// Fetch the PRs merged in `owner/repo` during the given calendar month.
    async fn fetch_merged_prs(
        &self,
        owner: &str,
        repo: &str,
        year: u32,
        month: u32,
    ) -> Result<Vec<MergedPr>, SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calculator::{EffortCalculator, ScalingSeries};
    use crate::domain::MergedPr;
    use chrono::TimeZone;

    /// An in-memory source: proves the trait is implementable without a
    /// network and that its output drives the calculator end to end.
    struct FakeSource {
        prs: Vec<MergedPr>,
    }

    impl PrSource for FakeSource {
        async fn list_orgs(&self) -> Result<Vec<String>, SourceError> {
            Ok(vec!["como".to_string()])
        }

        async fn list_repos(&self, _org: &str) -> Result<Vec<String>, SourceError> {
            Ok(vec!["alpha".to_string()])
        }

        async fn fetch_merged_prs(
            &self,
            _owner: &str,
            _repo: &str,
            _year: u32,
            _month: u32,
        ) -> Result<Vec<MergedPr>, SourceError> {
            Ok(self.prs.clone())
        }
    }

    #[test]
    fn fake_source_drives_the_calculator() {
        let source = FakeSource {
            prs: vec![MergedPr {
                number: 7,
                title: "Add widget".to_string(),
                body: None,
                url: "http://forge.example/como/alpha/pulls/7".to_string(),
                merged_at: chrono::Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap(),
                labels: vec!["effort:3-average".to_string(), "adr:ADR-0001".to_string()],
            }],
        };

        let orgs = futures::executor::block_on(source.list_orgs()).unwrap();
        assert_eq!(orgs, vec!["como".to_string()]);
        let repos = futures::executor::block_on(source.list_repos("como")).unwrap();
        assert_eq!(repos, vec!["alpha".to_string()]);

        let prs =
            futures::executor::block_on(source.fetch_merged_prs("como", "alpha", 2026, 1)).unwrap();
        let report = EffortCalculator::new(360.0, ScalingSeries::Linear).calculate_report(
            vec![("alpha".to_string(), prs)],
            "January".to_string(),
            2026,
            "como".to_string(),
        );

        assert_eq!(report.allocations.len(), 1);
        assert_eq!(report.allocations[0].pr_number, 7);
        assert_eq!(report.adr_totals.get("ADR-0001"), Some(&360.0));
        assert!(report.unallocated_prs.is_empty());
    }
}

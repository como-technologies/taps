//! Drive a [`PrSource`] through the core effort calculator.

use tuesday_core::{
    EffortCalculator, MergedPr, MonthlyReport, PrSource, ScalingSeries, SourceError,
};

// The English month-name mapping lives in core beside the seam-driven
// pipeline; the CLI re-exports it so its callers and tests keep one name.
pub use tuesday_core::month_name;

/// Fetch the month's merged PRs for every repo, in CLI argument order.
pub async fn fetch_repo_prs<S: PrSource>(
    source: &S,
    owner: &str,
    repos: &[String],
    year: u32,
    month: u32,
) -> Result<Vec<(String, Vec<MergedPr>)>, SourceError> {
    let mut repo_prs = Vec::with_capacity(repos.len());
    for repo in repos {
        let prs = source.fetch_merged_prs(owner, repo, year, month).await?;
        repo_prs.push((repo.clone(), prs));
    }
    Ok(repo_prs)
}

/// Run the core calculator over the fetched PRs — the CLI adds nothing to
/// the math, so its report is the same canonical `MonthlyReport` the web
/// head serializes (ADR-0004).
pub fn build_report(
    repo_prs: Vec<(String, Vec<MergedPr>)>,
    owner: &str,
    year: u32,
    month: u32,
    monthly_hours: f64,
    scaling: ScalingSeries,
) -> Result<MonthlyReport, String> {
    let month = month_name(month)?;
    Ok(
        EffortCalculator::new(monthly_hours, scaling).calculate_report(
            repo_prs,
            month.to_string(),
            year,
            owner.to_string(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakePrSource, make_pr};

    #[test]
    fn month_name_maps_the_twelve_months_and_rejects_others() {
        assert_eq!(month_name(1), Ok("January"));
        assert_eq!(month_name(6), Ok("June"));
        assert_eq!(month_name(12), Ok("December"));
        assert!(month_name(0).is_err());
        assert!(month_name(13).is_err());
    }

    #[tokio::test]
    async fn fetches_every_repo_in_cli_order() {
        let source = FakePrSource::new()
            .with_repo(
                "como",
                "alpha",
                vec![make_pr(1, "One", None, &["effort:1-super-quick", "a"])],
            )
            .with_repo("como", "beta", Vec::new());

        let repos = vec!["beta".to_string(), "alpha".to_string()];
        let repo_prs = fetch_repo_prs(&source, "como", &repos, 2026, 3)
            .await
            .unwrap();

        assert_eq!(repo_prs.len(), 2);
        assert_eq!(repo_prs[0].0, "beta");
        assert!(repo_prs[0].1.is_empty());
        assert_eq!(repo_prs[1].0, "alpha");
        assert_eq!(repo_prs[1].1[0].number, 1);
    }

    #[tokio::test]
    async fn source_errors_propagate() {
        let source = FakePrSource::failing_with("forge unreachable");
        let err = fetch_repo_prs(&source, "como", &["alpha".to_string()], 2026, 3)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("forge unreachable"));
    }

    #[test]
    fn build_report_drives_the_core_calculator() {
        let repo_prs = vec![(
            "alpha".to_string(),
            vec![
                make_pr(7, "Add widget", None, &["effort:3-average", "adr:ADR-0001"]),
                make_pr(8, "Mystery", None, &["feature"]),
            ],
        )];

        let report = build_report(repo_prs, "como", 2026, 3, 360.0, ScalingSeries::Linear).unwrap();

        assert_eq!(report.month, "March");
        assert_eq!(report.year, 2026);
        assert_eq!(report.organization, "como");
        assert_eq!(report.total_hours, 360.0);
        // The canonical report includes the ADR rollup (ADR-0004)...
        assert_eq!(report.adr_totals.get("ADR-0001"), Some(&360.0));
        // ...and the QC list of unallocatable PRs.
        assert_eq!(
            report.unallocated_prs,
            vec![("alpha".to_string(), 8, "Mystery".to_string())]
        );
    }

    #[test]
    fn build_report_rejects_invalid_months() {
        assert!(build_report(Vec::new(), "como", 2026, 13, 360.0, ScalingSeries::Linear).is_err());
    }

    #[test]
    fn scaling_series_reaches_the_calculator() {
        // Doubling: effort 2 -> 2 pts, effort 5 -> 16 pts; 360h/18pts = 20 h/pt.
        let repo_prs = vec![(
            "alpha".to_string(),
            vec![
                make_pr(1, "Small", None, &["effort:2-not-long", "a"]),
                make_pr(2, "Big", None, &["effort:5-felt-like-forever", "b"]),
            ],
        )];
        let report =
            build_report(repo_prs, "como", 2026, 3, 360.0, ScalingSeries::Doubling).unwrap();
        assert_eq!(report.hours_per_point, 20.0);
    }
}

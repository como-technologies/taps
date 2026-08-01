//! The pipeline behind `main`: PrSource → calculator → renderer → strict
//! verdict, in single-month and multi-month (--from/--to, ADR-0007) form.
//! Generic over the source so the whole flow is testable against the
//! in-crate `FakePrSource`.

use crate::cli::OutputFormat;
use crate::strict::StrictViolation;
use crate::window::{YearMonth, months_inclusive};
use tuesday_core::{MonthlyReport, PrSource, ScalingSeries};

/// Everything `run_with_source` needs besides the source itself.
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub owner: String,
    pub repos: Vec<String>,
    pub year: u32,
    pub month: u32,
    pub monthly_hours: f64,
    pub scaling: ScalingSeries,
    pub output: OutputFormat,
    pub strict: bool,
}

/// What the process should do: print `stdout` verbatim, list `violations`
/// on stderr, and exit nonzero iff `violations` is nonempty. `reports`
/// carries each month's canonical report beside its window so the `--kb`
/// page emission (kb.rs) renders from the same data the output did.
#[derive(Debug)]
pub struct RunOutcome {
    pub stdout: String,
    pub violations: Vec<StrictViolation>,
    pub reports: Vec<(YearMonth, MonthlyReport)>,
}

/// Fetch, calculate, render. The report is always produced (even when
/// strict violations exist, the JSON artifact stays inspectable); the
/// violations only drive the exit code.
pub async fn run_with_source<S: PrSource>(
    source: &S,
    request: &RunRequest,
) -> Result<RunOutcome, String> {
    let repo_prs = crate::report::fetch_repo_prs(
        source,
        &request.owner,
        &request.repos,
        request.year,
        request.month,
    )
    .await
    .map_err(|e| format!("fetching merged PRs: {e}"))?;

    let violations = if request.strict {
        crate::strict::check_strict(&repo_prs)
    } else {
        Vec::new()
    };

    let report = crate::report::build_report(
        repo_prs,
        &request.owner,
        request.year,
        request.month,
        request.monthly_hours,
        request.scaling,
    )?;

    let stdout = match request.output {
        OutputFormat::Json => crate::render::render_json(&report),
        OutputFormat::Table => crate::render::render_table(&report),
    };

    Ok(RunOutcome {
        stdout,
        violations,
        reports: vec![(
            YearMonth {
                year: request.year,
                month: request.month,
            },
            report,
        )],
    })
}

/// Everything `run_range_with_source` needs: the shared request fields plus
/// the inclusive `--from/--to` window (ADR-0007).
#[derive(Debug, Clone)]
pub struct RangeRequest {
    pub owner: String,
    pub repos: Vec<String>,
    pub from: YearMonth,
    pub to: YearMonth,
    pub monthly_hours: f64,
    pub scaling: ScalingSeries,
    pub output: OutputFormat,
    pub strict: bool,
}

/// The multi-month pipeline (ADR-0007): fetch, check, and calculate **per
/// month** — each month is the same canonical single-month report, with the
/// full `monthly_hours` budget and the strict contract applied to that
/// month's PRs alone — then render the months inside the additive range
/// envelope with the cross-month `adr_totals` rollup. An empty month yields
/// an empty report, not a hole in the range.
pub async fn run_range_with_source<S: PrSource>(
    source: &S,
    request: &RangeRequest,
) -> Result<RunOutcome, String> {
    let months = months_inclusive(request.from, request.to)?;

    let mut reports = Vec::with_capacity(months.len());
    let mut violations = Vec::new();
    for month in &months {
        let repo_prs = crate::report::fetch_repo_prs(
            source,
            &request.owner,
            &request.repos,
            month.year,
            month.month,
        )
        .await
        .map_err(|e| format!("fetching merged PRs for {month}: {e}"))?;

        if request.strict {
            violations.extend(crate::strict::check_strict(&repo_prs));
        }

        reports.push(crate::report::build_report(
            repo_prs,
            &request.owner,
            month.year,
            month.month,
            request.monthly_hours,
            request.scaling,
        )?);
    }

    let stdout = match request.output {
        OutputFormat::Json => crate::render::render_range_json(request.from, request.to, &reports),
        OutputFormat::Table => {
            crate::render::render_range_table(request.from, request.to, &reports)
        }
    };

    Ok(RunOutcome {
        stdout,
        violations,
        reports: months.into_iter().zip(reports).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakePrSource, make_pr};

    fn request(output: OutputFormat, strict: bool) -> RunRequest {
        RunRequest {
            owner: "como".to_string(),
            repos: vec!["alpha".to_string()],
            year: 2026,
            month: 3,
            monthly_hours: 360.0,
            scaling: ScalingSeries::Linear,
            output,
            strict,
        }
    }

    /// One contract-shaped PR (passes strict) and one effort-less PR
    /// (fails strict and lands in unallocated_prs).
    fn mixed_source() -> FakePrSource {
        FakePrSource::new().with_repo(
            "como",
            "alpha",
            vec![
                make_pr(1, "Good work", None, &["effort:3-average", "adr:ADR-0001"]),
                make_pr(2, "Mystery work", None, &["experiment"]),
            ],
        )
    }

    fn contract_source() -> FakePrSource {
        FakePrSource::new().with_repo(
            "como",
            "alpha",
            vec![make_pr(
                1,
                "Good work",
                None,
                &["effort:3-average", "adr:ADR-0001", "conduit:task"],
            )],
        )
    }

    #[tokio::test]
    async fn json_mode_emits_the_canonical_report() {
        let outcome = run_with_source(&mixed_source(), &request(OutputFormat::Json, false))
            .await
            .unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&outcome.stdout).expect("stdout is pure JSON in -o json mode");
        assert_eq!(value["month"], "March");
        assert_eq!(value["year"], 2026);
        assert_eq!(value["adr_totals"]["ADR-0001"], 360.0);
        assert_eq!(value["unallocated_prs"][0][1], 2);
    }

    #[tokio::test]
    async fn table_mode_emits_the_compact_table() {
        let outcome = run_with_source(&mixed_source(), &request(OutputFormat::Table, false))
            .await
            .unwrap();
        assert!(outcome.stdout.contains("March 2026"));
        assert!(outcome.stdout.contains("#1"));
        assert!(serde_json::from_str::<serde_json::Value>(&outcome.stdout).is_err());
    }

    #[tokio::test]
    async fn without_strict_violations_are_not_collected() {
        let outcome = run_with_source(&mixed_source(), &request(OutputFormat::Json, false))
            .await
            .unwrap();
        assert!(outcome.violations.is_empty());
    }

    #[tokio::test]
    async fn strict_passes_contract_shaped_prs() {
        // The referee ruling's positive side: effort + adr:* (no category
        // label anywhere) exits clean.
        let outcome = run_with_source(&contract_source(), &request(OutputFormat::Json, true))
            .await
            .unwrap();
        assert!(outcome.violations.is_empty(), "{:?}", outcome.violations);
    }

    #[tokio::test]
    async fn strict_reports_the_offending_prs_but_still_renders() {
        let outcome = run_with_source(&mixed_source(), &request(OutputFormat::Json, true))
            .await
            .unwrap();

        assert_eq!(outcome.violations.len(), 1);
        assert_eq!(outcome.violations[0].pr_number, 2);
        assert_eq!(outcome.violations[0].pr_title, "Mystery work");
        // The report artifact stays inspectable alongside the failure.
        assert!(serde_json::from_str::<serde_json::Value>(&outcome.stdout).is_ok());
    }

    #[tokio::test]
    async fn source_errors_surface_as_run_errors() {
        let err = run_with_source(
            &FakePrSource::failing_with("boom"),
            &request(OutputFormat::Json, false),
        )
        .await
        .unwrap_err();
        assert!(err.contains("boom"));
    }

    // --- the multi-month range pipeline (ADR-0007) ---

    fn ym(year: u32, month: u32) -> YearMonth {
        YearMonth { year, month }
    }

    fn range_request(
        from: YearMonth,
        to: YearMonth,
        output: OutputFormat,
        strict: bool,
    ) -> RangeRequest {
        RangeRequest {
            owner: "como".to_string(),
            repos: vec!["alpha".to_string()],
            from,
            to,
            monthly_hours: 360.0,
            scaling: ScalingSeries::Linear,
            output,
            strict,
        }
    }

    /// December 2025 and January 2026 scripted with contract-shaped PRs —
    /// ADR-0001 appears in both months, so the rollup must sum across the
    /// year boundary.
    fn year_boundary_source() -> FakePrSource {
        FakePrSource::new()
            .with_repo_month(
                "como",
                "alpha",
                2025,
                12,
                vec![make_pr(
                    1,
                    "December work",
                    None,
                    &["effort:3-average", "adr:ADR-0001"],
                )],
            )
            .with_repo_month(
                "como",
                "alpha",
                2026,
                1,
                vec![
                    make_pr(
                        2,
                        "January work",
                        None,
                        &["effort:1-super-quick", "adr:ADR-0001"],
                    ),
                    make_pr(
                        3,
                        "More January",
                        None,
                        &["effort:1-super-quick", "adr:ADR-0002"],
                    ),
                ],
            )
    }

    #[tokio::test]
    async fn range_emits_one_canonical_report_per_month_across_the_year_boundary() {
        let source = year_boundary_source();
        let outcome = run_range_with_source(
            &source,
            &range_request(ym(2025, 12), ym(2026, 1), OutputFormat::Json, false),
        )
        .await
        .unwrap();

        let envelope: serde_json::Value =
            serde_json::from_str(&outcome.stdout).expect("stdout is pure JSON");
        assert_eq!(envelope["from"], "2025-12");
        assert_eq!(envelope["to"], "2026-01");

        let reports = envelope["reports"].as_array().unwrap();
        assert_eq!(reports.len(), 2, "one report per month");
        assert_eq!(reports[0]["month"], "December");
        assert_eq!(reports[0]["year"], 2025);
        assert_eq!(reports[1]["month"], "January");
        assert_eq!(reports[1]["year"], 2026);

        // Cross-month rollup: ADR-0001 sums across the boundary
        // (360 in December + 180 in January), ADR-0002 is January-only.
        assert_eq!(envelope["adr_totals"]["ADR-0001"], 540.0);
        assert_eq!(envelope["adr_totals"]["ADR-0002"], 180.0);
    }

    #[tokio::test]
    async fn range_months_are_the_unchanged_single_month_reports() {
        // Per-month compatibility: each envelope element is exactly what
        // single-month mode emits for that month (the ADR-0004 schema,
        // untouched).
        let source = year_boundary_source();
        let outcome = run_range_with_source(
            &source,
            &range_request(ym(2025, 12), ym(2026, 1), OutputFormat::Json, false),
        )
        .await
        .unwrap();
        let envelope: serde_json::Value = serde_json::from_str(&outcome.stdout).unwrap();

        for (index, (year, month)) in [(2025, 12), (2026, 1)].into_iter().enumerate() {
            let single = run_with_source(
                &source,
                &RunRequest {
                    year,
                    month,
                    ..request(OutputFormat::Json, false)
                },
            )
            .await
            .unwrap();
            let single: serde_json::Value = serde_json::from_str(&single.stdout).unwrap();
            assert_eq!(
                envelope["reports"][index], single,
                "{year}-{month:02} differs between range and single-month mode"
            );
        }
    }

    #[tokio::test]
    async fn range_renders_empty_months_as_empty_reports_not_holes() {
        // 2025-12 .. 2026-02: only the boundary months have PRs; February
        // must still appear (catching up a quarter includes quiet months).
        let outcome = run_range_with_source(
            &year_boundary_source(),
            &range_request(ym(2025, 12), ym(2026, 2), OutputFormat::Json, false),
        )
        .await
        .unwrap();

        let envelope: serde_json::Value = serde_json::from_str(&outcome.stdout).unwrap();
        let reports = envelope["reports"].as_array().unwrap();
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[2]["month"], "February");
        assert_eq!(reports[2]["allocations"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn range_strict_checks_each_month_and_aggregates_violations() {
        // A violator in January only; December stays clean. Strict is
        // per-month: the violation is found in its month, the clean month
        // contributes none, and the report is still rendered.
        let source = FakePrSource::new()
            .with_repo_month(
                "como",
                "alpha",
                2025,
                12,
                vec![make_pr(
                    1,
                    "Good",
                    None,
                    &["effort:3-average", "adr:ADR-0001"],
                )],
            )
            .with_repo_month(
                "como",
                "alpha",
                2026,
                1,
                vec![make_pr(2, "Mystery work", None, &["experiment"])],
            );

        let outcome = run_range_with_source(
            &source,
            &range_request(ym(2025, 12), ym(2026, 1), OutputFormat::Json, true),
        )
        .await
        .unwrap();

        assert_eq!(outcome.violations.len(), 1);
        assert_eq!(outcome.violations[0].pr_number, 2);
        assert!(serde_json::from_str::<serde_json::Value>(&outcome.stdout).is_ok());
    }

    #[tokio::test]
    async fn range_table_mode_emits_the_sectioned_table() {
        let outcome = run_range_with_source(
            &year_boundary_source(),
            &range_request(ym(2025, 12), ym(2026, 1), OutputFormat::Table, false),
        )
        .await
        .unwrap();

        assert!(outcome.stdout.contains("December 2025"));
        assert!(outcome.stdout.contains("January 2026"));
        assert!(outcome.stdout.contains("ADR totals across the range"));
        assert!(serde_json::from_str::<serde_json::Value>(&outcome.stdout).is_err());
    }

    #[tokio::test]
    async fn inverted_range_is_an_error() {
        let err = run_range_with_source(
            &year_boundary_source(),
            &range_request(ym(2026, 1), ym(2025, 12), OutputFormat::Json, false),
        )
        .await
        .unwrap_err();
        assert!(err.contains("must run forward"), "{err}");
    }

    #[tokio::test]
    async fn range_source_errors_name_the_failing_month() {
        let err = run_range_with_source(
            &FakePrSource::failing_with("boom"),
            &range_request(ym(2025, 12), ym(2026, 1), OutputFormat::Json, false),
        )
        .await
        .unwrap_err();
        assert!(err.contains("boom") && err.contains("2025-12"), "{err}");
    }
}

//! The `tuesday-report` flag surface.
//!
//! The binary is named `tuesday-report` rather than `tuesday` because the
//! web head's executable owns the `tuesday` name for self-host-path reasons
//! (ADR-0008; see crates/tuesday-web/Cargo.toml and this crate's manifest).

use crate::window::YearMonth;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use tuesday_core::ScalingSeries;

/// Which forge provider backs the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceKind {
    Github,
    Gitea,
}

/// Output mode: the compact human table, or the canonical MonthlyReport JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

/// CLI-facing names for the core scaling series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScalingArg {
    Linear,
    Doubling,
    Fibonacci,
    Exponential,
    TShirtSizes,
    Square,
}

impl From<ScalingArg> for ScalingSeries {
    fn from(arg: ScalingArg) -> Self {
        match arg {
            ScalingArg::Linear => ScalingSeries::Linear,
            ScalingArg::Doubling => ScalingSeries::Doubling,
            ScalingArg::Fibonacci => ScalingSeries::Fibonacci,
            ScalingArg::Exponential => ScalingSeries::Exponential,
            ScalingArg::TShirtSizes => ScalingSeries::TShirtSizes,
            ScalingArg::Square => ScalingSeries::Square,
        }
    }
}

/// Headless monthly effort report: drives a read-only PR source through
/// tuesday-core's calculator and emits the canonical MonthlyReport.
#[derive(Debug, Parser)]
#[command(
    name = "tuesday-report",
    version,
    about = "Headless tuesday: one canonical MonthlyReport from merged PRs (ADR-0004)",
    long_about = "Headless tuesday: fetches merged PRs from a forge, runs the effort\n\
        calculator, and emits the canonical serde MonthlyReport — the same schema\n\
        as the web head's JSON export, including the adr_totals rollup.\n\n\
        The window is one month (--year/--month) or an inclusive multi-month\n\
        range (--from/--to, ADR-0007): the range emits one unchanged canonical\n\
        MonthlyReport per month plus a cross-month adr_totals rollup.\n\n\
        With -o json the report is pure JSON on stdout (logs go to stderr).\n\
        With --strict, every merged PR must carry exactly one effort:N-* label\n\
        AND (a category label OR an adr:* label); violations are listed on\n\
        stderr and the exit code is nonzero (ADR-0005 allocation ruling). In\n\
        range mode the contract is checked month by month."
)]
pub struct Args {
    /// Forge provider to read merged PRs from
    #[arg(long, value_enum)]
    pub source: SourceKind,

    /// Repository owner (organization or user)
    #[arg(long)]
    pub owner: String,

    /// Repository name; repeat the flag for multiple repositories
    #[arg(long = "repo", value_name = "REPO", required = true)]
    pub repos: Vec<String>,

    /// Report year, e.g. 2026 (single-month mode; pair with --month)
    #[arg(
        long,
        requires = "month",
        required_unless_present = "from",
        conflicts_with_all = ["from", "to"]
    )]
    pub year: Option<u32>,

    /// Report month, 1-12 (single-month mode; always pass it explicitly:
    /// the month-boundary trap is real)
    #[arg(
        long,
        value_parser = clap::value_parser!(u32).range(1..=12),
        requires = "year",
        required_unless_present = "from",
        conflicts_with_all = ["from", "to"]
    )]
    pub month: Option<u32>,

    /// First month of a multi-month range, inclusive (range mode, ADR-0007;
    /// pair with --to)
    #[arg(long, value_name = "YYYY-MM", requires = "to")]
    pub from: Option<YearMonth>,

    /// Last month of a multi-month range, inclusive
    #[arg(long, value_name = "YYYY-MM", requires = "from")]
    pub to: Option<YearMonth>,

    /// Total team hours to allocate across the month's merged PRs
    #[arg(long, default_value_t = 360.0)]
    pub monthly_hours: f64,

    /// Forge base URL (gitea only; defaults to conduit's dogfood forge at
    /// http://localhost:3000. GitHub's API base is fixed.)
    #[arg(long)]
    pub base_url: Option<String>,

    /// Read the API token from this file (overrides GITHUB_TOKEN /
    /// TUESDAY_GITEA_TOKEN; gitea also falls back to the documented
    /// ${COMO_CONDUIT_DIR:-../conduit}/.secrets/reviewer.token)
    #[arg(long, value_name = "PATH")]
    pub token_file: Option<PathBuf>,

    /// Output format: compact table for humans, json for the canonical report
    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,

    /// Effort-score scaling series for the hour split
    #[arg(long, value_enum, default_value_t = ScalingArg::Linear)]
    pub scaling: ScalingArg,

    /// Enforce the dogfood contract: exit nonzero unless every merged PR has
    /// exactly one effort label and a category or adr:* label
    #[arg(long)]
    pub strict: bool,

    /// Also write each month's report as a measure-report typed page into
    /// this KB space (the directory holding wiki.toml), at
    /// wiki/measures/<owner>-<YYYY-MM>.md. Deterministic — same forge data
    /// and arguments, byte-identical page. Admission (llm-wiki ingest) and
    /// committing stay with the caller; with --strict, pages are skipped
    /// when violations are found (a contract-violating month doesn't enter
    /// the record).
    #[arg(long, value_name = "SPACE_DIR")]
    pub kb: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, clap::Error> {
        Args::try_parse_from(std::iter::once("tuesday-report").chain(args.iter().copied()))
    }

    const MINIMAL: &[&str] = &[
        "--source",
        "gitea",
        "--owner",
        "como",
        "--repo",
        "conduit-dogfood",
        "--year",
        "2026",
        "--month",
        "6",
    ];

    #[test]
    fn minimal_surface_parses_with_documented_defaults() {
        let args = parse(MINIMAL).unwrap();
        assert_eq!(args.source, SourceKind::Gitea);
        assert_eq!(args.owner, "como");
        assert_eq!(args.repos, vec!["conduit-dogfood".to_string()]);
        assert_eq!(args.year, Some(2026));
        assert_eq!(args.month, Some(6));
        assert_eq!(args.from, None);
        assert_eq!(args.to, None);
        assert_eq!(args.monthly_hours, 360.0);
        assert_eq!(args.base_url, None);
        assert_eq!(args.token_file, None);
        assert_eq!(args.output, OutputFormat::Table);
        assert_eq!(args.scaling, ScalingArg::Linear);
        assert!(!args.strict);
    }

    /// MINIMAL with the single-month window swapped for a --from/--to range.
    fn range_args() -> Vec<&'static str> {
        let mut args: Vec<&str> = MINIMAL[..6].to_vec();
        args.extend_from_slice(&["--from", "2025-11", "--to", "2026-02"]);
        args
    }

    #[test]
    fn range_surface_parses_without_year_or_month() {
        let args = parse(&range_args()).unwrap();
        assert_eq!(args.year, None);
        assert_eq!(args.month, None);
        assert_eq!(
            args.from,
            Some(YearMonth {
                year: 2025,
                month: 11
            })
        );
        assert_eq!(
            args.to,
            Some(YearMonth {
                year: 2026,
                month: 2
            })
        );
    }

    #[test]
    fn from_and_to_require_each_other() {
        for flag in [&["--from", "2025-11"][..], &["--to", "2026-02"][..]] {
            let mut args: Vec<&str> = MINIMAL[..6].to_vec();
            args.extend_from_slice(flag);
            assert!(parse(&args).is_err(), "{flag:?} alone should be rejected");
        }
    }

    #[test]
    fn range_conflicts_with_year_and_month() {
        // Both windows at once is ambiguous, not a silent preference.
        let mut args: Vec<&str> = MINIMAL.to_vec();
        args.extend_from_slice(&["--from", "2025-11", "--to", "2026-02"]);
        assert!(parse(&args).is_err());
    }

    #[test]
    fn some_window_is_required() {
        // Neither --year/--month nor --from/--to: a usage error.
        assert!(parse(&MINIMAL[..6]).is_err());
    }

    #[test]
    fn malformed_range_months_are_usage_errors() {
        for bad in ["2026", "2026-13", "2026-3", "26-03"] {
            let mut args: Vec<&str> = MINIMAL[..6].to_vec();
            args.extend_from_slice(&["--from", bad, "--to", "2026-06"]);
            assert!(parse(&args).is_err(), "--from {bad} should be rejected");
        }
    }

    #[test]
    fn repo_flag_repeats_for_multiple_repositories() {
        let mut full: Vec<&str> = MINIMAL.to_vec();
        full.extend_from_slice(&["--repo", "second"]);
        let args = parse(&full).unwrap();
        assert_eq!(
            args.repos,
            vec!["conduit-dogfood".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn full_surface_parses() {
        let mut full: Vec<&str> = MINIMAL.to_vec();
        full.extend_from_slice(&[
            "--monthly-hours",
            "160",
            "--base-url",
            "http://localhost:3000",
            "--token-file",
            "/tmp/token",
            "-o",
            "json",
            "--scaling",
            "fibonacci",
            "--strict",
        ]);
        let args = parse(&full).unwrap();
        assert_eq!(args.monthly_hours, 160.0);
        assert_eq!(args.base_url, Some("http://localhost:3000".to_string()));
        assert_eq!(args.token_file, Some(PathBuf::from("/tmp/token")));
        assert_eq!(args.output, OutputFormat::Json);
        assert_eq!(args.scaling, ScalingArg::Fibonacci);
        assert!(args.strict);
    }

    #[test]
    fn month_outside_1_to_12_is_rejected() {
        for bad in ["0", "13"] {
            let mut args: Vec<&str> = MINIMAL[..8].to_vec();
            args.extend_from_slice(&["--month", bad]);
            assert!(parse(&args).is_err(), "month {bad} should be rejected");
        }
    }

    #[test]
    fn source_owner_repo_year_month_are_required() {
        // Dropping any required pair must fail parsing.
        for skip in (0..MINIMAL.len()).step_by(2) {
            let args: Vec<&str> = MINIMAL
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != skip && *i != skip + 1)
                .map(|(_, a)| *a)
                .collect();
            assert!(
                parse(&args).is_err(),
                "dropping {} should fail",
                MINIMAL[skip]
            );
        }
    }

    #[test]
    fn unknown_source_is_rejected() {
        let mut args: Vec<&str> = MINIMAL[2..].to_vec();
        args.extend_from_slice(&["--source", "bitbucket"]);
        assert!(parse(&args).is_err());
    }

    #[test]
    fn every_scaling_series_is_accepted() {
        for (name, series) in [
            ("linear", ScalingSeries::Linear),
            ("doubling", ScalingSeries::Doubling),
            ("fibonacci", ScalingSeries::Fibonacci),
            ("exponential", ScalingSeries::Exponential),
            ("t-shirt-sizes", ScalingSeries::TShirtSizes),
            ("square", ScalingSeries::Square),
        ] {
            let mut full: Vec<&str> = MINIMAL.to_vec();
            full.extend_from_slice(&["--scaling", name]);
            let args = parse(&full).unwrap_or_else(|e| panic!("--scaling {name}: {e}"));
            assert_eq!(ScalingSeries::from(args.scaling), series);
        }
    }
}

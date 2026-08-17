//! `tuesday-report`: the headless Measure head.
//!
//! Bare invocation is the KB lane: price the month's decisions from
//! door-stamped work-item pages over the appliance transport, land the
//! `measure-report` page, print the summary. The `forge` subcommand is the
//! demoted legacy lane over merged PRs (ADR-0004).
//!
//! Named `tuesday-report` and not `tuesday` because the web head's binary
//! owns the `tuesday` name — the generic self-host path (Containerfile,
//! scripts/build-static-release.sh, dx's `target/dx/tuesday` output dir —
//! ADR-0008) points at it; see crates/tuesday-web/Cargo.toml.
//!
//! Exit codes: 0 = report emitted (and, with the forge lane's `--strict`,
//! every merged PR satisfies the ADR-0005 contract); 1 = runtime failure
//! or strict violations (listed on stderr, the report still printed);
//! 2 = usage error (clap). Stdout carries only the report — every
//! diagnostic goes to stderr, so `-o json` output is pipeable.

use clap::Parser;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;
use tuesday_cli::cli::{Args, ForgeArgs, ForgeCmd, OutputFormat, SourceKind};
use tuesday_cli::run::{
    RangeRequest, RunOutcome, RunRequest, run_range_with_source, run_with_source,
};
use tuesday_cli::token::resolve_token;
use tuesday_cli::window::YearMonth;
use tuesday_core::{GitHubSource, GiteaSource, PrSource};

/// The old dogfood forge default for `forge --source gitea`.
const DEFAULT_GITEA_BASE_URL: &str = "http://localhost:3000";

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    // Core's tracing diagnostics go to stderr so stdout stays pure.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    match args.forge {
        None => {
            let period = args.period.unwrap_or_else(current_month);
            match tuesday_cli::measure::run(period, args.output == OutputFormat::Json).await {
                Ok(stdout) => {
                    print!("{stdout}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(ForgeCmd::Forge(forge)) => run_forge(*forge).await,
    }
}

fn current_month() -> YearMonth {
    use chrono::Datelike;
    let now = chrono::Utc::now();
    YearMonth {
        year: now.year() as u32,
        month: now.month(),
    }
}

async fn run_forge(args: ForgeArgs) -> ExitCode {
    match run(args).await {
        Ok(outcome) => {
            print!("{}", outcome.stdout);
            if !outcome.stdout.ends_with('\n') {
                println!();
            }
            if outcome.violations.is_empty() {
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "strict mode: {} merged PR(s) violate the contract \
                     (exactly one effort:N-* label AND a category or adr:* label):",
                    outcome.violations.len()
                );
                for violation in &outcome.violations {
                    eprintln!(
                        "  {} #{} {:?}: {}",
                        violation.repository,
                        violation.pr_number,
                        violation.pr_title,
                        violation.problems.join("; "),
                    );
                }
                ExitCode::FAILURE
            }
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: ForgeArgs) -> Result<RunOutcome, String> {
    if args.source == SourceKind::Github && args.base_url.is_some() {
        return Err(
            "--base-url applies only to --source gitea (GitHub's API base is fixed)".to_string(),
        );
    }

    let token = resolve_token(args.source, args.token_file.as_deref())?;

    match args.source {
        SourceKind::Github => dispatch(&GitHubSource::new(token), &args).await,
        SourceKind::Gitea => {
            let base_url = args
                .base_url
                .clone()
                .unwrap_or_else(|| DEFAULT_GITEA_BASE_URL.to_string());
            if token.is_none() {
                eprintln!(
                    "note: no gitea token (--token-file, TUESDAY_GITEA_TOKEN, or \
                     ${{COMO_CONDUIT_DIR:-../conduit}}/.secrets/reviewer.token); \
                     reading anonymously"
                );
            }
            dispatch(&GiteaSource::new(base_url, token), &args).await
        }
    }
}

/// Pick the window mode: `--from/--to` drives the multi-month range
/// pipeline (ADR-0007); otherwise `--year/--month` drives the single-month
/// pipeline. Clap guarantees exactly one of the two windows is present.
async fn dispatch<S: PrSource>(source: &S, args: &ForgeArgs) -> Result<RunOutcome, String> {
    if let (Some(from), Some(to)) = (args.from, args.to) {
        let request = RangeRequest {
            owner: args.owner.clone(),
            repos: args.repos.clone(),
            from,
            to,
            monthly_hours: args.monthly_hours,
            scaling: args.scaling.into(),
            output: args.output,
            strict: args.strict,
        };
        run_range_with_source(source, &request).await
    } else {
        let request = RunRequest {
            owner: args.owner.clone(),
            repos: args.repos.clone(),
            year: args.year.expect("clap: --year is required without --from"),
            month: args
                .month
                .expect("clap: --month is required without --from"),
            monthly_hours: args.monthly_hours,
            scaling: args.scaling.into(),
            output: args.output,
            strict: args.strict,
        };
        run_with_source(source, &request).await
    }
}

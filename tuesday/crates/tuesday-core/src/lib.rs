//! tuesday-core: the forge-neutral effort-measurement domain.
//!
//! Houses the effort calculator and report model, the neutral [`MergedPr`]
//! domain type, the read-only [`PrSource`] ingestion trait (ADR-0003), and
//! the GitHub and Gitea providers. Compiles for both native and
//! `wasm32-unknown-unknown` (ADR-0002: async reqwest, no tokio), so both
//! heads — the Dioxus web app and the future headless CLI — consume the
//! same report engine (ADR-0001).

pub mod calculator;
pub mod domain;
pub mod gitea;
pub mod github;
pub mod report;
pub mod source;

pub use calculator::{EffortCalculator, EffortScore, MonthlyReport, ScalingSeries, TimeAllocation};
pub use domain::MergedPr;
pub use gitea::GiteaSource;
pub use github::GitHubSource;
pub use report::{
    DEFAULT_GITEA_BASE_URL, ForgeSource, ReportConfig, generate_report,
    generate_report_with_source, month_name,
};
pub use source::{PrSource, SourceError, SourceKind};

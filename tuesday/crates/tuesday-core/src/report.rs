//! The config-driven report pipeline shared by the heads: a [`ReportConfig`]
//! in, the canonical [`MonthlyReport`] out, with the provider chosen through
//! the read-only [`PrSource`] seam (ADR-0003) from the config's source kind —
//! no head hardcodes a forge.

use crate::calculator::{EffortCalculator, MonthlyReport, ScalingSeries};
use crate::domain::MergedPr;
use crate::gitea::GiteaSource;
use crate::github::GitHubSource;
use crate::source::{PrSource, SourceError, SourceKind};
use chrono::Datelike;
use serde::{Deserialize, Serialize};

/// conduit's dogfood forge — the documented default Gitea base URL
/// (mirrors the CLI's `--base-url` default).
pub const DEFAULT_GITEA_BASE_URL: &str = "http://localhost:3000";

/// Everything a head collects to ask for one monthly report. Also the
/// request body of the web head's `POST /api/export_report` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportConfig {
    /// Which forge provider to read merged PRs from. Defaults to GitHub
    /// when absent, so pre-source-kind request bodies keep working.
    #[serde(default)]
    pub source: SourceKind,
    /// Forge base URL — Gitea only (GitHub's API base is fixed). Defaults
    /// to [`DEFAULT_GITEA_BASE_URL`] when absent.
    #[serde(default)]
    pub base_url: Option<String>,
    /// API token. Required for GitHub; optional for Gitea (anonymous read).
    pub token: String,
    pub monthly_hours: f64,
    pub repositories: Vec<String>,
    pub organization: String,
    pub year: u32,
    pub month: u32,
    pub scaling_series: ScalingSeries,
}

impl Default for ReportConfig {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            source: SourceKind::default(),
            base_url: None,
            token: String::new(),
            monthly_hours: 360.0,
            repositories: Vec::new(),
            organization: String::new(),
            year: now.year() as u32,
            month: now.month(),
            scaling_series: ScalingSeries::default(),
        }
    }
}

/// English month name for a 1-based month number.
pub fn month_name(month: u32) -> Result<&'static str, String> {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    NAMES
        .get(month.wrapping_sub(1) as usize)
        .copied()
        .ok_or_else(|| format!("invalid month {month}: expected 1-12"))
}

/// The concrete provider selected at runtime from a [`SourceKind`] — the
/// one place a `ReportConfig` becomes a [`PrSource`].
pub enum ForgeSource {
    Github(GitHubSource),
    Gitea(GiteaSource),
}

impl ForgeSource {
    /// Build the provider the config asks for, enforcing the per-forge
    /// rules: GitHub requires a token and has a fixed API base; Gitea
    /// defaults to the dogfood forge URL and reads anonymously without a
    /// token.
    pub fn from_config(cfg: &ReportConfig) -> Result<Self, String> {
        match cfg.source {
            SourceKind::Github => {
                if cfg.base_url.is_some() {
                    return Err(
                        "base_url applies only to the gitea source (GitHub's API base is fixed)"
                            .to_string(),
                    );
                }
                if cfg.token.is_empty() {
                    return Err("GitHub token required for fetching PRs. \
                         Please provide a token in the configuration."
                        .to_string());
                }
                Ok(Self::Github(GitHubSource::new(Some(cfg.token.clone()))))
            }
            SourceKind::Gitea => {
                let base_url = cfg
                    .base_url
                    .clone()
                    .unwrap_or_else(|| DEFAULT_GITEA_BASE_URL.to_string());
                let token = (!cfg.token.is_empty()).then(|| cfg.token.clone());
                Ok(Self::Gitea(GiteaSource::new(base_url, token)))
            }
        }
    }
}

impl PrSource for ForgeSource {
    async fn list_orgs(&self) -> Result<Vec<String>, SourceError> {
        match self {
            Self::Github(source) => source.list_orgs().await,
            Self::Gitea(source) => source.list_orgs().await,
        }
    }

    async fn list_repos(&self, org: &str) -> Result<Vec<String>, SourceError> {
        match self {
            Self::Github(source) => source.list_repos(org).await,
            Self::Gitea(source) => source.list_repos(org).await,
        }
    }

    async fn fetch_merged_prs(
        &self,
        owner: &str,
        repo: &str,
        year: u32,
        month: u32,
    ) -> Result<Vec<MergedPr>, SourceError> {
        match self {
            Self::Github(source) => source.fetch_merged_prs(owner, repo, year, month).await,
            Self::Gitea(source) => source.fetch_merged_prs(owner, repo, year, month).await,
        }
    }
}

/// Generate a report from configuration: build the configured provider and
/// drive it through [`generate_report_with_source`].
pub async fn generate_report(cfg: ReportConfig) -> Result<MonthlyReport, String> {
    tracing::debug!("Starting report generation");
    tracing::debug!(
        "Config - Source: {:?}, Org: {}, Repos: {:?}, Token length: {}",
        cfg.source,
        cfg.organization,
        cfg.repositories,
        cfg.token.len(),
    );
    tracing::debug!("Config - Year: {}, Month: {}", cfg.year, cfg.month);

    let source = ForgeSource::from_config(&cfg)?;
    generate_report_with_source(&source, &cfg).await
}

/// Fetch the month's merged PRs for every configured repository (in
/// parallel) through any [`PrSource`], then run the effort calculator.
/// A repository whose fetch fails contributes an empty PR list rather than
/// aborting the report (matching the long-standing web-head behavior); the
/// failure is logged.
pub async fn generate_report_with_source<S: PrSource>(
    source: &S,
    cfg: &ReportConfig,
) -> Result<MonthlyReport, String> {
    if cfg.organization.is_empty() || cfg.repositories.is_empty() {
        return Err("Organization or repositories are empty".to_string());
    }
    let month_name = month_name(cfg.month)?;

    tracing::debug!(
        "Fetching PRs from {} repositories...",
        cfg.repositories.len()
    );
    let fetches = cfg.repositories.iter().map(|repo| async move {
        match source
            .fetch_merged_prs(&cfg.organization, repo, cfg.year, cfg.month)
            .await
        {
            Ok(prs) => {
                tracing::info!("Successfully fetched {} PRs from {}", prs.len(), repo);
                (repo.clone(), prs)
            }
            Err(e) => {
                tracing::error!("Failed to fetch PRs from {}: {}", repo, e);
                (repo.clone(), Vec::new())
            }
        }
    });
    let repo_prs: Vec<(String, Vec<MergedPr>)> = futures::future::join_all(fetches).await;

    for (repo_name, prs) in &repo_prs {
        if prs.is_empty() {
            tracing::warn!(
                "No PRs found for repository {} in the specified period!",
                repo_name
            );
        }
    }

    let calculator = EffortCalculator::new(cfg.monthly_hours, cfg.scaling_series);
    tracing::debug!("Calculating report for {} {}", month_name, cfg.year);
    let report = calculator.calculate_report(
        repo_prs,
        month_name.to_string(),
        cfg.year,
        cfg.organization.clone(),
    );
    tracing::debug!(
        "Report calculated: {} allocations",
        report.allocations.len()
    );

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_pr(number: u64, labels: &[&str]) -> MergedPr {
        MergedPr {
            number,
            title: format!("PR {number}"),
            body: None,
            url: format!("http://forge.example/como/alpha/pulls/{number}"),
            merged_at: chrono::Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap(),
            labels: labels.iter().map(|name| name.to_string()).collect(),
        }
    }

    /// In-memory source: per-repo PR lists, with designated failing repos.
    struct FakeSource {
        repos: Vec<(String, Vec<MergedPr>)>,
        failing: Vec<String>,
    }

    impl PrSource for FakeSource {
        async fn list_orgs(&self) -> Result<Vec<String>, SourceError> {
            Ok(vec!["como".to_string()])
        }

        async fn list_repos(&self, _org: &str) -> Result<Vec<String>, SourceError> {
            Ok(self.repos.iter().map(|(name, _)| name.clone()).collect())
        }

        async fn fetch_merged_prs(
            &self,
            _owner: &str,
            repo: &str,
            _year: u32,
            _month: u32,
        ) -> Result<Vec<MergedPr>, SourceError> {
            if self.failing.iter().any(|name| name == repo) {
                return Err("forge unreachable".into());
            }
            Ok(self
                .repos
                .iter()
                .find(|(name, _)| name == repo)
                .map(|(_, prs)| prs.clone())
                .unwrap_or_default())
        }
    }

    fn config_for(repos: &[&str]) -> ReportConfig {
        ReportConfig {
            source: SourceKind::Gitea,
            token: "tok".to_string(),
            organization: "como".to_string(),
            repositories: repos.iter().map(|name| name.to_string()).collect(),
            year: 2026,
            month: 1,
            monthly_hours: 360.0,
            ..ReportConfig::default()
        }
    }

    // --- ReportConfig shape (export endpoint request contract) ---

    #[test]
    fn report_config_deserializes_documented_request_shape() {
        // The exact JSON config documented for POST /api/export_report.
        let body = r#"{"source":"gitea","base_url":"http://localhost:3000","token":"<token>","monthly_hours":160.0,"repositories":["my-repo"],"organization":"my-org","year":2026,"month":5,"scaling_series":"Linear"}"#;
        let cfg: ReportConfig = serde_json::from_str(body).unwrap();
        assert_eq!(cfg.source, SourceKind::Gitea);
        assert_eq!(cfg.base_url, Some("http://localhost:3000".to_string()));
        assert_eq!(cfg.token, "<token>");
        assert_eq!(cfg.monthly_hours, 160.0);
        assert_eq!(cfg.repositories, vec!["my-repo".to_string()]);
        assert_eq!(cfg.organization, "my-org");
        assert_eq!(cfg.year, 2026);
        assert_eq!(cfg.month, 5);
        assert_eq!(cfg.scaling_series, ScalingSeries::Linear);
    }

    #[test]
    fn source_and_base_url_default_for_pre_source_kind_requests() {
        // A request body predating the source-kind field still parses and
        // targets GitHub.
        let body = r#"{"token":"<token>","monthly_hours":360.0,"repositories":["my-repo"],"organization":"my-org","year":2026,"month":5,"scaling_series":"Linear"}"#;
        let cfg: ReportConfig = serde_json::from_str(body).unwrap();
        assert_eq!(cfg.source, SourceKind::Github);
        assert_eq!(cfg.base_url, None);
    }

    #[test]
    fn default_config_targets_github_with_the_360_hour_budget() {
        let cfg = ReportConfig::default();
        assert_eq!(cfg.source, SourceKind::Github);
        assert_eq!(cfg.base_url, None);
        assert!(cfg.token.is_empty());
        assert_eq!(cfg.monthly_hours, 360.0);
        assert_eq!(cfg.scaling_series, ScalingSeries::Linear);
    }

    #[test]
    fn source_kind_serde_form_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&SourceKind::Github).unwrap(),
            r#""github""#
        );
        assert_eq!(
            serde_json::from_str::<SourceKind>(r#""gitea""#).unwrap(),
            SourceKind::Gitea
        );
    }

    // --- ForgeSource::from_config (the dispatch rules) ---

    #[test]
    fn github_config_builds_the_github_provider() {
        let cfg = ReportConfig {
            source: SourceKind::Github,
            token: "tok".to_string(),
            ..ReportConfig::default()
        };
        assert!(matches!(
            ForgeSource::from_config(&cfg),
            Ok(ForgeSource::Github(_))
        ));
    }

    #[test]
    fn gitea_config_builds_the_gitea_provider_with_or_without_token() {
        for token in ["tok", ""] {
            let cfg = ReportConfig {
                source: SourceKind::Gitea,
                token: token.to_string(),
                ..ReportConfig::default()
            };
            assert!(
                matches!(ForgeSource::from_config(&cfg), Ok(ForgeSource::Gitea(_))),
                "token {token:?}"
            );
        }
    }

    #[test]
    fn github_requires_a_token() {
        let cfg = ReportConfig {
            source: SourceKind::Github,
            ..ReportConfig::default()
        };
        let err = ForgeSource::from_config(&cfg).err().unwrap();
        assert!(err.contains("token"), "names the missing token: {err}");
    }

    #[test]
    fn base_url_is_rejected_for_github() {
        let cfg = ReportConfig {
            source: SourceKind::Github,
            token: "tok".to_string(),
            base_url: Some("http://localhost:3000".to_string()),
            ..ReportConfig::default()
        };
        let err = ForgeSource::from_config(&cfg).err().unwrap();
        assert!(
            err.contains("gitea"),
            "explains the flag is gitea-only: {err}"
        );
    }

    // --- generate_report_with_source (the seam-driven pipeline) ---

    #[test]
    fn fake_source_drives_the_calculator_through_the_seam() {
        let source = FakeSource {
            repos: vec![(
                "alpha".to_string(),
                vec![make_pr(7, &["effort:3-average", "adr:ADR-0001"])],
            )],
            failing: Vec::new(),
        };

        let report = futures::executor::block_on(generate_report_with_source(
            &source,
            &config_for(&["alpha"]),
        ))
        .unwrap();

        assert_eq!(report.month, "January");
        assert_eq!(report.organization, "como");
        assert_eq!(report.allocations[0].pr_number, 7);
        assert_eq!(report.adr_totals.get("ADR-0001"), Some(&360.0));
    }

    #[test]
    fn empty_organization_or_repositories_is_an_error() {
        let source = FakeSource {
            repos: Vec::new(),
            failing: Vec::new(),
        };

        let mut no_org = config_for(&["alpha"]);
        no_org.organization = String::new();
        let err = futures::executor::block_on(generate_report_with_source(&source, &no_org))
            .err()
            .unwrap();
        assert!(err.contains("empty"));

        let no_repos = config_for(&[]);
        let err = futures::executor::block_on(generate_report_with_source(&source, &no_repos))
            .err()
            .unwrap();
        assert!(err.contains("empty"));
    }

    #[test]
    fn invalid_month_is_an_error_not_a_panic() {
        let source = FakeSource {
            repos: Vec::new(),
            failing: Vec::new(),
        };
        let mut cfg = config_for(&["alpha"]);
        cfg.month = 13;
        let err = futures::executor::block_on(generate_report_with_source(&source, &cfg))
            .err()
            .unwrap();
        assert!(err.contains("invalid month"));
    }

    #[test]
    fn failing_repository_contributes_empty_not_abort() {
        // Long-standing web-head behavior: one unreachable repo must not
        // sink the whole report.
        let source = FakeSource {
            repos: vec![(
                "alpha".to_string(),
                vec![make_pr(7, &["effort:3-average", "adr:ADR-0001"])],
            )],
            failing: vec!["broken".to_string()],
        };

        let report = futures::executor::block_on(generate_report_with_source(
            &source,
            &config_for(&["alpha", "broken"]),
        ))
        .unwrap();

        assert_eq!(report.allocations.len(), 1);
        assert_eq!(report.allocations[0].pr_number, 7);
    }

    // --- month_name ---

    #[test]
    fn month_name_maps_the_twelve_months_and_rejects_others() {
        assert_eq!(month_name(1), Ok("January"));
        assert_eq!(month_name(6), Ok("June"));
        assert_eq!(month_name(12), Ok("December"));
        assert!(month_name(0).is_err());
        assert!(month_name(13).is_err());
    }
}

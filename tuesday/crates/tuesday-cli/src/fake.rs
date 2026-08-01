//! In-crate scripted [`PrSource`] for tests: no network, fully deterministic.

use std::collections::HashMap;
use tuesday_core::{MergedPr, PrSource, SourceError};

/// A scripted source keyed by `owner/repo`. Unknown repos error, mimicking
/// a forge 404, and an explicit error can be injected to test propagation.
/// Repos scripted with [`Self::with_repo`] answer every window with the
/// same PRs; month-scripted repos ([`Self::with_repo_month`]) answer the
/// scripted window with its PRs and every other window with an empty month,
/// the shape range tests need.
pub struct FakePrSource {
    repos: HashMap<String, Vec<MergedPr>>,
    monthly: HashMap<(String, u32, u32), Vec<MergedPr>>,
    error: Option<String>,
}

impl FakePrSource {
    pub fn new() -> Self {
        Self {
            repos: HashMap::new(),
            monthly: HashMap::new(),
            error: None,
        }
    }

    pub fn with_repo(mut self, owner: &str, repo: &str, prs: Vec<MergedPr>) -> Self {
        self.repos.insert(format!("{owner}/{repo}"), prs);
        self
    }

    /// Script one calendar month of a repo; unscripted months of the same
    /// repo are empty, not errors.
    pub fn with_repo_month(
        mut self,
        owner: &str,
        repo: &str,
        year: u32,
        month: u32,
        prs: Vec<MergedPr>,
    ) -> Self {
        self.monthly
            .insert((format!("{owner}/{repo}"), year, month), prs);
        self
    }

    pub fn failing_with(message: &str) -> Self {
        Self {
            repos: HashMap::new(),
            monthly: HashMap::new(),
            error: Some(message.to_string()),
        }
    }
}

impl PrSource for FakePrSource {
    async fn list_orgs(&self) -> Result<Vec<String>, SourceError> {
        Ok(Vec::new())
    }

    async fn list_repos(&self, _org: &str) -> Result<Vec<String>, SourceError> {
        Ok(Vec::new())
    }

    async fn fetch_merged_prs(
        &self,
        owner: &str,
        repo: &str,
        year: u32,
        month: u32,
    ) -> Result<Vec<MergedPr>, SourceError> {
        if let Some(message) = &self.error {
            return Err(message.clone().into());
        }
        let key = format!("{owner}/{repo}");
        if let Some(prs) = self.repos.get(&key) {
            return Ok(prs.clone());
        }
        if self.monthly.keys().any(|(name, _, _)| *name == key) {
            return Ok(self
                .monthly
                .get(&(key, year, month))
                .cloned()
                .unwrap_or_default());
        }
        Err(SourceError::from(format!(
            "scripted forge has no repo {key}"
        )))
    }
}

/// A contract-shaped merged PR builder for tests.
pub fn make_pr(number: u64, title: &str, body: Option<&str>, labels: &[&str]) -> MergedPr {
    MergedPr {
        number,
        title: title.to_string(),
        body: body.map(|b| b.to_string()),
        url: format!("http://forge.example/como/alpha/pulls/{number}"),
        merged_at: chrono::DateTime::parse_from_rfc3339("2026-03-15T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        labels: labels.iter().map(|l| l.to_string()).collect(),
    }
}

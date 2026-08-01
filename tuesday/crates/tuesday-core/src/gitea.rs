use crate::domain::MergedPr;
use crate::source::{PrSource, SourceError};
use chrono::{DateTime, TimeZone, Utc};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Page size for Gitea list endpoints. Gitea clamps `limit` to its
/// `api.MAX_RESPONSE_ITEMS` setting (default 50); requesting exactly that
/// default keeps the short-page stop sound: a page shorter than the
/// requested limit is the last page. Verified against gitea/gitea:1.24
/// (`page` is 1-based, a past-the-end page returns `[]`).
const PAGE_LIMIT: usize = 50;

// Gitea API types - these match Gitea's REST v1 payloads exactly (fixtures
// recorded from a real gitea/gitea:1.24 container live in
// tests/fixtures/gitea_*.json). Quarantined inside this provider (ADR-0003):
// the calculator consumes only the neutral `MergedPr` produced by
// `GiteaPull::into_merged`.
#[derive(Debug, Clone, Deserialize)]
struct GiteaOrg {
    /// The org login. Gitea also sends `name`, but `username` is the field
    /// documented as the org's identifier.
    username: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GiteaRepo {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GiteaLabel {
    name: String,
}

/// The subset of `GET /repos/{owner}/{repo}/pulls` tuesday reads. Labels
/// arrive inline on the list payload (no per-PR follow-up request needed).
#[derive(Debug, Clone, Deserialize)]
struct GiteaPull {
    number: u64,
    title: String,
    body: Option<String>,
    html_url: String,
    merged: bool,
    merged_at: Option<DateTime<Utc>>,
    /// Defensive default: sibling fields (`assignees`) are `null` rather
    /// than `[]` when empty, so tolerate `null` (and an absent key) here.
    #[serde(default, deserialize_with = "null_as_empty")]
    labels: Vec<GiteaLabel>,
}

/// Deserialize `null` as an empty list (`#[serde(default)]` alone only
/// covers a *missing* key, not an explicit `null`).
fn null_as_empty<'de, D>(deserializer: D) -> Result<Vec<GiteaLabel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<GiteaLabel>>::deserialize(deserializer)?.unwrap_or_default())
}

impl GiteaPull {
    /// Convert the REST shape into the neutral domain type at the provider
    /// boundary. Returns `None` for closed-but-unmerged PRs: `state=closed`
    /// includes rejected/abandoned PRs, which Measure must not count.
    fn into_merged(self) -> Option<MergedPr> {
        if !self.merged {
            return None;
        }
        let merged_at = self.merged_at?;
        Some(MergedPr {
            number: self.number,
            title: self.title,
            body: self.body,
            url: self.html_url,
            merged_at,
            labels: self.labels.into_iter().map(|l| l.name).collect(),
        })
    }
}

/// The half-open UTC window `[first of month, first of next month)`.
fn month_window(year: u32, month: u32) -> Result<(DateTime<Utc>, DateTime<Utc>), SourceError> {
    let start_of = |y: u32, m: u32| {
        Utc.with_ymd_and_hms(y as i32, m, 1, 0, 0, 0)
            .single()
            .ok_or_else(|| SourceError::from(format!("invalid report month: {y:04}-{m:02}")))
    };
    let start = start_of(year, month)?;
    let end = if month == 12 {
        start_of(year + 1, 1)?
    } else {
        start_of(year, month + 1)?
    };
    Ok((start, end))
}

/// Keep only the PRs merged inside the window, converted to the neutral
/// type. Pure so the merge-window rule is testable against recorded
/// payloads without a network.
fn merged_in_window(
    pulls: Vec<GiteaPull>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<MergedPr> {
    pulls
        .into_iter()
        .filter_map(GiteaPull::into_merged)
        .filter(|pr| pr.merged_at >= start && pr.merged_at < end)
        .collect()
}

/// Read-only [`PrSource`] over Gitea's REST v1 API.
///
/// Targets a self-hosted instance (the dogfood path is conduit's throwaway
/// forge at `http://localhost:3000`), authenticated with
/// `Authorization: token <tok>` — plain token, no OAuth (ADR-0004-era
/// decision: OAuth stays a GitHub-web-head concern).
#[derive(Debug, Clone)]
pub struct GiteaSource {
    client: reqwest::Client,
    /// Instance base URL without a trailing slash, e.g. `http://localhost:3000`.
    base_url: String,
    token: Option<String>,
}

impl GiteaSource {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            client: reqwest::Client::new(),
            base_url,
            token,
        }
    }

    /// GET `{base}/api/v1{path_and_query}` and deserialize the JSON body.
    async fn get_json<T: DeserializeOwned>(&self, path_and_query: &str) -> Result<T, SourceError> {
        let url = format!("{}/api/v1{}", self.base_url, path_and_query);

        let mut request = self
            .client
            .get(&url)
            .header(USER_AGENT, "Tuesday Time Tracker")
            .header(ACCEPT, "application/json");

        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("token {token}"));
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let error_text = response.text().await?;
            return Err(format!("Gitea API error ({status}): {error_text}").into());
        }

        Ok(response.json().await?)
    }

    /// Fetch every page of a list endpoint: explicit `page`/`limit` query
    /// params, stopping on the first page shorter than [`PAGE_LIMIT`].
    async fn get_paginated<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &str,
    ) -> Result<Vec<T>, SourceError> {
        let sep = if query.is_empty() { "" } else { "&" };
        let mut all = Vec::new();
        let mut page = 1usize;

        loop {
            let batch: Vec<T> = self
                .get_json(&format!(
                    "{path}?{query}{sep}page={page}&limit={PAGE_LIMIT}"
                ))
                .await?;
            let batch_len = batch.len();
            all.extend(batch);

            if batch_len < PAGE_LIMIT {
                break;
            }
            page += 1;
        }

        Ok(all)
    }
}

impl PrSource for GiteaSource {
    /// `GET /api/v1/user/orgs` — orgs the token's user is a *member* of.
    /// Note the dogfood reviewer identity is a repo collaborator, not an
    /// org member, so this can legitimately be empty; callers may pass an
    /// owner directly to [`PrSource::list_repos`] / `fetch_merged_prs`.
    async fn list_orgs(&self) -> Result<Vec<String>, SourceError> {
        let orgs: Vec<GiteaOrg> = self.get_paginated("/user/orgs", "").await?;
        let mut names: Vec<String> = orgs.into_iter().map(|org| org.username).collect();
        names.sort_by_key(|name| name.to_lowercase());
        Ok(names)
    }

    /// `GET /api/v1/orgs/{org}/repos` — repos visible to the token.
    async fn list_repos(&self, org: &str) -> Result<Vec<String>, SourceError> {
        let repos: Vec<GiteaRepo> = self
            .get_paginated(&format!("/orgs/{org}/repos"), "")
            .await?;
        Ok(repos.into_iter().map(|repo| repo.name).collect())
    }

    /// `GET /api/v1/repos/{owner}/{repo}/pulls?state=closed`, then a
    /// client-side `merged_at`-in-month filter. Gitea cannot filter by
    /// merge date server-side (no `merged:` search like GitHub GraphQL),
    /// and `state=closed` is the narrowest state that includes merged PRs —
    /// it also includes rejected ones, which `into_merged` drops.
    async fn fetch_merged_prs(
        &self,
        owner: &str,
        repo: &str,
        year: u32,
        month: u32,
    ) -> Result<Vec<MergedPr>, SourceError> {
        let (start, end) = month_window(year, month)?;

        let pulls: Vec<GiteaPull> = self
            .get_paginated(&format!("/repos/{owner}/{repo}/pulls"), "state=closed")
            .await?;
        tracing::debug!(
            "Gitea {owner}/{repo}: {} closed PRs fetched; filtering to [{start}, {end})",
            pulls.len()
        );

        Ok(merged_in_window(pulls, start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures recorded verbatim from gitea/gitea:1.24.7 on conduit's
    // dogfood forge (org como, repo conduit-dogfood) after seeding one
    // merged PR (number 1, contract-shaped labels) and one
    // closed-without-merging PR (number 2).
    const PULLS_CLOSED: &str = include_str!("../tests/fixtures/gitea_pulls_closed.json");
    const USER_ORGS: &str = include_str!("../tests/fixtures/gitea_user_orgs.json");
    const ORG_REPOS: &str = include_str!("../tests/fixtures/gitea_org_repos.json");

    fn recorded_pulls() -> Vec<GiteaPull> {
        serde_json::from_str(PULLS_CLOSED).expect("recorded pulls payload deserializes")
    }

    #[test]
    fn recorded_closed_pulls_payload_deserializes() {
        let pulls = recorded_pulls();
        assert_eq!(pulls.len(), 2);
        // Newest first: PR 2 is the closed-unmerged one.
        assert_eq!(pulls[0].number, 2);
        assert!(!pulls[0].merged);
        assert_eq!(pulls[1].number, 1);
        assert!(pulls[1].merged);
    }

    #[test]
    fn merged_pull_converts_to_neutral_merged_pr_with_labels_and_body() {
        let pull = recorded_pulls().remove(1);

        let merged_pr = pull.into_merged().expect("PR 1 is merged");

        assert_eq!(
            merged_pr,
            MergedPr {
                number: 1,
                title: "[ADR-0001] Seed merged PR for tuesday e2e".to_string(),
                body: Some(
                    "Seeds one contract-shaped merged PR so tuesday can measure it.\n\n\
                     Adr-Reference: ADR-0001"
                        .to_string()
                ),
                url: "http://localhost:3000/como/conduit-dogfood/pulls/1".to_string(),
                merged_at: Utc.with_ymd_and_hms(2026, 6, 12, 9, 32, 16).unwrap(),
                labels: vec![
                    "adr:ADR-0001".to_string(),
                    "effort:1-super-quick".to_string()
                ],
            }
        );
    }

    #[test]
    fn closed_unmerged_pull_is_dropped() {
        // state=closed includes rejected PRs; Measure must not count them.
        let pull = recorded_pulls().remove(0);
        assert_eq!(pull.into_merged(), None);
    }

    #[test]
    fn window_filter_keeps_only_prs_merged_in_the_month() {
        let in_june = {
            let (start, end) = month_window(2026, 6).unwrap();
            merged_in_window(recorded_pulls(), start, end)
        };
        assert_eq!(
            in_june.iter().map(|pr| pr.number).collect::<Vec<_>>(),
            vec![1]
        );

        // The recorded PR was merged 2026-06-12; adjacent months are empty.
        for (year, month) in [(2026, 5), (2026, 7)] {
            let (start, end) = month_window(year, month).unwrap();
            assert!(merged_in_window(recorded_pulls(), start, end).is_empty());
        }
    }

    #[test]
    fn month_window_is_half_open_and_rolls_over_december() {
        let (start, end) = month_window(2026, 12).unwrap();
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap());

        // Exactly midnight on the 1st belongs to the month; the next
        // month's midnight does not.
        let mut boundary = recorded_pulls().remove(1);
        boundary.merged_at = Some(start);
        assert_eq!(
            merged_in_window(vec![boundary.clone()], start, end).len(),
            1
        );
        boundary.merged_at = Some(end);
        assert!(merged_in_window(vec![boundary], start, end).is_empty());
    }

    #[test]
    fn month_window_rejects_invalid_months() {
        assert!(month_window(2026, 0).is_err());
        assert!(month_window(2026, 13).is_err());
    }

    #[test]
    fn pull_payload_tolerates_null_or_absent_labels() {
        // Gitea sends null (not []) for empty sibling collections such as
        // assignees; be tolerant if labels ever arrives null or absent.
        for labels_part in [r#","labels":null}"#, "}"] {
            let json = format!(
                r#"{{"number":9,"title":"t","body":null,"html_url":"http://localhost:3000/o/r/pulls/9",
                    "merged":true,"merged_at":"2026-06-12T09:32:16Z"{labels_part}"#,
            );
            let pull: GiteaPull = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("labels variant {labels_part:?} tolerated: {e}"));
            assert!(pull.labels.is_empty());
            assert_eq!(pull.into_merged().unwrap().labels, Vec::<String>::new());
        }
    }

    #[test]
    fn recorded_org_and_repo_payloads_deserialize() {
        let orgs: Vec<GiteaOrg> = serde_json::from_str(USER_ORGS).unwrap();
        assert_eq!(
            orgs.iter().map(|o| o.username.as_str()).collect::<Vec<_>>(),
            vec!["como"]
        );

        let repos: Vec<GiteaRepo> = serde_json::from_str(ORG_REPOS).unwrap();
        assert_eq!(
            repos.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["conduit-dogfood"]
        );
    }

    #[test]
    fn base_url_trailing_slash_is_trimmed() {
        let source = GiteaSource::new("http://localhost:3000/", None);
        assert_eq!(source.base_url, "http://localhost:3000");
    }
}

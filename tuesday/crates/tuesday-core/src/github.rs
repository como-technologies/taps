use crate::domain::MergedPr;
use crate::source::{PrSource, SourceError};
use chrono::{DateTime, Utc};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};

// GitHub API types - these match GitHub's GraphQL API structure exactly.
// Quarantined inside this provider (ADR-0003): the calculator consumes only
// the neutral `MergedPr` produced by `PullRequest::into_merged`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequest {
    number: i32,
    title: String,
    body: Option<String>,
    created_at: DateTime<Utc>,
    merged_at: Option<DateTime<Utc>>,
    url: String,
    author: Option<Actor>,
    assignees: Assignees,
    labels: Labels,
}

impl PullRequest {
    /// Convert the GraphQL shape into the neutral domain type at the
    /// provider boundary. The search query is `is:merged`, so `merged_at`
    /// is always present in practice; fall back to `created_at` rather
    /// than panic if it ever is not.
    fn into_merged(self) -> MergedPr {
        MergedPr {
            number: self.number as u64,
            title: self.title,
            body: self.body,
            url: self.url,
            merged_at: self.merged_at.unwrap_or(self.created_at),
            labels: self.labels.nodes.into_iter().map(|l| l.name).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Actor {
    login: String,
    name: Option<String>, // GitHub profile display name
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Assignees {
    nodes: Vec<Actor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Labels {
    nodes: Vec<Label>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Label {
    name: String,
}

// GraphQL query and response structures
#[derive(Serialize)]
struct GraphQLRequest {
    query: String,
    variables: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct GraphQLResponse {
    data: Option<GraphQLData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Deserialize, Debug)]
struct GraphQLData {
    search: SearchResult,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SearchResult {
    page_info: PageInfo,
    nodes: Vec<Option<PullRequest>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GitHubOrg {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GitHubRepo {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct GitHubTeam {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct GitHubSource {
    client: reqwest::Client,
    token: Option<String>,
}

impl GitHubSource {
    pub fn new(token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            token,
        }
    }

    // Mock data for testing
    pub async fn fetch_user_orgs(&self) -> Result<Vec<GitHubOrg>, SourceError> {
        let url = "https://api.github.com/user/orgs?per_page=100&sort=name";

        let mut request = self
            .client
            .get(url)
            .header(USER_AGENT, "Tuesday Time Tracker")
            .header(ACCEPT, "application/vnd.github.v3+json");

        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }

        let response = request.send().await?;
        let status = response.status();
        let scopes = response
            .headers()
            .get("x-oauth-scopes")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("none");
        tracing::info!(
            "GitHub /user/orgs response: status={}, scopes={}",
            status,
            scopes
        );

        if !status.is_success() {
            let error_text = response.text().await?;
            return Err(format!("GitHub API error: {error_text}").into());
        }

        let mut orgs: Vec<GitHubOrg> = response.json().await?;
        tracing::debug!("Fetched {} organizations", orgs.len());

        orgs.sort_by_key(|org| org.login.to_lowercase());
        Ok(orgs)
    }

    pub async fn fetch_org_repos(&self, org: &str) -> Result<Vec<GitHubRepo>, SourceError> {
        let url = format!("https://api.github.com/orgs/{org}/repos?per_page=100&sort=updated");

        let mut request = self
            .client
            .get(&url)
            .header(USER_AGENT, "Tuesday Time Tracker")
            .header(ACCEPT, "application/vnd.github.v3+json");

        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("GitHub API error: {error_text}").into());
        }

        let repos: Vec<GitHubRepo> = response.json().await?;
        let count = repos.len();
        tracing::debug!("Fetched {count} repositories for org {org}");

        Ok(repos)
    }

    pub async fn fetch_merged_prs_graphql(
        &self,
        org: &str,
        repo: &str,
        year: u32,
        month: u32,
    ) -> Result<Vec<MergedPr>, SourceError> {
        tracing::debug!("fetch_merged_prs_graphql START");
        tracing::debug!(
            "Org: {}, Repo: {}, Year: {}, Month: {}",
            org,
            repo,
            year,
            month
        );

        let start_date = format!("{year:04}-{month:02}-01");
        let end_date = if month == 12 {
            format!("{:04}-01-01", year + 1)
        } else {
            format!("{year:04}-{:02}-01", month + 1)
        };

        let search_query =
            format!("repo:{org}/{repo} is:pr is:merged merged:{start_date}..{end_date}");
        tracing::debug!("Search query: {search_query}");

        let graphql_query = r#"
            query FetchMergedPRs($query: String!, $first: Int!, $after: String) {
                search(query: $query, type: ISSUE, first: $first, after: $after) {
                    issueCount
                    pageInfo {
                        hasNextPage
                        endCursor
                    }
                    nodes {
                        ... on PullRequest {
                            number
                            title
                            body
                            createdAt
                            mergedAt
                            url
                            author {
                                login
                            }
                            assignees(first: 10) {
                                nodes {
                                    login
                                }
                            }
                            labels(first: 20) {
                                nodes {
                                    name
                                }
                            }
                        }
                    }
                }
            }
        "#;

        let mut all_prs = Vec::new();
        let mut cursor: Option<String> = None;
        let mut has_next_page = true;

        while has_next_page {
            let variables = serde_json::json!({
                "query": search_query,
                "first": 100,
                "after": cursor
            });

            let request_body = GraphQLRequest {
                query: graphql_query.to_string(),
                variables,
            };

            let mut request = self
                .client
                .post("https://api.github.com/graphql")
                .header(USER_AGENT, "Tuesday Time Tracker")
                .header(ACCEPT, "application/vnd.github.v4+json");

            if let Some(token) = &self.token {
                request = request.header(AUTHORIZATION, format!("Bearer {token}"));
            }

            let response = request.json(&request_body).send().await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                tracing::error!("GraphQL API error: {error_text}");
                return Err(format!("GraphQL API error: {error_text}").into());
            }

            let graphql_response: GraphQLResponse = response.json().await?;

            if let Some(errors) = graphql_response.errors {
                let error_messages: Vec<String> =
                    errors.iter().map(|e| e.message.clone()).collect();
                tracing::error!("GraphQL errors: {:?}", error_messages);
                return Err(format!("GraphQL errors: {:?}", error_messages).into());
            }

            if let Some(data) = graphql_response.data {
                tracing::debug!("Found {} PRs in this page", data.search.nodes.len());

                for pr in data.search.nodes.into_iter().flatten() {
                    // Convert to the neutral domain type at the boundary
                    all_prs.push(pr.into_merged());
                }

                has_next_page = data.search.page_info.has_next_page;
                cursor = data.search.page_info.end_cursor;
            } else {
                break;
            }
        }

        tracing::debug!(
            "fetch_merged_prs_graphql END: Returning {} PRs",
            all_prs.len()
        );
        Ok(all_prs)
    }
}

impl PrSource for GitHubSource {
    async fn list_orgs(&self) -> Result<Vec<String>, SourceError> {
        Ok(self
            .fetch_user_orgs()
            .await?
            .into_iter()
            .map(|org| org.login)
            .collect())
    }

    async fn list_repos(&self, org: &str) -> Result<Vec<String>, SourceError> {
        Ok(self
            .fetch_org_repos(org)
            .await?
            .into_iter()
            .map(|repo| repo.name)
            .collect())
    }

    async fn fetch_merged_prs(
        &self,
        owner: &str,
        repo: &str,
        year: u32,
        month: u32,
    ) -> Result<Vec<MergedPr>, SourceError> {
        self.fetch_merged_prs_graphql(owner, repo, year, month)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MergedPr;
    use chrono::TimeZone;

    fn graphql_pr(merged_at: Option<DateTime<Utc>>) -> PullRequest {
        PullRequest {
            number: 42,
            title: "Add widget".to_string(),
            body: Some("Body text".to_string()),
            created_at: Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
            merged_at,
            url: "https://github.com/como/alpha/pull/42".to_string(),
            author: Some(Actor {
                login: "dev".to_string(),
                name: None,
            }),
            assignees: Assignees { nodes: Vec::new() },
            labels: Labels {
                nodes: vec![
                    Label {
                        name: "effort:2-not-long".to_string(),
                    },
                    Label {
                        name: "adr:ADR-0003".to_string(),
                    },
                ],
            },
        }
    }

    #[test]
    fn graphql_pull_request_converts_to_neutral_merged_pr() {
        let merged = Utc.with_ymd_and_hms(2026, 1, 9, 8, 7, 6).unwrap();

        let merged_pr = graphql_pr(Some(merged)).into_merged();

        assert_eq!(
            merged_pr,
            MergedPr {
                number: 42,
                title: "Add widget".to_string(),
                body: Some("Body text".to_string()),
                url: "https://github.com/como/alpha/pull/42".to_string(),
                merged_at: merged,
                labels: vec!["effort:2-not-long".to_string(), "adr:ADR-0003".to_string()],
            }
        );
    }

    #[test]
    fn conversion_falls_back_to_created_at_when_merged_at_missing() {
        // The GraphQL search is `is:merged`, so merged_at is always present
        // in practice; a missing value must not panic.
        let merged_pr = graphql_pr(None).into_merged();
        assert_eq!(
            merged_pr.merged_at,
            Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
        );
    }
}

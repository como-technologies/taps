use chrono::{DateTime, Duration, Utc};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// GitHub App configuration (client-side safe values only)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitHubAppConfig {
    pub client_id: String,
    pub redirect_uri: String,
}

impl GitHubAppConfig {
    /// Generate the OAuth authorization URL
    pub fn authorization_url(&self, state: &str) -> String {
        // Request scopes needed for the app:
        // - repo: read repository data and PRs
        // - read:org: list user's organizations
        let scope = "repo read:org";

        let url = format!(
            "https://github.com/login/oauth/authorize?\
            client_id={}&\
            redirect_uri={}&\
            scope={}&\
            state={}",
            self.client_id,
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(scope),
            urlencoding::encode(state)
        );

        tracing::info!("OAuth URL: {}", url);
        url
    }
}

/// Server function to get GitHub App config (public values only)
/// This allows runtime configuration following 12-factor principles
#[server]
pub async fn get_github_app_config() -> Result<Option<GitHubAppConfig>, ServerFnError> {
    let client_id = std::env::var("GITHUB_APP_CLIENT_ID").ok();
    let redirect_uri = std::env::var("GITHUB_APP_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:8080/auth/callback".to_string());

    tracing::info!(
        "get_github_app_config: client_id={:?}",
        client_id.as_ref().map(|_| "[set]")
    );

    Ok(client_id.map(|client_id| GitHubAppConfig {
        client_id,
        redirect_uri,
    }))
}

/// Response from GitHub token exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub refresh_token_expires_in: i64,
    pub scope: String,
}

impl TokenResponse {
    /// Calculate the absolute expiry time based on expires_in seconds
    pub fn expires_at(&self) -> DateTime<Utc> {
        Utc::now() + Duration::seconds(self.expires_in)
    }
}

/// Error response from GitHub OAuth. Deserialized only inside the
/// server-side token-exchange bodies, so it exists only on that lane
/// (`error_uri` arrives too but is never read; serde ignores it).
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
pub struct GitHubOAuthError {
    pub error: String,
    pub error_description: Option<String>,
}

#[cfg(feature = "server")]
impl std::fmt::Display for GitHubOAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)?;
        if let Some(desc) = &self.error_description {
            write!(f, ": {}", desc)?;
        }
        Ok(())
    }
}

/// Generate a random state string for CSRF protection
pub fn generate_oauth_state() -> String {
    // Use js_sys for WASM-compatible random/time
    #[cfg(target_arch = "wasm32")]
    {
        // Use Math.random() and Date.now() for WASM
        let random = (js_sys::Math::random() * 1_000_000_000.0) as u64;
        let time = js_sys::Date::now() as u64;
        format!("{:x}{:x}", time, random)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{:x}", nonce)
    }
}

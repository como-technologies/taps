use super::github_app::TokenResponse;
use dioxus::prelude::*;

// Only the server-side bodies below deserialize GitHub's OAuth error shape;
// on the web lane those bodies are stripped, so the import is lane-gated.
#[cfg(feature = "server")]
use super::github_app::GitHubOAuthError;

/// Exchange OAuth authorization code for access token
/// This runs on the server to keep the client secret secure
#[server]
pub async fn exchange_oauth_code(code: String) -> Result<TokenResponse, ServerFnError> {
    let client_id = std::env::var("GITHUB_APP_CLIENT_ID")
        .map_err(|_| ServerFnError::new("GITHUB_APP_CLIENT_ID not configured"))?;
    let client_secret = std::env::var("GITHUB_APP_CLIENT_SECRET")
        .map_err(|_| ServerFnError::new("GITHUB_APP_CLIENT_SECRET not configured"))?;

    tracing::info!("Exchanging OAuth code for token");

    let client = reqwest::Client::new();
    let response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .header("User-Agent", "Tuesday-Effort-Tracker")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
        ])
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("Request failed: {}", e)))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to read response: {}", e)))?;

    tracing::debug!(
        "GitHub OAuth response status: {}, body length: {}",
        status,
        text.len()
    );

    // Try to parse as success first
    if let Ok(token) = serde_json::from_str::<TokenResponse>(&text) {
        tracing::info!("Successfully obtained access token");
        return Ok(token);
    }

    // Try to parse as error
    if let Ok(error) = serde_json::from_str::<GitHubOAuthError>(&text) {
        tracing::warn!("GitHub OAuth error: {}", error);
        return Err(ServerFnError::new(format!("GitHub error: {}", error)));
    }

    tracing::error!("Failed to parse GitHub response: {}", text);
    Err(ServerFnError::new("Failed to parse GitHub response"))
}

/// Refresh an expired access token
/// This runs on the server to keep the client secret secure
#[server]
pub async fn refresh_oauth_token(refresh_token: String) -> Result<TokenResponse, ServerFnError> {
    let client_id = std::env::var("GITHUB_APP_CLIENT_ID")
        .map_err(|_| ServerFnError::new("GITHUB_APP_CLIENT_ID not configured"))?;
    let client_secret = std::env::var("GITHUB_APP_CLIENT_SECRET")
        .map_err(|_| ServerFnError::new("GITHUB_APP_CLIENT_SECRET not configured"))?;

    tracing::info!("Refreshing OAuth token");

    let client = reqwest::Client::new();
    let response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .header("User-Agent", "Tuesday-Effort-Tracker")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("Request failed: {}", e)))?;

    let text = response
        .text()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to read response: {}", e)))?;

    // Try to parse as success first
    if let Ok(token) = serde_json::from_str::<TokenResponse>(&text) {
        tracing::info!("Successfully refreshed access token");
        return Ok(token);
    }

    // Try to parse as error
    if let Ok(error) = serde_json::from_str::<GitHubOAuthError>(&text) {
        tracing::warn!("GitHub OAuth refresh error: {}", error);
        return Err(ServerFnError::new(format!("GitHub error: {}", error)));
    }

    tracing::error!("Failed to parse GitHub refresh response");
    Err(ServerFnError::new("Failed to parse GitHub response"))
}

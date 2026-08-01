use crate::auth::{
    AuthState, TokenStorage, clear_oauth_state, exchange_oauth_code, get_oauth_state,
};
use dioxus::prelude::*;

/// OAuth callback handler component
/// Processes the authorization code from GitHub and exchanges it for tokens
#[component]
pub fn AuthCallback(on_complete: EventHandler<Result<AuthState, String>>) -> Element {
    let mut status = use_signal(|| "Processing authentication...".to_string());
    let mut error = use_signal(|| Option::<String>::None);

    // Process the callback on mount
    use_effect(move || {
        spawn(async move {
            match process_oauth_callback().await {
                Ok(auth_state) => {
                    status.set("Authentication successful! Redirecting...".to_string());
                    if let Err(e) = TokenStorage::save(&auth_state) {
                        tracing::warn!("Failed to save auth state to localStorage: {}", e);
                    }
                    on_complete.call(Ok(auth_state));
                }
                Err(e) => {
                    tracing::error!("OAuth callback failed: {}", e);
                    error.set(Some(e.clone()));
                    status.set("Authentication failed".to_string());
                    on_complete.call(Err(e));
                }
            }
        });
    });

    rsx! {
        div { class: "auth-callback",
            div { class: "auth-callback-content",
                h2 { "GitHub Authentication" }

                if let Some(err) = error() {
                    div { class: "auth-error",
                        p { "Error: {err}" }
                        p { "Please try again or use a Personal Access Token." }
                    }
                } else {
                    div { class: "auth-loading",
                        div { class: "spinner" }
                        p { "{status}" }
                    }
                }
            }
        }
    }
}

/// Process the OAuth callback by extracting the code and exchanging it for tokens
async fn process_oauth_callback() -> Result<AuthState, String> {
    tracing::info!("Processing OAuth callback");

    // Get URL parameters
    let (code, returned_state) = get_url_params()?;
    tracing::info!(
        "Got URL params, code length: {}, state: {}",
        code.len(),
        &returned_state[..8.min(returned_state.len())]
    );

    // Validate state for CSRF protection
    let stored_state = get_oauth_state().ok_or("No stored OAuth state - possible CSRF attack")?;
    tracing::info!(
        "Stored state: {}",
        &stored_state[..8.min(stored_state.len())]
    );

    if returned_state != stored_state {
        clear_oauth_state();
        return Err("OAuth state mismatch - possible CSRF attack".to_string());
    }

    // Clear stored state now that we've validated it
    clear_oauth_state();

    tracing::info!("OAuth callback received, exchanging code for token");

    // Exchange code for token via server function
    let token_response = exchange_oauth_code(code)
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;
    tracing::info!(
        "Token exchange successful, expires_in: {}",
        token_response.expires_in
    );

    // Get user info
    let user_login = get_user_login(&token_response.access_token).await?;
    tracing::info!("Got user login: {}", user_login);

    // Calculate expires_at before moving fields
    let expires_at = token_response.expires_at();
    let auth_state = AuthState::GitHubAppAuthenticated {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_at,
        user_login: user_login.clone(),
    };

    tracing::info!("OAuth flow complete, authenticated as {}", user_login);
    Ok(auth_state)
}

/// Extract code and state from URL query parameters
fn get_url_params() -> Result<(String, String), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;

        let window = window().ok_or("No window object")?;
        let location = window.location();
        let search = location.search().map_err(|_| "Failed to get URL search")?;

        let params = web_sys::UrlSearchParams::new_with_str(&search)
            .map_err(|_| "Failed to parse URL params")?;

        // Check for error from GitHub
        if let Some(error) = params.get("error") {
            let description = params.get("error_description").unwrap_or_default();
            return Err(format!("GitHub error: {} - {}", error, description));
        }

        let code = params.get("code").ok_or("Missing authorization code")?;
        let state = params.get("state").ok_or("Missing state parameter")?;

        Ok((code, state))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("URL params not available on server".to_string())
    }
}

/// Get user login from GitHub API
async fn get_user_login(access_token: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct GitHubUser {
        login: String,
    }

    let client = reqwest::Client::new();

    let response = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "Tuesday-Effort-Tracker")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("Failed to get user info: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(format!("GitHub API error: {}", status));
    }

    let user: GitHubUser = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse user info: {}", e))?;

    Ok(user.login)
}

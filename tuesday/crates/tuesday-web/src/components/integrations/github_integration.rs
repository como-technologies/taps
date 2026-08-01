use super::{IntegrationCard, IntegrationStatus};
use crate::auth::{
    AuthState, TokenStorage, generate_oauth_state, get_github_app_config, store_oauth_state,
};
use dioxus::prelude::*;

/// GitHub integration card for Settings page
#[component]
pub fn GitHubIntegration(
    auth_state: Signal<AuthState>,
    on_auth_change: EventHandler<AuthState>,
) -> Element {
    let mut error = use_signal(|| Option::<String>::None);
    let mut pat_value = use_signal(String::new);
    let mut auth_method = use_signal(|| AuthMethod::OAuth);

    // Fetch GitHub App config from server
    let github_app_config = use_resource(|| async { get_github_app_config().await.ok().flatten() });

    // Determine connection status
    let (status, account_name) = match auth_state() {
        AuthState::GitHubAppAuthenticated { user_login, .. } => {
            (IntegrationStatus::Connected, Some(user_login))
        }
        AuthState::PatAuthenticated { .. } => (
            IntegrationStatus::Connected,
            Some("Personal Token".to_string()),
        ),
        AuthState::Unauthenticated => (IntegrationStatus::Disconnected, None),
    };

    let github_app_available = github_app_config
        .read()
        .as_ref()
        .map(|c| c.is_some())
        .unwrap_or(false);

    rsx! {
        IntegrationCard {
            name: "GitHub",
            description: "Access repositories and pull requests for effort tracking",
            icon: "\u{1F419}", // Octopus emoji (GitHub mascot)
            status,
            account_name,

            // Card content
            if status.is_connected() {
                // Connected state - show disconnect option
                div { class: "integration-connected",
                    p { class: "connected-info",
                        "Your GitHub account is connected. Tuesday can access your repositories and pull requests."
                    }
                    button {
                        class: "disconnect-btn",
                        onclick: move |_| {
                            if let Err(e) = TokenStorage::clear() {
                                tracing::warn!("Failed to clear storage: {}", e);
                            }
                            on_auth_change.call(AuthState::Unauthenticated);
                        },
                        "Disconnect"
                    }
                }
            } else {
                // Disconnected state - show auth options
                div { class: "integration-auth",
                    // Error display
                    if let Some(err) = error() {
                        div { class: "auth-error", "{err}" }
                    }

                    // Auth method tabs (only if OAuth is available)
                    if github_app_available {
                        div { class: "auth-method-tabs",
                            button {
                                class: if auth_method() == AuthMethod::OAuth { "tab active" } else { "tab" },
                                onclick: move |_| auth_method.set(AuthMethod::OAuth),
                                "Sign in with GitHub"
                            }
                            button {
                                class: if auth_method() == AuthMethod::Pat { "tab active" } else { "tab" },
                                onclick: move |_| auth_method.set(AuthMethod::Pat),
                                "Personal Access Token"
                            }
                        }
                    }

                    // Auth content based on method
                    match auth_method() {
                        AuthMethod::OAuth if github_app_available => rsx! {
                            div { class: "oauth-section",
                                p { class: "auth-info",
                                    "Sign in securely with your GitHub account. Tokens refresh automatically."
                                }
                                button {
                                    class: "connect-btn github-btn",
                                    onclick: move |_| {
                                        if let Some(Some(config)) = github_app_config.read().as_ref() {
                                            let state = generate_oauth_state();
                                            match store_oauth_state(&state) {
                                                Ok(()) => {
                                                    let url = config.authorization_url(&state);
                                                    navigate_to_url(&url);
                                                }
                                                Err(e) => {
                                                    error.set(Some(format!("Failed to store OAuth state: {}", e)));
                                                }
                                            }
                                        } else {
                                            error.set(Some("GitHub App not configured".to_string()));
                                        }
                                    },
                                    "Connect GitHub Account"
                                }
                            }
                        },
                        _ => rsx! {
                            div { class: "pat-section",
                                p { class: "auth-info",
                                    "Enter a GitHub Personal Access Token with "
                                    code { "repo" }
                                    " and "
                                    code { "read:org" }
                                    " scopes."
                                }
                                div { class: "pat-input-group",
                                    input {
                                        r#type: "password",
                                        class: "pat-input",
                                        placeholder: "ghp_...",
                                        value: "{pat_value}",
                                        oninput: move |evt| {
                                            pat_value.set(evt.value().clone());
                                        },
                                    }
                                    button {
                                        class: "connect-btn",
                                        disabled: pat_value().is_empty(),
                                        onclick: move |_| {
                                            let token = pat_value();
                                            if !token.is_empty() {
                                                let new_state = AuthState::PatAuthenticated { token };
                                                if let Err(e) = TokenStorage::save(&new_state) {
                                                    tracing::warn!("Failed to save auth state: {}", e);
                                                }
                                                on_auth_change.call(new_state);
                                            }
                                        },
                                        "Connect"
                                    }
                                }
                                p { class: "auth-help",
                                    a {
                                        href: "https://github.com/settings/tokens/new?scopes=repo,read:org&description=Tuesday%20Effort%20Tracker",
                                        target: "_blank",
                                        "Create a new token on GitHub \u{2192}"
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AuthMethod {
    OAuth,
    Pat,
}

/// Navigate to external URL
fn navigate_to_url(url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;

        if let Some(window) = window() {
            if let Err(e) = window.location().set_href(url) {
                tracing::error!("Failed to navigate: {:?}", e);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = url;
        tracing::warn!("Navigation not available on server");
    }
}

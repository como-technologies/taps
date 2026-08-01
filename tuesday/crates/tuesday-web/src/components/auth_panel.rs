use crate::auth::{
    AuthState, TokenStorage, generate_oauth_state, get_github_app_config, store_oauth_state,
};
use dioxus::prelude::*;

/// Tab selection for auth methods
#[derive(Clone, Copy, PartialEq)]
enum AuthTab {
    GitHubApp,
    PersonalToken,
}

/// Unified authentication panel with tabs for GitHub App and PAT login
#[component]
pub fn AuthPanel(
    auth_state: Signal<AuthState>,
    on_auth_change: EventHandler<AuthState>,
) -> Element {
    let mut active_tab = use_signal(|| AuthTab::GitHubApp);
    let mut error = use_signal(|| Option::<String>::None);
    let mut pat_value = use_signal(String::new);

    // Fetch GitHub App config from server (runtime, 12-factor compliant)
    let github_app_config = use_resource(|| async {
        let result = get_github_app_config().await;
        tracing::info!("GitHub App config result: {:?}", result);
        result.ok().flatten()
    });

    // Check if GitHub App is configured
    let config_read = github_app_config.read();
    let github_app_available = config_read.as_ref().map(|c| c.is_some()).unwrap_or(false);
    tracing::debug!("github_app_available: {}", github_app_available);

    // Debug: log the current auth state
    tracing::debug!("AuthPanel rendering with state: {:?}", auth_state());

    rsx! {
        div { class: "auth-panel",
            h3 { "GitHub Authentication" }

            // Show current auth status if authenticated
            match auth_state() {
                AuthState::GitHubAppAuthenticated { user_login, .. } => rsx! {
                    div { class: "auth-status authenticated",
                        span { class: "auth-status-text",
                            "Signed in as "
                            strong { "{user_login}" }
                        }
                        button {
                            class: "logout-btn",
                            onclick: move |_| {
                                if let Err(e) = TokenStorage::clear() {
                                    tracing::warn!("Failed to clear storage: {}", e);
                                }
                                on_auth_change.call(AuthState::Unauthenticated);
                            },
                            "Sign out"
                        }
                    }
                },
                AuthState::PatAuthenticated { .. } => rsx! {
                    div { class: "auth-status authenticated",
                        span { class: "auth-status-text", "Using Personal Access Token" }
                        button {
                            class: "logout-btn",
                            onclick: move |_| {
                                if let Err(e) = TokenStorage::clear() {
                                    tracing::warn!("Failed to clear storage: {}", e);
                                }
                                on_auth_change.call(AuthState::Unauthenticated);
                            },
                            "Clear token"
                        }
                    }
                },
                AuthState::Unauthenticated => rsx! {
                    // Tab selector
                    div { class: "auth-tabs",
                        if github_app_available {
                            button {
                                class: if active_tab() == AuthTab::GitHubApp { "tab active" } else { "tab" },
                                onclick: move |_| active_tab.set(AuthTab::GitHubApp),
                                "GitHub"
                            }
                        }
                        button {
                            class: if active_tab() == AuthTab::PersonalToken { "tab active" } else { "tab" },
                            onclick: move |_| active_tab.set(AuthTab::PersonalToken),
                            "Access Token"
                        }
                    }

                    // Error display
                    if let Some(err) = error() {
                        div { class: "auth-error", "{err}" }
                    }

                    // Tab content
                    match active_tab() {
                        AuthTab::GitHubApp if github_app_available => rsx! {
                            div { class: "auth-content",
                                p { class: "auth-description",
                                    "Sign in securely with your GitHub account. "
                                    "Tokens expire after 1 hour and refresh automatically."
                                }
                                button {
                                    class: "github-login-btn",
                                    onclick: move |_| {
                                        if let Some(Some(config)) = github_app_config.read().as_ref() {
                                            let state = generate_oauth_state();
                                            match store_oauth_state(&state) {
                                                Ok(()) => {
                                                    let url = config.authorization_url(&state);
                                                    navigate_to_github(&url);
                                                }
                                                Err(e) => {
                                                    error.set(Some(format!("Failed to store OAuth state: {}", e)));
                                                }
                                            }
                                        } else {
                                            error.set(Some("GitHub App not configured".to_string()));
                                        }
                                    },
                                    "Sign in with GitHub"
                                }
                            }
                        },
                        _ => rsx! {
                            div { class: "auth-content",
                                p { class: "auth-description",
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
                                        class: "pat-submit-btn",
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
                                        "Save Token"
                                    }
                                }
                                p { class: "pat-help",
                                    a {
                                        href: "https://github.com/settings/tokens/new?scopes=repo,read:org&description=Tuesday%20Effort%20Tracker",
                                        target: "_blank",
                                        "Create a new token on GitHub →"
                                    }
                                }
                            }
                        },
                    }
                },
            }
        }
    }
}

/// Navigate to GitHub OAuth URL
fn navigate_to_github(url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;

        if let Some(window) = window() {
            if let Err(e) = window.location().set_href(url) {
                tracing::error!("Failed to navigate to GitHub: {:?}", e);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = url;
        tracing::warn!("Navigation not available on server");
    }
}

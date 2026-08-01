use crate::Route;
use crate::auth::AuthState;
use dioxus::prelude::*;

/// Top header bar with branding and connection status
#[component]
pub fn Header(auth_state: Signal<AuthState>) -> Element {
    // Determine GitHub connection status
    let github_connected = matches!(
        auth_state(),
        AuthState::GitHubAppAuthenticated { .. } | AuthState::PatAuthenticated { .. }
    );

    // Get user login if available
    let user_login = match auth_state() {
        AuthState::GitHubAppAuthenticated { user_login, .. } => Some(user_login),
        _ => None,
    };

    rsx! {
        header { class: "app-header",
            // Left side - page title
            h1 { class: "header-title", "Tuesday" }

            // Right side - connection status and user
            div { class: "header-actions",
                // Connection status icons
                div { class: "connection-status",
                    // GitHub status
                    Link {
                        to: Route::Settings {},
                        class: "status-icon-link",
                        title: if github_connected { "GitHub: Connected" } else { "GitHub: Not connected" },
                        div {
                            class: if github_connected { "status-icon connected" } else { "status-icon disconnected" },
                            "\u{1F419}" // Octopus emoji
                        }
                    }

                    // Claude status (always disconnected for now)
                    Link {
                        to: Route::Settings {},
                        class: "status-icon-link",
                        title: "Claude AI: Not configured",
                        div { class: "status-icon disconnected",
                            "\u{1F916}" // Robot emoji
                        }
                    }
                }

                // User indicator (if logged in via GitHub)
                if let Some(login) = user_login {
                    div { class: "user-indicator",
                        span { class: "user-avatar", "{login.chars().next().unwrap_or('?').to_uppercase()}" }
                        span { class: "user-name", "{login}" }
                    }
                }
            }
        }
    }
}

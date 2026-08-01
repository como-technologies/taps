mod auth;
mod components;
mod hooks;
mod server_fns;

use components::auth_callback::AuthCallback;
use components::{AppShell, Dashboard, GitHubIntegration, GiteaIntegration};
use dioxus::prelude::*;
use hooks::use_auth;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    // Load .env file if present (server-side only, silently ignored on WASM)
    let _ = dotenvy::dotenv();

    dioxus::launch(App);
}

/// Application routes
#[derive(Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    // Routes wrapped in AppShell layout
    #[layout(AppShell)]
        #[route("/")]
        Home {},
        #[route("/reports")]
        Reports {},
        #[route("/settings")]
        Settings {},
    #[end_layout]

    // Auth callback is outside the shell (no sidebar during OAuth flow)
    #[route("/auth/callback")]
    AuthCallbackPage {},
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Router::<Route> {}
    }
}

/// Home page - welcome/overview
#[component]
fn Home() -> Element {
    rsx! {
        div { class: "page home-page",
            h2 { "Welcome to Tuesday" }
            p { class: "home-intro",
                "Tuesday helps you track and analyze engineering effort across your projects."
            }

            div { class: "home-cards",
                div { class: "home-card",
                    h3 { "\u{1F4CA} Reports" }
                    p { "Generate effort analysis reports from your GitHub pull requests." }
                    Link { to: Route::Reports {}, class: "card-link", "Go to Reports \u{2192}" }
                }

                div { class: "home-card",
                    h3 { "\u{2699}\u{FE0F} Settings" }
                    p { "Configure your preferences and manage integrations." }
                    Link { to: Route::Settings {}, class: "card-link", "Go to Settings \u{2192}" }
                }
            }
        }
    }
}

/// Reports page - wraps existing Dashboard
#[component]
fn Reports() -> Element {
    rsx! { Dashboard {} }
}

/// Settings page with Integrations section
#[component]
fn Settings() -> Element {
    // Get auth state for integrations
    let (auth_state, set_auth) = use_auth();

    rsx! {
        div { class: "page settings-page",
            h2 { "Settings" }

            // Integrations section
            section { class: "settings-section",
                h3 { "Integrations" }
                p { class: "section-intro",
                    "Connect Tuesday to external services to enable features."
                }

                div { class: "integrations-list",
                    GitHubIntegration {
                        auth_state,
                        on_auth_change: move |new_state| {
                            set_auth.call(new_state);
                        },
                    }

                    GiteaIntegration {}

                    // Placeholder for Claude integration
                    div { class: "integration-card coming-soon",
                        div { class: "integration-header",
                            div { class: "integration-icon", "\u{1F916}" } // Robot emoji
                            div { class: "integration-info",
                                div { class: "integration-name", "Claude AI" }
                                div { class: "integration-description", "Enable AI-powered chat assistant" }
                            }
                            div { class: "integration-status-area",
                                span { class: "coming-soon-badge", "Coming Soon" }
                            }
                        }
                    }

                    // Placeholder for future integrations
                    div { class: "integration-card coming-soon",
                        div { class: "integration-header",
                            div { class: "integration-icon", "\u{1F92E}" } // Vomit emoji
                            div { class: "integration-info",
                                div { class: "integration-name", "Jira" }
                                div { class: "integration-description", "Sync issues and track time against tickets" }
                            }
                            div { class: "integration-status-area",
                                span { class: "coming-soon-badge", "Coming Never" }
                            }
                        }
                    }
                }
            }

            // Other settings sections (placeholders)
            section { class: "settings-section",
                h3 { "Preferences" }
                p { class: "section-intro", "User preferences coming soon." }
            }
        }
    }
}

/// OAuth callback handler page (outside shell)
#[component]
fn AuthCallbackPage() -> Element {
    let navigator = use_navigator();

    rsx! {
        div { class: "auth-callback-page",
            AuthCallback {
                on_complete: move |result: Result<auth::AuthState, String>| {
                    match &result {
                        Ok(_) => tracing::info!("OAuth authentication successful"),
                        Err(e) => tracing::error!("OAuth authentication failed: {}", e),
                    }
                    // Navigate home regardless of success/failure
                    navigator.push(Route::Home {});
                }
            }
        }
    }
}

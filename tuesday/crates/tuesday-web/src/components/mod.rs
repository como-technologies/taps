pub mod auth_callback;
pub mod auth_panel;
pub mod config;
pub mod effort_config;
pub mod github_token_input;
pub mod integrations;
pub mod org_selector;
pub mod repo_selector;
pub mod report;
pub mod shell;
pub mod team_selector;
pub mod time_period_config;
pub mod work_source_config;

// Re-export shell components for easy access
pub use shell::AppShell;

// Re-export integration components
pub use integrations::{GitHubIntegration, GiteaIntegration};

use crate::auth::{AuthState, GiteaSettings, GiteaSettingsStorage, forge_credentials};
use crate::hooks::use_auth;
use config::ConfigPanel;
use dioxus::prelude::*;
use tuesday_core::{ReportConfig, generate_report};

/// View state enum that tracks UI state (not data)
#[derive(Clone, Copy, PartialEq)]
enum ViewState {
    Configuration,
    Loading,
    Report,
}

/// Main dashboard component that orchestrates configuration, generation, and display
#[component]
pub fn Dashboard() -> Element {
    // Auth state with localStorage persistence and auto-refresh
    let (auth_state, set_auth) = use_auth();

    // Configuration state
    let mut config = use_signal(ReportConfig::default);

    // Gitea card settings (Settings page) — loaded after hydration
    let mut gitea_settings = use_signal(GiteaSettings::default);
    use_effect(move || {
        if let Ok(settings) = GiteaSettingsStorage::load() {
            gitea_settings.set(settings);
        }
    });

    // Feed the selected forge's credentials into the config whenever the
    // forge, the GitHub auth state, or the stored Gitea settings change
    use_effect(move || {
        let auth_token = auth_state().token().map(|s| s.to_string());
        let settings = gitea_settings();
        let mut cfg = config();
        let (token, base_url) = forge_credentials(cfg.source, auth_token.as_deref(), &settings);
        if cfg.token != token || cfg.base_url != base_url {
            cfg.token = token;
            cfg.base_url = base_url;
            config.set(cfg);
        }
    });

    // View state is the single source of truth for UI
    let mut view_state = use_signal(|| ViewState::Configuration);

    // Resource that watches config and regenerates when config changes
    let report_resource = use_resource(move || {
        let cfg = config();
        async move { generate_report(cfg).await }
    });

    // Effect to auto-transition from Loading to Report when resource completes
    use_effect(move || {
        if view_state() == ViewState::Loading {
            match &*report_resource.read_unchecked() {
                Some(Ok(_)) => {
                    view_state.set(ViewState::Report);
                }
                Some(Err(_)) => {
                    // Stay in Loading state to show error
                }
                None => {
                    // Still loading
                }
            }
        }
    });

    rsx! {
        div { class: "dashboard",
            h1 { "Tuesday Effort Analysis" }

            // Top button area - show either Generate or Configure button
            match view_state() {
                ViewState::Configuration => rsx! {
                    button {
                        class: "generate-btn",
                        onclick: move |_| {
                            view_state.set(ViewState::Loading);
                        },
                        "Generate Report"
                    }
                },
                ViewState::Loading => rsx! {
                    button { class: "generate-btn", disabled: true, "Generating..." }
                },
                ViewState::Report => rsx! {
                    button {
                        class: "generate-btn",
                        onclick: move |_| {
                            view_state.set(ViewState::Configuration);
                        },
                        "Configure"
                    }
                },
            }

            // Main content area - show either ConfigPanel or ReportView (never both)
            match view_state() {
                ViewState::Configuration => rsx! {
                    ConfigPanel {
                        config,
                        auth_state,
                        on_auth_change: move |new_state: AuthState| {
                            set_auth.call(new_state);
                        },
                    }
                    div { class: "placeholder",
                        p { "Configure your settings and click 'Generate Report' to begin" }
                    }
                },
                ViewState::Loading => {
                    // Check resource status
                    match &*report_resource.read_unchecked() {
                        Some(Ok(_)) => rsx! {
                            div { class: "loading", "Report ready!" }
                        },
                        Some(Err(e)) => rsx! {
                            div { class: "error",
                                p { "Error generating report: {e}" }
                                button { onclick: move |_| view_state.set(ViewState::Configuration), "Back to Configuration" }
                            }
                        },
                        None => rsx! {
                            div { class: "loading", "Loading report..." }
                        },
                    }
                }
                ViewState::Report => {
                    match &*report_resource.read_unchecked() {
                        Some(Ok(report_data)) => rsx! {
                            report::ReportView { report: report_data.clone() }
                        },
                        _ => rsx! {
                            div { class: "error", "Report data unavailable" }
                        },
                    }
                }
            }
        }
    }
}

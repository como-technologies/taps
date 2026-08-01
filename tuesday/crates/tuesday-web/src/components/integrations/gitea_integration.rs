use super::{IntegrationCard, IntegrationStatus};
use crate::auth::{GiteaSettings, GiteaSettingsStorage};
use dioxus::prelude::*;
use tuesday_core::DEFAULT_GITEA_BASE_URL;

/// Gitea integration card for the Settings page: instance base URL plus a
/// pasted API token. Deliberately no OAuth — token paste matches the CLI's
/// contract-pinned token handling; OAuth stays a GitHub-app concern.
#[component]
pub fn GiteaIntegration() -> Element {
    let mut settings = use_signal(GiteaSettings::default);
    let mut base_url_value = use_signal(|| DEFAULT_GITEA_BASE_URL.to_string());
    let mut token_value = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);

    // Load persisted settings after hydration (client-side only)
    use_effect(move || {
        if let Ok(stored) = GiteaSettingsStorage::load() {
            base_url_value.set(stored.base_url.clone());
            settings.set(stored);
        }
    });

    let status = if settings().is_connected() {
        IntegrationStatus::Connected
    } else {
        IntegrationStatus::Disconnected
    };

    rsx! {
        IntegrationCard {
            name: "Gitea",
            description: "Read merged pull requests from a self-hosted Gitea instance",
            icon: "\u{1F375}", // Teacup (Gitea mascot's beverage)
            status,
            account_name: status
                .is_connected()
                .then(|| settings().base_url.clone()),

            if status.is_connected() {
                div { class: "integration-connected",
                    p { class: "connected-info",
                        "Tuesday reads merged PRs from "
                        code { {settings().base_url.clone()} }
                        " with the stored token. Pick the Gitea forge on the Reports page to use it."
                    }
                    button {
                        class: "disconnect-btn",
                        onclick: move |_| {
                            if let Err(e) = GiteaSettingsStorage::clear() {
                                tracing::warn!("Failed to clear Gitea settings: {}", e);
                            }
                            token_value.set(String::new());
                            settings.set(GiteaSettings {
                                base_url: base_url_value(),
                                token: String::new(),
                            });
                        },
                        "Disconnect"
                    }
                }
            } else {
                div { class: "integration-auth",
                    if let Some(err) = error() {
                        div { class: "auth-error", "{err}" }
                    }

                    p { class: "auth-info",
                        "Enter the instance URL and an API token "
                        "(Settings \u{2192} Applications \u{2192} Generate New Token on your Gitea). "
                        "Read access to the repositories is enough — Measure never writes."
                    }
                    div { class: "pat-input-group",
                        input {
                            r#type: "text",
                            class: "pat-input",
                            placeholder: "{DEFAULT_GITEA_BASE_URL}",
                            value: "{base_url_value}",
                            oninput: move |evt| {
                                base_url_value.set(evt.value().clone());
                            },
                        }
                        input {
                            r#type: "password",
                            class: "pat-input",
                            placeholder: "Gitea API token",
                            value: "{token_value}",
                            oninput: move |evt| {
                                token_value.set(evt.value().clone());
                            },
                        }
                        button {
                            class: "connect-btn",
                            disabled: token_value().is_empty(),
                            onclick: move |_| {
                                let base_url = base_url_value();
                                let new_settings = GiteaSettings {
                                    base_url: if base_url.trim().is_empty() {
                                        DEFAULT_GITEA_BASE_URL.to_string()
                                    } else {
                                        base_url.trim().trim_end_matches('/').to_string()
                                    },
                                    token: token_value(),
                                };
                                if let Err(e) = GiteaSettingsStorage::save(&new_settings) {
                                    error.set(Some(format!("Failed to save settings: {e}")));
                                } else {
                                    error.set(None);
                                    base_url_value.set(new_settings.base_url.clone());
                                    settings.set(new_settings);
                                }
                            },
                            "Connect"
                        }
                    }
                }
            }
        }
    }
}

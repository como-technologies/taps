use dioxus::prelude::*;

/// Status of an integration connection
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IntegrationStatus {
    Connected,
    Disconnected,
}

impl IntegrationStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, IntegrationStatus::Connected)
    }

    pub fn status_text(&self) -> &'static str {
        match self {
            IntegrationStatus::Connected => "Connected",
            IntegrationStatus::Disconnected => "Not connected",
        }
    }

    pub fn status_class(&self) -> &'static str {
        match self {
            IntegrationStatus::Connected => "status-connected",
            IntegrationStatus::Disconnected => "status-disconnected",
        }
    }
}

/// Reusable integration card component
#[component]
pub fn IntegrationCard(
    /// Integration name (e.g., "GitHub", "Claude")
    name: &'static str,
    /// Short description of what this integration does
    description: &'static str,
    /// Icon/emoji for the integration
    icon: &'static str,
    /// Current connection status
    status: IntegrationStatus,
    /// Optional: Connected account name/identifier
    account_name: Option<String>,
    /// Content to show when expanded (auth forms, config, etc.)
    children: Element,
) -> Element {
    let mut expanded = use_signal(|| false);

    rsx! {
        div { class: "integration-card",
            // Card header - always visible
            div {
                class: "integration-header",
                onclick: move |_| expanded.toggle(),

                div { class: "integration-icon", "{icon}" }

                div { class: "integration-info",
                    div { class: "integration-name", "{name}" }
                    div { class: "integration-description", "{description}" }
                }

                div { class: "integration-status-area",
                    // Status indicator
                    div { class: format!("integration-status {}", status.status_class()),
                        span { class: "status-dot" }
                        span { class: "status-text", "{status.status_text()}" }
                    }

                    // Account name if connected
                    if let Some(account) = &account_name {
                        if status.is_connected() {
                            span { class: "account-name", "{account}" }
                        }
                    }

                    // Expand/collapse arrow
                    span { class: if expanded() { "expand-arrow expanded" } else { "expand-arrow" },
                        "\u{25BC}"
                    }
                }
            }

            // Expandable content area
            if expanded() {
                div { class: "integration-content",
                    {children}
                }
            }
        }
    }
}

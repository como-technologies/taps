use dioxus::prelude::*;

/// Sidebar navigation component
#[component]
pub fn Sidebar(collapsed: Signal<bool>) -> Element {
    rsx! {
        nav {
            class: if collapsed() { "sidebar collapsed" } else { "sidebar" },

            // Brand/logo area
            div { class: "sidebar-brand",
                if collapsed() {
                    span { class: "brand-icon", "T" }
                } else {
                    span { class: "brand-text", "Tuesday" }
                }
            }

            // Navigation items
            div { class: "sidebar-nav",
                NavItem {
                    to: "/",
                    icon: "\u{1F3E0}", // Home emoji
                    label: "Home",
                    collapsed: collapsed(),
                }
                NavItem {
                    to: "/reports",
                    icon: "\u{1F4CA}", // Chart emoji
                    label: "Reports",
                    collapsed: collapsed(),
                }
                NavItem {
                    to: "/settings",
                    icon: "\u{2699}\u{FE0F}", // Gear emoji
                    label: "Settings",
                    collapsed: collapsed(),
                }
            }

            // Collapse toggle at bottom
            div { class: "sidebar-footer",
                button {
                    class: "sidebar-toggle",
                    onclick: move |_| collapsed.toggle(),
                    if collapsed() {
                        "\u{25B6}" // Right arrow
                    } else {
                        "\u{25C0}" // Left arrow
                    }
                }
            }
        }
    }
}

/// Individual navigation item
#[component]
fn NavItem(to: &'static str, icon: &'static str, label: &'static str, collapsed: bool) -> Element {
    // Get current path to determine active state
    let current_path = use_current_path();
    let is_active = is_path_active(&current_path, to);

    rsx! {
        Link {
            to,
            class: if is_active { "nav-item active" } else { "nav-item" },
            span { class: "nav-icon", "{icon}" }
            if !collapsed {
                span { class: "nav-label", "{label}" }
            }
        }
    }
}

/// Get the current URL path
fn use_current_path() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().pathname().ok())
            .unwrap_or_else(|| "/".to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        "/".to_string()
    }
}

/// Check if path matches current route (handles exact and prefix matching)
fn is_path_active(current: &str, target: &str) -> bool {
    if target == "/" {
        current == "/"
    } else {
        current == target || current.starts_with(&format!("{}/", target))
    }
}

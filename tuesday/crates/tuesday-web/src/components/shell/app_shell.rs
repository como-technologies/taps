use super::{Header, Sidebar};
use crate::hooks::use_auth;
use dioxus::prelude::*;

/// Main application shell layout
/// Provides sidebar navigation, header, and main content area
#[component]
pub fn AppShell() -> Element {
    // Auth state with localStorage persistence
    let (auth_state, _set_auth) = use_auth();

    // Sidebar collapsed state (persisted in localStorage for convenience)
    let sidebar_collapsed = use_signal(load_sidebar_collapsed);

    // Save collapsed state when it changes
    use_effect(move || {
        save_sidebar_collapsed(sidebar_collapsed());
    });

    rsx! {
        div { class: "app-shell",
            Sidebar { collapsed: sidebar_collapsed }

            div {
                class: if sidebar_collapsed() { "main-area sidebar-collapsed" } else { "main-area" },

                Header { auth_state }

                main { class: "main-content",
                    // Outlet renders the matched child route
                    Outlet::<crate::Route> {}
                }
            }
        }
    }
}

/// Load sidebar collapsed state from localStorage
fn load_sidebar_collapsed() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item("sidebar_collapsed").ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Save sidebar collapsed state to localStorage
fn save_sidebar_collapsed(collapsed: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.set_item(
                "sidebar_collapsed",
                if collapsed { "true" } else { "false" },
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = collapsed;
    }
}

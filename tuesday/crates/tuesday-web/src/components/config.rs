use crate::auth::AuthState;
use crate::components::auth_panel::AuthPanel;
use crate::components::effort_config::EffortConfig;
use crate::components::time_period_config::TimePeriodConfig;
use crate::components::work_source_config::WorkSourceConfig;
use dioxus::prelude::*;
use tuesday_core::ReportConfig;

/// Pure configuration panel component that only handles configuration editing
#[component]
pub fn ConfigPanel(
    config: Signal<ReportConfig>,
    auth_state: Signal<AuthState>,
    on_auth_change: EventHandler<AuthState>,
) -> Element {
    rsx! {
        div { class: "config-panel",
            h2 { "Configuration" }

            AuthPanel {
                auth_state,
                on_auth_change,
            }

            WorkSourceConfig { config }

            TimePeriodConfig { config }

            EffortConfig { config }
        }
    }
}

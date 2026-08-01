use dioxus::prelude::*;

#[component]
pub fn GitHubTokenInput(
    value: String,
    on_change: EventHandler<String>,
    on_focus: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "form-group",
            label { "GitHub Token:" }
            input {
                r#type: "password",
                value: "{value}",
                placeholder: "ghp_...",
                oninput: move |evt| {
                    on_change.call(evt.value().clone());
                },
                onfocus: move |_| {
                    on_focus.call(());
                },
            }
        }
    }
}

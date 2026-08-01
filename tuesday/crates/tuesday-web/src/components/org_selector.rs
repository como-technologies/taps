use dioxus::prelude::*;

#[component]
pub fn OrgSelector(
    value: String,
    options: Option<Vec<String>>,
    placeholder: String,
    on_change: EventHandler<String>,
    on_focus: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "form-group",
            label { "Organization:" }
            match options {
                None => rsx! {
                    input {
                        r#type: "text",
                        value: "{value}",
                        placeholder: "{placeholder}",
                        oninput: move |evt| {
                            on_change.call(evt.value().clone());
                        },
                        onfocus: move |_| {
                            if let Some(handler) = &on_focus {
                                handler.call(());
                            }
                        },
                    }
                },
                Some(ref orgs) if orgs.is_empty() => rsx! {
                    input {
                        r#type: "text",
                        value: "{value}",
                        placeholder: "{placeholder}",
                        oninput: move |evt| {
                            on_change.call(evt.value().clone());
                        },
                        onfocus: move |_| {
                            if let Some(handler) = &on_focus {
                                handler.call(());
                            }
                        },
                    }
                },
                Some(orgs) => rsx! {
                    select {
                        value: "{value}",
                        onchange: move |evt| {
                            on_change.call(evt.value().clone());
                        },
                        option { value: "", "Select an organization" }
                        for org in orgs {
                            option { value: "{org}", "{org}" }
                        }
                    }
                },
            }
        }
    }
}

use dioxus::prelude::*;
use tuesday_core::github::GitHubTeam;

#[component]
pub fn TeamSelector(
    value: String,
    options: Option<Vec<GitHubTeam>>,
    placeholder: String,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "form-group",
            label { "Team:" }
            match options {
                None => rsx! {
                    input {
                        r#type: "text",
                        value: "{value}",
                        placeholder: "{placeholder}",
                        oninput: move |evt| {
                            on_change.call(evt.value().clone());
                        },
                    }
                },
                Some(ref teams) if teams.is_empty() => rsx! {
                    input {
                        r#type: "text",
                        value: "{value}",
                        placeholder: "{placeholder}",
                        oninput: move |evt| {
                            on_change.call(evt.value().clone());
                        },
                    }
                },
                Some(teams) => rsx! {
                    select {
                        value: "{value}",
                        onchange: move |evt| {
                            on_change.call(evt.value().clone());
                        },
                        option { value: "", "Select a team" }
                        for team in teams {
                            option { value: "{team.slug}", "{team.name}" }
                        }
                    }
                },
            }
        }
    }
}

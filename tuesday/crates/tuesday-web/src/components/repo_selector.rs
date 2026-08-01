use dioxus::prelude::*;

#[component]
pub fn RepoSelector(
    value: ReadSignal<Vec<String>>,
    options: Option<Vec<String>>,
    placeholder: String,
    on_change: EventHandler<Vec<String>>,
    on_focus: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "form-group",
            label { "Repositories:" }

            match options {
                None => rsx! {
                    input {
                        r#type: "text",
                        placeholder: "{placeholder}",
                        onfocus: move |_| {
                            if let Some(handler) = &on_focus {
                                handler.call(());
                            }
                        },
                    }
                },
                Some(ref repos) if repos.is_empty() => rsx! {
                    input {
                        r#type: "text",
                        placeholder: "{placeholder}",
                        onfocus: move |_| {
                            if let Some(handler) = &on_focus {
                                handler.call(());
                            }
                        },
                    }
                },
                Some(repos) => rsx! {
                    // Selected repos section
                    if !value().is_empty() {
                        div { class: "repo-section",
                            div { class: "repo-section-label", "Selected:" }
                            div { class: "repo-chips",
                                for repo_name in value().clone() {
                                    {
                                        let repo_to_toggle = repo_name.clone();
                                        rsx! {
                                            span {
                                                class: "repo-chip selected",
                                                onclick: move |_| {
                                                    let mut new_value = value();
                                                    new_value.retain(|r| r != &repo_to_toggle);
                                                    on_change.call(new_value);
                                                },
                                                "{repo_name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Available repos section






                    {
                        let available_repos: Vec<_> = repos
                            .iter()
                            .filter(|repo| !value().contains(repo))
                            .collect();

                        if !available_repos.is_empty() {
                            rsx! {
                                div { class: "repo-section",
                                    div { class: "repo-section-label", "Available:" }
                                    div { class: "repo-chips",
                                        for repo in available_repos {
                                            {
                                                let repo_to_toggle = repo.clone();
                                                rsx! {
                                                    span {
                                                        class: "repo-chip available",
                                                        onclick: move |_| {
                                                            let mut new_value = value();
                                                            new_value.push(repo_to_toggle.clone());
                                                            on_change.call(new_value);
                                                        },
                                                        "{repo}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                },
            }
        }
    }
}

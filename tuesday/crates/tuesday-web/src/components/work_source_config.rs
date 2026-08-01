use crate::components::org_selector::OrgSelector;
use crate::components::repo_selector::RepoSelector;
use crate::hooks::{use_forge_orgs, use_forge_repos};
use dioxus::prelude::*;
use tuesday_core::{DEFAULT_GITEA_BASE_URL, ReportConfig, SourceKind};

#[component]
pub fn WorkSourceConfig(config: Signal<ReportConfig>) -> Element {
    // Derive a signal to prevent unnecessary refetches
    let repositories = use_memo(move || config().repositories.clone());

    // Org/repo pickers go through the PrSource seam for both forges
    let (orgs, orgs_loading) = use_forge_orgs(config);
    let (repos, repos_loading) = use_forge_repos(config);

    let source = config().source;

    rsx! {
        div {
            h3 { "Work Source" }

            div { class: "form-group",
                label { "Forge:" }
                select {
                    value: match source {
                        SourceKind::Github => "github",
                        SourceKind::Gitea => "gitea",
                    },
                    onchange: move |evt| {
                        let kind = match evt.value().as_str() {
                            "gitea" => SourceKind::Gitea,
                            _ => SourceKind::Github,
                        };
                        let mut cfg = config();
                        if cfg.source != kind {
                            cfg.source = kind;
                            // Selections belong to the previous forge
                            cfg.organization.clear();
                            cfg.repositories.clear();
                            config.set(cfg);
                        }
                    },
                    option { value: "github", "GitHub" }
                    option { value: "gitea", "Gitea" }
                }
            }

            if source == SourceKind::Gitea {
                p { class: "forge-hint",
                    "Gitea instance: "
                    code {
                        {config().base_url.unwrap_or_else(|| DEFAULT_GITEA_BASE_URL.to_string())}
                    }
                    " — set the URL and token on the Settings page."
                }
            }

            // Show loading state at container level if desired
            if orgs_loading {
                div { class: "loading", "Loading organizations..." }
            }

            OrgSelector {
                value: config().organization.clone(),
                options: orgs,
                placeholder: "Enter token first or type org name".to_string(),
                on_change: move |new_org| {
                    let mut cfg = config();
                    cfg.organization = new_org;
                    cfg.repositories.clear(); // Reset repos when org changes
                    config.set(cfg);
                },
                on_focus: None, // No need for focus tricks anymore
            }

            if repos_loading {
                div { class: "loading", "Loading repositories..." }
            }

            RepoSelector {
                value: repositories,
                options: repos,
                placeholder: "Select org first or type repo name".to_string(),
                on_change: move |new_repos| {
                    let mut cfg = config();
                    cfg.repositories = new_repos;
                    config.set(cfg);
                },
                on_focus: None,
            }
        }
    }
}

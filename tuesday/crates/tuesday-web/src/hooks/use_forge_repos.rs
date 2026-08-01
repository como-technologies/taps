use dioxus::prelude::*;
use tuesday_core::{ForgeSource, PrSource, ReportConfig};

/// Hook fetching the repositories of the configured organization on the
/// configured forge — GitHub or Gitea, through the `PrSource` seam. The
/// `None` / `Some(vec![])` split mirrors `use_forge_orgs`.
pub fn use_forge_repos(config: Signal<ReportConfig>) -> (Option<Vec<String>>, bool) {
    let mut repos = use_signal(Option::<Vec<String>>::default);
    let mut loading = use_signal(bool::default);
    let mut last_key = use_signal(String::new);

    use_effect(move || {
        let cfg = config();
        let key = format!(
            "{:?}|{}|{}|{}",
            cfg.source,
            cfg.base_url.as_deref().unwrap_or(""),
            cfg.token,
            cfg.organization
        );

        // Reset if the connection identity or the organization changed
        if key != last_key() {
            repos.set(None);
            last_key.set(key);
        }

        if !cfg.token.is_empty() && !cfg.organization.is_empty() && repos().is_none() {
            let mut repos = repos;
            let mut loading = loading;
            spawn(async move {
                loading.set(true);
                let fetched = match ForgeSource::from_config(&cfg) {
                    Ok(source) => source.list_repos(&cfg.organization).await,
                    Err(e) => Err(e.into()),
                };
                match fetched {
                    Ok(names) => {
                        tracing::info!(
                            "Fetched {} repositories for {}",
                            names.len(),
                            cfg.organization
                        );
                        repos.set(Some(names));
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch repositories: {}", e);
                        repos.set(Some(vec![])); // Empty = tried but failed
                    }
                }
                loading.set(false);
            });
        } else if cfg.token.is_empty() || cfg.organization.is_empty() {
            repos.set(None); // Reset when the token or org is cleared
            loading.set(false);
        }
    });

    (repos(), loading())
}

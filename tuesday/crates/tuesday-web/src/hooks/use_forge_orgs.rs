use dioxus::prelude::*;
use tuesday_core::{ForgeSource, PrSource, ReportConfig};

/// Identity of the connection a fetch depends on; when it changes, cached
/// results are stale.
fn connection_key(cfg: &ReportConfig) -> String {
    format!(
        "{:?}|{}|{}",
        cfg.source,
        cfg.base_url.as_deref().unwrap_or(""),
        cfg.token
    )
}

/// Hook fetching the organizations visible on the configured forge —
/// GitHub or Gitea, through the `PrSource` seam. `None` means "not tried"
/// (no token yet); `Some(vec![])` means tried and got nothing usable, so
/// the selector falls back to free-text entry.
pub fn use_forge_orgs(config: Signal<ReportConfig>) -> (Option<Vec<String>>, bool) {
    let mut orgs = use_signal(Option::<Vec<String>>::default);
    let mut loading = use_signal(bool::default);
    let mut last_key = use_signal(String::new);

    use_effect(move || {
        let cfg = config();
        let key = connection_key(&cfg);

        // Reset if the connection identity changed
        if key != last_key() {
            orgs.set(None);
            last_key.set(key);
        }

        // Listing orgs is an authenticated call on both forges.
        if !cfg.token.is_empty() && orgs().is_none() {
            let mut orgs = orgs;
            let mut loading = loading;
            spawn(async move {
                loading.set(true);
                let fetched = match ForgeSource::from_config(&cfg) {
                    Ok(source) => source.list_orgs().await,
                    Err(e) => Err(e.into()),
                };
                match fetched {
                    Ok(names) => {
                        tracing::info!("Fetched {} organizations", names.len());
                        orgs.set(Some(names));
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch organizations: {}", e);
                        orgs.set(Some(vec![])); // Empty = tried but failed
                    }
                }
                loading.set(false);
            });
        } else if cfg.token.is_empty() {
            orgs.set(None); // Reset when the token is cleared
            loading.set(false);
        }
    });

    (orgs(), loading())
}

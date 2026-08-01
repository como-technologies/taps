//! Gitea connection settings: base URL + pasted token, persisted in
//! localStorage like the GitHub auth state. Deliberately no OAuth — the
//! token-paste path matches the CLI's contract-pinned token handling;
//! OAuth stays a GitHub-app concern.

use serde::{Deserialize, Serialize};
use tuesday_core::DEFAULT_GITEA_BASE_URL;
use tuesday_core::SourceKind;

/// What the Gitea integration card collects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GiteaSettings {
    /// Instance base URL, e.g. `http://localhost:3000`.
    pub base_url: String,
    /// API token (may stay empty: Gitea reads anonymously).
    pub token: String,
}

impl Default for GiteaSettings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_GITEA_BASE_URL.to_string(),
            token: String::new(),
        }
    }
}

impl GiteaSettings {
    /// The card shows "Connected" once a token is stored.
    pub fn is_connected(&self) -> bool {
        !self.token.is_empty()
    }
}

/// The token + base_url pair the dashboard feeds into `ReportConfig` for
/// the selected forge: GitHub takes the auth-state token (no base URL,
/// GitHub's API base is fixed); Gitea takes the stored card settings.
pub fn forge_credentials(
    source: SourceKind,
    github_token: Option<&str>,
    gitea: &GiteaSettings,
) -> (String, Option<String>) {
    match source {
        SourceKind::Github => (github_token.unwrap_or_default().to_string(), None),
        SourceKind::Gitea => (
            gitea.token.clone(),
            Some(gitea.base_url.clone()).filter(|url| !url.is_empty()),
        ),
    }
}

#[cfg(target_arch = "wasm32")]
const GITEA_STORAGE_KEY: &str = "tuesday_gitea_settings";

/// LocalStorage wrapper for persisting the Gitea settings (mirrors
/// `TokenStorage`; a no-op returning defaults on the server).
pub struct GiteaSettingsStorage;

impl GiteaSettingsStorage {
    pub fn save(settings: &GiteaSettings) -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;

            let window = window().ok_or("No window object")?;
            let storage = window
                .local_storage()
                .map_err(|_| "Failed to access localStorage")?
                .ok_or("localStorage not available")?;

            let json = serde_json::to_string(settings)
                .map_err(|e| format!("Serialization error: {}", e))?;
            storage
                .set_item(GITEA_STORAGE_KEY, &json)
                .map_err(|_| "Failed to save to localStorage")?;
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = settings;
            tracing::debug!("localStorage not available on server");
            Ok(())
        }
    }

    pub fn load() -> Result<GiteaSettings, String> {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;

            let window = window().ok_or("No window object")?;
            let storage = window
                .local_storage()
                .map_err(|_| "Failed to access localStorage")?
                .ok_or("localStorage not available")?;

            match storage
                .get_item(GITEA_STORAGE_KEY)
                .map_err(|_| "Failed to read from localStorage")?
            {
                Some(data) => {
                    serde_json::from_str(&data).map_err(|e| format!("Deserialization error: {}", e))
                }
                None => Ok(GiteaSettings::default()),
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            tracing::debug!("localStorage not available on server");
            Ok(GiteaSettings::default())
        }
    }

    pub fn clear() -> Result<(), String> {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;

            let window = window().ok_or("No window object")?;
            let storage = window
                .local_storage()
                .map_err(|_| "Failed to access localStorage")?
                .ok_or("localStorage not available")?;

            storage
                .remove_item(GITEA_STORAGE_KEY)
                .map_err(|_| "Failed to clear localStorage")?;
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_point_at_the_dogfood_forge_disconnected() {
        let settings = GiteaSettings::default();
        assert_eq!(settings.base_url, "http://localhost:3000");
        assert!(settings.token.is_empty());
        assert!(!settings.is_connected());
    }

    #[test]
    fn a_stored_token_means_connected() {
        let settings = GiteaSettings {
            token: "tok".to_string(),
            ..GiteaSettings::default()
        };
        assert!(settings.is_connected());
    }

    #[test]
    fn settings_roundtrip_through_the_storage_serde_shape() {
        let settings = GiteaSettings {
            base_url: "http://gitea.internal:3000".to_string(),
            token: "tok".to_string(),
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<GiteaSettings>(&json).unwrap(),
            settings
        );
    }

    #[test]
    fn github_takes_the_auth_state_token_and_no_base_url() {
        let gitea = GiteaSettings {
            token: "gitea-tok".to_string(),
            ..GiteaSettings::default()
        };
        assert_eq!(
            forge_credentials(SourceKind::Github, Some("gh-tok"), &gitea),
            ("gh-tok".to_string(), None)
        );
        assert_eq!(
            forge_credentials(SourceKind::Github, None, &gitea),
            (String::new(), None)
        );
    }

    #[test]
    fn gitea_takes_the_card_settings() {
        let gitea = GiteaSettings {
            base_url: "http://localhost:3000".to_string(),
            token: "gitea-tok".to_string(),
        };
        assert_eq!(
            forge_credentials(SourceKind::Gitea, Some("gh-tok"), &gitea),
            (
                "gitea-tok".to_string(),
                Some("http://localhost:3000".to_string())
            )
        );
    }

    #[test]
    fn an_empty_stored_base_url_falls_back_to_the_core_default() {
        // None lets core's ForgeSource::from_config supply the documented
        // default rather than sending an empty URL.
        let gitea = GiteaSettings {
            base_url: String::new(),
            token: "tok".to_string(),
        };
        assert_eq!(
            forge_credentials(SourceKind::Gitea, None, &gitea),
            ("tok".to_string(), None)
        );
    }
}

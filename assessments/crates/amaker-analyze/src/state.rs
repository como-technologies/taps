//! Analyst-binary state. Read-only — no project locks needed.

use std::sync::Arc;

use amaker_core::services::{ResponseService, StorageService};

use crate::config::Config;

pub struct AppState {
    pub storage: StorageService,
    pub responses: ResponseService,
    /// "Back to authoring" link + regenerate-narrative POST target.
    pub author_base_url: String,
    /// "Edit response" link.
    pub assess_base_url: String,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        let store = amaker_core::build_store(&config.storage_backend)?;
        let storage = StorageService::new(store);
        storage.init().await?;
        let responses = ResponseService::new(storage.clone());
        Ok(Arc::new(Self {
            storage,
            responses,
            author_base_url: config.author_base_url,
            assess_base_url: config.assess_base_url,
        }))
    }
}

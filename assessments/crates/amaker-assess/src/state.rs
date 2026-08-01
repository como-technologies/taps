//! Respondent binary state. No LLM, no model picker, no project locks —
//! concurrency is the storage layer's job now (ETag conditional writes).

use std::sync::Arc;

use amaker_core::services::{ResponseService, StorageService};

use crate::config::Config;

pub struct AppState {
    pub storage: StorageService,
    pub responses: ResponseService,
    /// "Back" link target in templates.
    pub author_base_url: String,
    /// "View results" link target.
    pub analyze_base_url: String,
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
            analyze_base_url: config.analyze_base_url,
        }))
    }
}

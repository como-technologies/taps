//! Application state shared across handlers.

use std::sync::Arc;

use crate::config::Config;
use crate::services::AiService;
use amaker_core::AppError;
use amaker_core::models::ModelOption;
use amaker_core::services::{ResponseService, StorageService};

/// Shared application state. The per-process mutex map that previously
/// serialized writes is gone — concurrency is the storage layer's job
/// now, via ETag conditional writes (`load_assessment_yaml_versioned` +
/// `save_assessment_yaml_if_match`).
pub struct AppState {
    /// LLM service (Anthropic via rig)
    pub ai: AiService,
    /// File storage service
    pub storage: StorageService,
    /// Response storage + archive helpers.
    pub responses: ResponseService,
    /// Available models reported by the provider's `/v1/models` endpoint
    pub models: Vec<ModelOption>,
    /// Resolved default model id (env override if listed, else first from `models`)
    pub default_model: String,
    /// Base URL of the assess binary — used to build absolute "Respond"
    /// header links since assess runs in a separate process.
    pub assess_base_url: String,
    /// Base URL of the analyze binary — same for the "Analyze" link and
    /// for the regenerate-narrative POST that analyze's UI sends back here.
    pub analyze_base_url: String,
    /// Public URL of this binary, echoed into the regenerate-narrative
    /// response so analyze's swapped-in HTML keeps pointing back here.
    pub author_public_url: String,
}

impl AppState {
    /// Create new application state. Fetches the model list from the provider
    /// at startup so the UI picker reflects whatever the key has access to.
    pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        let store = amaker_core::build_store(&config.storage_backend)?;
        let storage = StorageService::new(store);
        storage.init().await?;

        // Build the AI client with a placeholder default; it's replaced once
        // we've fetched the real model list below.
        let placeholder = config.ai_model.clone().unwrap_or_default();
        let mut ai = AiService::new(
            &config.anthropic_api_key,
            placeholder,
            config.questions_min_per_practice,
            config.questions_max_per_practice,
            config.question_gen_max_retries,
        )?;

        let models = ai.list_models().await?;
        if models.is_empty() {
            return Err(AppError::Ai(
                "Anthropic /v1/models returned no models for this API key".into(),
            )
            .into());
        }

        let default_model = resolve_default_model(&config.ai_model, &models);
        tracing::info!(
            "Loaded {} models from Anthropic. Default: {}",
            models.len(),
            default_model
        );

        // Rebuild with the resolved default so `AiService::default_model` works
        // without the placeholder.
        ai = AiService::new(
            &config.anthropic_api_key,
            default_model.clone(),
            config.questions_min_per_practice,
            config.questions_max_per_practice,
            config.question_gen_max_retries,
        )?;

        let responses = ResponseService::new(storage.clone());

        Ok(Arc::new(Self {
            ai,
            storage,
            responses,
            models,
            default_model,
            assess_base_url: config.assess_base_url,
            analyze_base_url: config.analyze_base_url,
            author_public_url: config.author_public_url,
        }))
    }
}

/// Pick the default model id. Priority:
/// 1. `AUTHOR_AI_MODEL` env override, if it appears in the list.
/// 2. Newest Sonnet (Anthropic returns models newest-first, so the first
///    sonnet encountered is the most recent — no need to hardcode a version).
/// 3. First model in the list.
fn resolve_default_model(override_id: &Option<String>, models: &[ModelOption]) -> String {
    if let Some(id) = override_id {
        if models.iter().any(|m| &m.value == id) {
            return id.clone();
        }
        tracing::warn!(
            "AUTHOR_AI_MODEL={} not in provider's model list; falling back to a default",
            id,
        );
    }
    if let Some(m) = models.iter().find(|m| m.value.contains("sonnet")) {
        return m.value.clone();
    }
    models[0].value.clone()
}

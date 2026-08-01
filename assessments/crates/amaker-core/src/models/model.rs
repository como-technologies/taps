//! Available LLM model option for the UI picker.
//!
//! The list is populated dynamically from the provider's `/v1/models` endpoint
//! at startup (see `AppState::new`). There is no hardcoded default — if the
//! user sets an env override it must match a model the provider actually
//! returns.

/// A model option for the UI picker.
#[derive(Debug, Clone)]
pub struct ModelOption {
    /// API model identifier (e.g., "claude-sonnet-4-5").
    pub value: String,
    /// Display label (e.g., "Claude Sonnet 4.5").
    pub label: String,
}

/// Default model alias to use when none is specified.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-5";

/// The static Claude model catalog (anthropic provider). The ollama-aware
/// picker (live tags when `AI_PROVIDER=ollama`) lives in `services::models`.
pub fn claude_models() -> Vec<ModelOption> {
    vec![
        ModelOption {
            value: "claude-sonnet-4-5".to_string(),
            label: "Sonnet 4.5".to_string(),
        },
        ModelOption {
            value: "claude-haiku-4-5".to_string(),
            label: "Haiku 4.5".to_string(),
        },
        ModelOption {
            value: "claude-opus-4-5".to_string(),
            label: "Opus 4.5".to_string(),
        },
    ]
}

//! The `LlmProvider` seam: the chat surface every AI backend must offer.
//!
//! A provider exposes exactly one operation — [`LlmProvider::chat`] — which
//! takes a system prompt, the conversation so far, the tool definitions
//! available this turn, a token budget, and an optional per-request model
//! override, and returns a normalized [`ChatResponse`].
//!
//! The server-side tool-execution loop (running tool calls, making follow-up
//! turns) deliberately lives *outside* the provider — in the chat handler and
//! `services::tools` — so every backend gets identical loop behavior.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::config::{AiProvider, Config};
use crate::error::AppError;
use crate::services::anthropic::AnthropicProvider;
use crate::services::ollama::OllamaProvider;
use crate::services::tools::ToolDef;

/// Default token budget for a chat turn when the caller has no special need.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Response from a chat turn, normalized across providers.
#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    /// Concatenated text content from the response.
    pub text: String,
    /// Tool calls requested by the model.
    pub tool_uses: Vec<ToolUseBlock>,
    /// Provider-reported stop reason (e.g. "EndTurn", "ToolUse", "MaxTokens").
    pub stop_reason: String,
    /// Whether the response was truncated by the token budget.
    pub was_truncated: bool,
}

/// A tool call requested by the model.
#[derive(Debug, Clone)]
pub struct ToolUseBlock {
    /// Unique ID for this tool use.
    pub id: String,
    /// Name of the tool to call.
    pub name: String,
    /// Input arguments for the tool.
    pub input: Value,
}

/// The chat surface an AI backend must implement.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send one chat turn and return the normalized response.
    ///
    /// `messages` are `(role, content)` pairs with roles `"user"` and
    /// `"assistant"`. `model_override` selects a provider-specific model for
    /// this request only; `None` uses the provider's configured default.
    async fn chat(
        &self,
        system: &str,
        messages: Vec<(String, String)>,
        tools: Vec<ToolDef>,
        max_tokens: u32,
        model_override: Option<&str>,
    ) -> Result<ChatResponse, AppError>;
}

/// Errors constructing a provider from configuration.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("ANTHROPIC_API_KEY is required when AI_PROVIDER=anthropic")]
    MissingAnthropicKey,

    #[error("failed to construct the Anthropic provider: {0}")]
    Anthropic(String),
}

/// Construct the provider selected by `AI_PROVIDER` in the configuration.
pub fn build_provider(config: &Config) -> Result<Arc<dyn LlmProvider>, ProviderError> {
    match config.ai_provider {
        AiProvider::Anthropic => {
            let api_key = config
                .anthropic_api_key
                .as_deref()
                .ok_or(ProviderError::MissingAnthropicKey)?;
            let provider = AnthropicProvider::new(api_key, config.claude_model.clone())
                .map_err(|e| ProviderError::Anthropic(e.to_string()))?;
            Ok(Arc::new(provider))
        }
        AiProvider::Ollama => Ok(Arc::new(OllamaProvider::new(
            config.ollama_host.clone(),
            config.ollama_model.clone(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(provider: AiProvider, key: Option<&str>) -> Config {
        Config {
            host: "127.0.0.1".to_string(),
            port: 0,
            ai_provider: provider,
            anthropic_api_key: key.map(String::from),
            claude_model: "claude-sonnet-4-5".to_string(),
            ollama_host: "http://localhost:11434".to_string(),
            ollama_model: "llama3.2".to_string(),
            data_dir: "./data".into(),
            rust_log: "info".to_string(),
        }
    }

    #[test]
    fn ollama_arm_constructs_without_any_key() {
        let provider = build_provider(&test_config(AiProvider::Ollama, None));
        assert!(provider.is_ok(), "ollama needs no API key to construct");
    }

    #[test]
    fn anthropic_arm_requires_key() {
        let Err(err) = build_provider(&test_config(AiProvider::Anthropic, None)) else {
            panic!("expected missing-key error");
        };
        assert!(matches!(err, ProviderError::MissingAnthropicKey));
    }

    #[test]
    fn anthropic_arm_constructs_with_key() {
        let provider = build_provider(&test_config(AiProvider::Anthropic, Some("sk-test")));
        assert!(provider.is_ok());
    }
}

/// Scripted [`LlmProvider`] for tests: replays queued responses and records
/// every call it receives so tests can assert on prompt assembly.
///
/// Not `#[cfg(test)]`: integration tests (and CI paths that must run without
/// any AI backend) build real `AppState` around a `FakeProvider`.
pub mod fake {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    /// Everything a [`FakeProvider`] saw for one `chat` call.
    #[derive(Debug, Clone)]
    pub struct RecordedCall {
        pub system: String,
        pub messages: Vec<(String, String)>,
        pub tool_names: Vec<String>,
        pub max_tokens: u32,
        pub model_override: Option<String>,
    }

    /// An [`LlmProvider`] that replays scripted responses in order.
    ///
    /// Errors if the script is exhausted, so tests fail loudly when the code
    /// under test makes more provider calls than expected.
    #[derive(Default)]
    pub struct FakeProvider {
        responses: Mutex<VecDeque<ChatResponse>>,
        calls: Mutex<Vec<RecordedCall>>,
    }

    impl FakeProvider {
        /// Create a provider that replays `responses` in order.
        pub fn new(responses: Vec<ChatResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(Vec::new()),
            }
        }

        /// Build a scripted plain-text response.
        pub fn text(text: &str) -> ChatResponse {
            ChatResponse {
                text: text.to_string(),
                stop_reason: "EndTurn".to_string(),
                ..Default::default()
            }
        }

        /// Build a scripted response containing a single tool call.
        pub fn tool_call(name: &str, input: Value) -> ChatResponse {
            ChatResponse {
                tool_uses: vec![ToolUseBlock {
                    id: format!("toolu_fake_{name}"),
                    name: name.to_string(),
                    input,
                }],
                stop_reason: "ToolUse".to_string(),
                ..Default::default()
            }
        }

        /// All calls made so far, in order.
        pub fn calls(&self) -> Vec<RecordedCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LlmProvider for FakeProvider {
        async fn chat(
            &self,
            system: &str,
            messages: Vec<(String, String)>,
            tools: Vec<ToolDef>,
            max_tokens: u32,
            model_override: Option<&str>,
        ) -> Result<ChatResponse, AppError> {
            self.calls.lock().unwrap().push(RecordedCall {
                system: system.to_string(),
                messages,
                tool_names: tools.into_iter().map(|t| t.name).collect(),
                max_tokens,
                model_override: model_override.map(str::to_string),
            });
            self.responses.lock().unwrap().pop_front().ok_or_else(|| {
                AppError::Internal(
                    "FakeProvider: no scripted response left for this call".to_string(),
                )
            })
        }
    }
}

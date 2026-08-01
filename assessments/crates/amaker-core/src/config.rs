//! Application configuration following 12-factor principles.
//!
//! All configuration is loaded from environment variables.

use std::env;
use std::path::PathBuf;

use crate::models::DEFAULT_MODEL;

/// Which AI backend the [`crate::services::LlmProvider`] seam uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiProvider {
    /// Anthropic Claude via the hosted API (requires `ANTHROPIC_API_KEY`).
    Anthropic,
    /// Local Ollama via its native `/api/chat` endpoint (no API key needed).
    Ollama,
}

impl AiProvider {
    /// Parse an `AI_PROVIDER` value (case-insensitive).
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Some(Self::Anthropic),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Server host address
    pub host: String,
    /// Server port
    pub port: u16,

    /// Which AI provider to use (`AI_PROVIDER`, default: anthropic)
    pub ai_provider: AiProvider,
    /// Anthropic API key; required only when `ai_provider` is Anthropic
    pub anthropic_api_key: Option<String>,
    /// Claude model to use
    pub claude_model: String,
    /// Ollama server base URL (`OLLAMA_HOST`)
    pub ollama_host: String,
    /// Ollama model to use (`OLLAMA_MODEL`)
    pub ollama_model: String,

    /// Directory for storing project data
    pub data_dir: PathBuf,

    /// Log filter (standard RUST_LOG format, e.g., "info", "amaker_core=debug")
    pub rust_log: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Required:
    /// - `ANTHROPIC_API_KEY`: Anthropic API key — only when
    ///   `AI_PROVIDER=anthropic` (the default)
    ///
    /// Optional (with defaults):
    /// - `AI_PROVIDER`: AI backend, `anthropic` or `ollama` (default: anthropic)
    /// - `HOST`: Server host (default: 127.0.0.1)
    /// - `PORT`: Server port (default: 3000)
    /// - `CLAUDE_MODEL`: Model to use (default: claude-sonnet-4-5)
    /// - `OLLAMA_HOST`: Ollama server URL (default: http://localhost:11434)
    /// - `OLLAMA_MODEL`: Ollama model (default: llama3.2)
    /// - `DATA_DIR`: Data directory (default: ./data)
    /// - `RUST_LOG`: Log filter (default: info)
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Load configuration from an environment-shaped lookup function.
    /// Keeps validation testable without touching process-global env vars.
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let ai_provider = match get("AI_PROVIDER") {
            None => AiProvider::Anthropic,
            Some(raw) if raw.trim().is_empty() => AiProvider::Anthropic,
            Some(raw) => AiProvider::parse(&raw).ok_or(ConfigError::InvalidValue("AI_PROVIDER"))?,
        };

        // The key is provider-gated: required for anthropic, optional otherwise,
        // so the app can boot fully locally once another provider is selected.
        let anthropic_api_key = get("ANTHROPIC_API_KEY").filter(|k| !k.trim().is_empty());
        if ai_provider == AiProvider::Anthropic && anthropic_api_key.is_none() {
            return Err(ConfigError::Missing("ANTHROPIC_API_KEY"));
        }

        Ok(Self {
            host: get("HOST").unwrap_or_else(|| "127.0.0.1".to_string()),
            port: get("PORT")
                .unwrap_or_else(|| "3000".to_string())
                .parse()
                .map_err(|_| ConfigError::InvalidValue("PORT"))?,

            ai_provider,
            anthropic_api_key,
            claude_model: get("CLAUDE_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            ollama_host: get("OLLAMA_HOST").unwrap_or_else(|| "http://localhost:11434".to_string()),
            ollama_model: get("OLLAMA_MODEL").unwrap_or_else(|| "llama3.2".to_string()),

            data_dir: PathBuf::from(get("DATA_DIR").unwrap_or_else(|| "./data".to_string())),

            rust_log: get("RUST_LOG").unwrap_or_else(|| "info".to_string()),
        })
    }

    /// Get the server bind address.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    Missing(&'static str),

    #[error("Invalid value for environment variable: {0}")]
    InvalidValue(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Build a Config from a fixed set of "environment" variables.
    fn cfg(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|key| map.get(key).cloned())
    }

    #[test]
    fn default_provider_is_anthropic_and_requires_key() {
        let err = cfg(&[]).unwrap_err();
        assert!(matches!(err, ConfigError::Missing("ANTHROPIC_API_KEY")));
    }

    #[test]
    fn anthropic_provider_with_key_loads() {
        let config = cfg(&[("ANTHROPIC_API_KEY", "sk-test")]).unwrap();
        assert_eq!(config.ai_provider, AiProvider::Anthropic);
        assert_eq!(config.anthropic_api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn ollama_provider_boots_without_anthropic_key() {
        let config = cfg(&[("AI_PROVIDER", "ollama")]).unwrap();
        assert_eq!(config.ai_provider, AiProvider::Ollama);
        assert_eq!(config.anthropic_api_key, None);
    }

    #[test]
    fn ollama_settings_have_local_defaults() {
        let config = cfg(&[("AI_PROVIDER", "ollama")]).unwrap();
        assert_eq!(config.ollama_host, "http://localhost:11434");
        assert_eq!(config.ollama_model, "llama3.2");
    }

    #[test]
    fn ollama_settings_are_overridable() {
        let config = cfg(&[
            ("AI_PROVIDER", "ollama"),
            ("OLLAMA_HOST", "http://gpu-box:11434"),
            ("OLLAMA_MODEL", "qwen2.5"),
        ])
        .unwrap();
        assert_eq!(config.ollama_host, "http://gpu-box:11434");
        assert_eq!(config.ollama_model, "qwen2.5");
    }

    #[test]
    fn blank_anthropic_key_counts_as_missing() {
        let err = cfg(&[("ANTHROPIC_API_KEY", "   ")]).unwrap_err();
        assert!(matches!(err, ConfigError::Missing("ANTHROPIC_API_KEY")));
    }

    #[test]
    fn unknown_provider_is_rejected() {
        let err = cfg(&[("AI_PROVIDER", "skynet"), ("ANTHROPIC_API_KEY", "sk-test")]).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidValue("AI_PROVIDER")));
    }

    #[test]
    fn provider_name_is_case_insensitive() {
        let config = cfg(&[("AI_PROVIDER", "Ollama")]).unwrap();
        assert_eq!(config.ai_provider, AiProvider::Ollama);
    }

    #[test]
    fn empty_provider_value_falls_back_to_default() {
        let config = cfg(&[("AI_PROVIDER", ""), ("ANTHROPIC_API_KEY", "sk-test")]).unwrap();
        assert_eq!(config.ai_provider, AiProvider::Anthropic);
    }
}

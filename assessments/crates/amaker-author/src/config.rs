//! Application configuration following 12-factor principles.
//!
//! All configuration is loaded from environment variables. The author
//! binary's vars are prefixed `AUTHOR_*` so they don't collide with the
//! assess / analyze binaries in a shared `.env` or process environment.
//!
//! `ANTHROPIC_API_KEY` stays unprefixed because it's a third-party
//! provider variable — the `anthropic` SDK looks for it directly and
//! tooling like the `claude` CLI expects it under the same name.

use std::env;

use amaker_core::StorageBackend;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Server host address
    pub host: String,
    /// Server port
    pub port: u16,

    /// Anthropic API key
    pub anthropic_api_key: String,
    /// Optional model override (must match an id returned by the provider's
    /// `/v1/models` endpoint). When unset, the first model returned is used.
    pub ai_model: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// Minimum allowed `target_count` for `generate_questions`. Outer LLM asks
    /// below this are rejected so the orchestrator is forced to correct.
    pub questions_min_per_practice: u8,
    /// Maximum allowed `target_count` for `generate_questions`.
    pub questions_max_per_practice: u8,
    /// How many count-mismatch retries the sub-LLM gets before we give up.
    pub question_gen_max_retries: u8,

    /// Base URL of the assess binary — used to build absolute "Respond"
    /// links from the authoring header so the SME crosses cleanly into
    /// the responder's process.
    pub assess_base_url: String,
    /// Base URL of the analyze binary — same idea for the "Analyze" link.
    pub analyze_base_url: String,
    /// Public URL this binary is reachable at. Echoed back in the
    /// `regenerate_narrative` response so analyze's swapped-in HTML
    /// keeps targeting the right author endpoint on subsequent
    /// regenerate clicks.
    pub author_public_url: String,

    /// Where project data lives. See [`load_storage_backend`].
    pub storage_backend: StorageBackend,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Required:
    /// - `ANTHROPIC_API_KEY`: Your Anthropic API key
    ///
    /// Optional (with defaults):
    /// - `AUTHOR_HOST`: Server host (default: 127.0.0.1 — loopback only; set to
    ///   `0.0.0.0` to expose the server, e.g. inside a container)
    /// - `AUTHOR_PORT` / `PORT`: Server port (default: 3000). `AUTHOR_PORT`
    ///   wins; `PORT` is the fallback Cloud Run and similar platforms inject.
    /// - `AUTHOR_AI_MODEL`: Model id override; must appear in the provider's model list
    /// - `AUTHOR_DATA_DIR`: Filesystem-backend data directory (default: ./data)
    /// - `AUTHOR_LOG_LEVEL`: Log level (default: info)
    /// - `AUTHOR_QUESTIONS_MIN`: Minimum `target_count` (default: 3)
    /// - `AUTHOR_QUESTIONS_MAX`: Maximum `target_count` (default: 12)
    /// - `AUTHOR_QUESTIONS_GEN_RETRIES`: Count-mismatch retry budget (default: 3)
    /// - `AUTHOR_ASSESS_BASE_URL`: Where the assess binary lives (default: http://localhost:3001)
    /// - `AUTHOR_ANALYZE_BASE_URL`: Where the analyze binary lives (default: http://localhost:3002)
    /// - `AUTHOR_PUBLIC_URL`: Where this binary is reachable (default: http://localhost:3000)
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let questions_min_per_practice = parse_u8_env("AUTHOR_QUESTIONS_MIN", 3)?;
        let questions_max_per_practice = parse_u8_env("AUTHOR_QUESTIONS_MAX", 12)?;
        if questions_min_per_practice == 0 {
            return Err(ConfigError::InvalidValue("AUTHOR_QUESTIONS_MIN"));
        }
        if questions_min_per_practice > questions_max_per_practice {
            return Err(ConfigError::InvalidValue("AUTHOR_QUESTIONS_MIN"));
        }

        Ok(Self {
            host: env::var("AUTHOR_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("AUTHOR_PORT")
                .or_else(|_| env::var("PORT"))
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .map_err(|_| ConfigError::InvalidValue("AUTHOR_PORT"))?,

            anthropic_api_key: env::var("ANTHROPIC_API_KEY")
                .map_err(|_| ConfigError::Missing("ANTHROPIC_API_KEY"))?,
            ai_model: env::var("AUTHOR_AI_MODEL").ok().filter(|s| !s.is_empty()),

            log_level: env::var("AUTHOR_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),

            questions_min_per_practice,
            questions_max_per_practice,
            question_gen_max_retries: parse_u8_env("AUTHOR_QUESTIONS_GEN_RETRIES", 3)?,

            assess_base_url: env::var("AUTHOR_ASSESS_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3001".to_string()),
            analyze_base_url: env::var("AUTHOR_ANALYZE_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3002".to_string()),
            author_public_url: env::var("AUTHOR_PUBLIC_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),

            storage_backend: amaker_core::load_from_env("AUTHOR")
                .map_err(|e| ConfigError::Invalid(e.to_string()))?,
        })
    }

    /// Get the server bind address.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Parse a u8-valued environment variable with a default when unset.
fn parse_u8_env(name: &'static str, default: u8) -> Result<u8, ConfigError> {
    match env::var(name) {
        Ok(s) if !s.is_empty() => s.parse().map_err(|_| ConfigError::InvalidValue(name)),
        _ => Ok(default),
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    Missing(&'static str),

    #[error("Invalid value for environment variable: {0}")]
    InvalidValue(&'static str),

    #[error("{0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_u8_env_uses_default_when_unset() {
        // SAFETY: test-only; the env name is unique per test to avoid cross-test
        // interference with single-threaded env access.
        unsafe { env::remove_var("AMAKER_TEST_U8_DEFAULT") };
        assert_eq!(parse_u8_env("AMAKER_TEST_U8_DEFAULT", 7).unwrap(), 7);
    }

    #[test]
    fn parse_u8_env_parses_explicit_value() {
        unsafe { env::set_var("AMAKER_TEST_U8_EXPLICIT", "9") };
        assert_eq!(parse_u8_env("AMAKER_TEST_U8_EXPLICIT", 3).unwrap(), 9);
        unsafe { env::remove_var("AMAKER_TEST_U8_EXPLICIT") };
    }

    #[test]
    fn parse_u8_env_rejects_garbage() {
        unsafe { env::set_var("AMAKER_TEST_U8_BAD", "not-a-number") };
        assert!(matches!(
            parse_u8_env("AMAKER_TEST_U8_BAD", 3),
            Err(ConfigError::InvalidValue(_))
        ));
        unsafe { env::remove_var("AMAKER_TEST_U8_BAD") };
    }
}

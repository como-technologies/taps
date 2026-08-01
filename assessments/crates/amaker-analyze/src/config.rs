//! Analyst-binary config. Reads `ANALYZE_*` env vars only.

use std::env;

use amaker_core::StorageBackend;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    /// Base URL of the author binary — "Back to authoring" link + the
    /// cross-origin POST target for regenerate-narrative.
    pub author_base_url: String,
    /// Base URL of the assess binary — "Edit response" link.
    pub assess_base_url: String,
    pub storage_backend: StorageBackend,
}

impl Config {
    /// Optional (with defaults):
    /// - `ANALYZE_HOST` (default: `127.0.0.1`; set `0.0.0.0` in a container)
    /// - `ANALYZE_PORT` / `PORT` (default: `3002`). `ANALYZE_PORT` wins;
    ///   `PORT` is the fallback Cloud Run and similar platforms inject.
    /// - `ANALYZE_DATA_DIR` (filesystem backend; default: `./data`)
    /// - `ANALYZE_LOG_LEVEL` (default: `info`)
    /// - `ANALYZE_AUTHOR_BASE_URL` (default: `http://localhost:3000`)
    /// - `ANALYZE_ASSESS_BASE_URL` (default: `http://localhost:3001`)
    /// - `ANALYZE_STORAGE_BACKEND` + provider vars (see
    ///   `amaker_core::load_from_env`)
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        Ok(Self {
            host: env::var("ANALYZE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("ANALYZE_PORT")
                .or_else(|_| env::var("PORT"))
                .unwrap_or_else(|_| "3002".to_string())
                .parse()
                .map_err(|_| ConfigError::InvalidValue("ANALYZE_PORT"))?,
            log_level: env::var("ANALYZE_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            author_base_url: env::var("ANALYZE_AUTHOR_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            assess_base_url: env::var("ANALYZE_ASSESS_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3001".to_string()),
            storage_backend: amaker_core::load_from_env("ANALYZE")
                .map_err(|e| ConfigError::Invalid(e.to_string()))?,
        })
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid value for environment variable: {0}")]
    InvalidValue(&'static str),

    #[error("{0}")]
    Invalid(String),
}

//! Respondent binary configuration. Reads `ASSESS_*` env vars; doesn't
//! touch the author / analyze namespaces so a shared `.env` is safe.

use std::env;

use amaker_core::StorageBackend;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    /// Base URL of the author binary — used for the "Back to authoring" link.
    pub author_base_url: String,
    /// Base URL of the analyze binary — used for the "View results" link.
    pub analyze_base_url: String,
    pub storage_backend: StorageBackend,
}

impl Config {
    /// Optional (with defaults):
    /// - `ASSESS_HOST` (default: `127.0.0.1`; set `0.0.0.0` in a container)
    /// - `ASSESS_PORT` / `PORT` (default: `3001`). `ASSESS_PORT` wins; `PORT`
    ///   is the fallback Cloud Run and similar platforms inject.
    /// - `ASSESS_DATA_DIR` (filesystem backend; default: `./data`)
    /// - `ASSESS_LOG_LEVEL` (default: `info`)
    /// - `ASSESS_AUTHOR_BASE_URL` (default: `http://localhost:3000`)
    /// - `ASSESS_ANALYZE_BASE_URL` (default: `http://localhost:3002`)
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        Ok(Self {
            host: env::var("ASSESS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("ASSESS_PORT")
                .or_else(|_| env::var("PORT"))
                .unwrap_or_else(|_| "3001".to_string())
                .parse()
                .map_err(|_| ConfigError::InvalidValue("ASSESS_PORT"))?,
            log_level: env::var("ASSESS_LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            author_base_url: env::var("ASSESS_AUTHOR_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            analyze_base_url: env::var("ASSESS_ANALYZE_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3002".to_string()),
            storage_backend: amaker_core::load_from_env("ASSESS")
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

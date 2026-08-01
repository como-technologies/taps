//! Provider-aware model catalog and selection.
//!
//! The workspace's model picker must offer what the *configured provider*
//! can actually serve: the static Claude catalog when
//! `AI_PROVIDER=anthropic`, the live tag list from the local ollama server
//! (`GET /api/tags`) when `AI_PROVIDER=ollama`. A stored per-project model
//! selection from the other provider's namespace (e.g. the historical
//! `claude-sonnet-4-5` default on an ollama deployment) is scoped back to
//! the configured provider's default at chat time — otherwise the turn
//! would ask ollama for a Claude model and fail.

use std::time::Duration;

use serde::Deserialize;

use crate::config::{AiProvider, Config};
use crate::models::{ModelOption, claude_models};

/// Timeout for the live `/api/tags` lookup; the picker must not stall the
/// workspace page when the local server is down.
const TAGS_TIMEOUT: Duration = Duration::from_secs(2);

/// The model options the picker should offer under this configuration.
///
/// Ollama failures degrade gracefully: when `/api/tags` is unreachable or
/// empty, the picker falls back to the single configured model (labeled as
/// configured) instead of an empty or misleading list.
pub async fn available_models(config: &Config) -> Vec<ModelOption> {
    match config.ai_provider {
        AiProvider::Anthropic => claude_models(),
        AiProvider::Ollama => match fetch_ollama_tags(&config.ollama_host).await {
            Ok(tags) if !tags.is_empty() => tags
                .into_iter()
                .map(|name| ModelOption {
                    label: name.clone(),
                    value: name,
                })
                .collect(),
            _ => vec![ModelOption {
                value: config.ollama_model.clone(),
                label: format!("{} (configured)", config.ollama_model),
            }],
        },
    }
}

/// The default model a new project stores under this configuration.
pub fn default_model_for(config: &Config) -> String {
    match config.ai_provider {
        AiProvider::Anthropic => config.claude_model.clone(),
        AiProvider::Ollama => config.ollama_model.clone(),
    }
}

/// The model a chat turn actually uses for a project's stored selection.
///
/// A selection from the other provider's namespace — Claude aliases all
/// start with `claude-`; ollama tags never do — falls back to the
/// configured provider default instead of producing a request the backend
/// must reject.
pub fn effective_model(config: &Config, selected: &str) -> String {
    let is_claude_alias = selected.starts_with("claude-");
    let valid = match config.ai_provider {
        AiProvider::Anthropic => is_claude_alias,
        AiProvider::Ollama => !selected.is_empty() && !is_claude_alias,
    };
    if valid {
        selected.to_string()
    } else {
        default_model_for(config)
    }
}

/// Shape of ollama's `GET /api/tags` response (the part we use).
#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

/// Live model list from the ollama server.
async fn fetch_ollama_tags(host: &str) -> Result<Vec<String>, reqwest::Error> {
    let url = format!("{}/api/tags", host.trim_end_matches('/'));
    let client = reqwest::Client::builder().timeout(TAGS_TIMEOUT).build()?;
    let response = client.get(&url).send().await?.error_for_status()?;
    let tags: TagsResponse = response.json().await?;
    Ok(tags.models.into_iter().map(|m| m.name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(provider: AiProvider, ollama_host: &str) -> Config {
        Config {
            host: "127.0.0.1".to_string(),
            port: 0,
            ai_provider: provider,
            anthropic_api_key: Some("sk-test".to_string()),
            claude_model: "claude-sonnet-4-5".to_string(),
            ollama_host: ollama_host.to_string(),
            ollama_model: "llama3.2".to_string(),
            data_dir: "./data".into(),
            rust_log: "info".to_string(),
        }
    }

    /// Serve a fixed `/api/tags` JSON body on an ephemeral port.
    async fn stub_tags_server(json: &'static str) -> String {
        let app = axum::Router::new().route(
            "/api/tags",
            axum::routing::get(move || async move {
                (
                    [(axum::http::header::CONTENT_TYPE, "application/json")],
                    json,
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn anthropic_provider_offers_the_claude_catalog() {
        let models = available_models(&config(AiProvider::Anthropic, "http://unused")).await;
        let values: Vec<_> = models.iter().map(|m| m.value.as_str()).collect();
        assert_eq!(
            values,
            ["claude-sonnet-4-5", "claude-haiku-4-5", "claude-opus-4-5"]
        );
    }

    #[tokio::test]
    async fn ollama_provider_offers_the_live_tag_list() {
        let host = stub_tags_server(
            r#"{"models":[{"name":"llama3.2:latest","size":1},{"name":"qwen2.5:7b","size":2}]}"#,
        )
        .await;

        let models = available_models(&config(AiProvider::Ollama, &host)).await;

        let values: Vec<_> = models.iter().map(|m| m.value.as_str()).collect();
        assert_eq!(values, ["llama3.2:latest", "qwen2.5:7b"]);
        assert_eq!(models[0].label, "llama3.2:latest");
    }

    #[tokio::test]
    async fn unreachable_ollama_falls_back_to_the_configured_model() {
        // Nothing listens on port 9; the picker must not come back empty.
        let models = available_models(&config(AiProvider::Ollama, "http://127.0.0.1:9")).await;

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].value, "llama3.2");
        assert!(
            models[0].label.contains("configured"),
            "the fallback must say it is the configured model: {}",
            models[0].label
        );
    }

    #[tokio::test]
    async fn empty_tag_list_also_falls_back() {
        let host = stub_tags_server(r#"{"models":[]}"#).await;
        let models = available_models(&config(AiProvider::Ollama, &host)).await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].value, "llama3.2");
    }

    #[test]
    fn effective_model_keeps_in_namespace_selections() {
        let anthropic = config(AiProvider::Anthropic, "http://unused");
        assert_eq!(
            effective_model(&anthropic, "claude-haiku-4-5"),
            "claude-haiku-4-5"
        );
        let ollama = config(AiProvider::Ollama, "http://unused");
        assert_eq!(effective_model(&ollama, "qwen2.5:7b"), "qwen2.5:7b");
    }

    #[test]
    fn effective_model_scopes_cross_provider_selections_to_the_default() {
        // The historical bug: projects stored the claude default while the
        // deployment ran ollama — every web chat turn then asked ollama for
        // a Claude model. The selection is scoped back to the provider.
        let ollama = config(AiProvider::Ollama, "http://unused");
        assert_eq!(effective_model(&ollama, "claude-sonnet-4-5"), "llama3.2");
        assert_eq!(effective_model(&ollama, ""), "llama3.2");

        let anthropic = config(AiProvider::Anthropic, "http://unused");
        assert_eq!(effective_model(&anthropic, "llama3.2"), "claude-sonnet-4-5");
        assert_eq!(effective_model(&anthropic, ""), "claude-sonnet-4-5");
    }

    #[test]
    fn default_model_follows_the_provider() {
        assert_eq!(
            default_model_for(&config(AiProvider::Anthropic, "http://unused")),
            "claude-sonnet-4-5"
        );
        assert_eq!(
            default_model_for(&config(AiProvider::Ollama, "http://unused")),
            "llama3.2"
        );
    }
}

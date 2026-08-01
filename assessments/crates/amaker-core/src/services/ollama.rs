//! Ollama implementation of the [`LlmProvider`] seam.
//!
//! Talks to ollama's **native** `/api/chat` endpoint (not the OpenAI-compat
//! layer): one non-streaming POST per chat turn, with the system prompt
//! prepended as a `system`-role message and [`ToolDef`] JSON schemas passed
//! straight through as `tools[].function.parameters`.
//!
//! Normalization rules (ollama → [`ChatResponse`]):
//! - `message.content` becomes [`ChatResponse::text`]
//! - `message.tool_calls[].function` become [`ToolUseBlock`]s; ollama ids are
//!   kept when present, otherwise `ollama_tool_<index>` is synthesized
//!   (older ollama versions return no id)
//! - `stop_reason` is `"ToolUse"` whenever tool calls are present (ollama
//!   reports `done_reason: "stop"` even for tool calls); otherwise
//!   `done_reason` maps `"stop"` → `"EndTurn"`, `"length"` → `"MaxTokens"`,
//!   and anything else passes through verbatim
//! - `was_truncated` is true exactly when `done_reason` is `"length"`
//!
//! Sampling: tool-free turns are the plain-YAML generation paths
//! (`generate_structure`, `generate_questions_for_practice`), so the request
//! pins `temperature: 0` for deterministic output — that is what makes a
//! small local model viable there. Tool-bearing (interactive) turns keep the
//! model's default sampling.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::AppError;
use crate::services::provider::{ChatResponse, LlmProvider, ToolUseBlock};
use crate::services::tools::ToolDef;

/// Request timeout: structure generation on a small local model can take
/// minutes for a long completion, so this is deliberately generous.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Explicit context window (`num_ctx`) for every request. Ollama's default
/// is 2048 tokens and overruns are SILENT: the live context-bearing
/// authoring prompt (~2k tokens) left generation ~50 tokens of window and
/// every deterministic retry was clipped mid-fence. 8192 comfortably holds
/// the largest authoring prompt plus a full structure completion on
/// llama3.2-class models.
const NUM_CTX: u32 = 8192;

/// [`LlmProvider`] backed by a local Ollama server's native `/api/chat`.
pub struct OllamaProvider {
    client: reqwest::Client,
    host: String,
    model: String,
}

impl OllamaProvider {
    /// Create a new Ollama provider for `host` (e.g. `http://localhost:11434`)
    /// using `model` as the default model.
    pub fn new(host: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("default reqwest client construction cannot fail");
        Self {
            client,
            host: host.trim_end_matches('/').to_string(),
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn chat(
        &self,
        system: &str,
        messages: Vec<(String, String)>,
        tools: Vec<ToolDef>,
        max_tokens: u32,
        model_override: Option<&str>,
    ) -> Result<ChatResponse, AppError> {
        let model = model_override.unwrap_or(&self.model);
        let body = build_request(model, system, &messages, &tools, max_tokens);
        let url = format!("{}/api/chat", self.host);

        tracing::info!(
            "Sending request to ollama: url={}, model={}, tools={}, messages={}",
            url,
            model,
            tools.len(),
            messages.len()
        );

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::OllamaApi(format!("request to {url} failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(AppError::OllamaApi(format!(
                "{url} returned {status}: {detail}"
            )));
        }

        let value: Value = response
            .json()
            .await
            .map_err(|e| AppError::OllamaApi(format!("{url} returned a non-JSON response: {e}")))?;

        let chat_response = chat_response_from_value(value)?;
        tracing::info!(
            "ollama response: stop_reason={}, tool_uses={}, text_len={}, truncated={}",
            chat_response.stop_reason,
            chat_response.tool_uses.len(),
            chat_response.text.len(),
            chat_response.was_truncated
        );
        Ok(chat_response)
    }
}

/// Build the native `/api/chat` request body.
///
/// The system prompt is prepended as a `system`-role message (skipped when
/// empty); tool-free requests pin `temperature: 0` (see module docs).
fn build_request(
    model: &str,
    system: &str,
    messages: &[(String, String)],
    tools: &[ToolDef],
    max_tokens: u32,
) -> Value {
    let mut chat_messages = Vec::with_capacity(messages.len() + 1);
    if !system.is_empty() {
        chat_messages.push(json!({"role": "system", "content": system}));
    }
    for (role, content) in messages {
        chat_messages.push(json!({"role": role, "content": content}));
    }

    let mut options = json!({"num_predict": max_tokens, "num_ctx": NUM_CTX});
    if tools.is_empty() {
        options["temperature"] = json!(0);
    }

    let mut request = json!({
        "model": model,
        "messages": chat_messages,
        "stream": false,
        "options": options,
    });

    if !tools.is_empty() {
        let tool_values: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        // ToolDef schemas are plain JSON Schema, which is
                        // exactly what ollama expects here — direct passthrough.
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();
        request["tools"] = Value::Array(tool_values);
    }

    request
}

/// Shape of ollama's non-streaming `/api/chat` response (the parts we use).
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
    #[serde(default)]
    done_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<OllamaToolCall>,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCall {
    /// Present on newer ollama versions, absent on older ones.
    #[serde(default)]
    id: Option<String>,
    function: OllamaFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaFunction {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// Normalize a native `/api/chat` response into the provider-agnostic
/// [`ChatResponse`] (rules in the module docs).
fn chat_response_from_value(value: Value) -> Result<ChatResponse, AppError> {
    let response: OllamaChatResponse = serde_json::from_value(value)
        .map_err(|e| AppError::OllamaApi(format!("malformed /api/chat response: {e}")))?;

    let tool_uses: Vec<ToolUseBlock> = response
        .message
        .tool_calls
        .into_iter()
        .enumerate()
        .map(|(i, call)| ToolUseBlock {
            id: call.id.unwrap_or_else(|| format!("ollama_tool_{i}")),
            name: call.function.name,
            input: call.function.arguments,
        })
        .collect();

    let done_reason = response.done_reason.as_deref();
    let was_truncated = done_reason == Some("length");
    let stop_reason = if !tool_uses.is_empty() {
        "ToolUse".to_string()
    } else {
        match done_reason {
            Some("stop") => "EndTurn".to_string(),
            Some("length") => "MaxTokens".to_string(),
            Some(other) => other.to_string(),
            None => String::new(),
        }
    };

    Ok(ChatResponse {
        text: response.message.content,
        tool_uses,
        stop_reason,
        was_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_def(name: &str) -> ToolDef {
        ToolDef {
            name: name.to_string(),
            description: format!("{name} description"),
            input_schema: json!({
                "type": "object",
                "properties": {"reason": {"type": "string"}},
                "required": ["reason"]
            }),
        }
    }

    // --- request building ---

    #[test]
    fn request_prepends_system_and_maps_roles() {
        let messages = vec![
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "hi".to_string()),
            ("user".to_string(), "go on".to_string()),
        ];
        let request = build_request("llama3.2", "SYSTEM PROMPT", &messages, &[], 4096);

        let msgs = request["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "SYSTEM PROMPT");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "hello");
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(msgs[3]["content"], "go on");
        assert_eq!(request["model"], "llama3.2");
    }

    #[test]
    fn request_skips_empty_system_prompt() {
        let messages = vec![("user".to_string(), "hello".to_string())];
        let request = build_request("llama3.2", "", &messages, &[], 4096);

        let msgs = request["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn request_is_non_streaming_with_token_budget() {
        let request = build_request("llama3.2", "s", &[], &[], 8192);

        assert_eq!(request["stream"], false);
        assert_eq!(request["options"]["num_predict"], 8192);
    }

    /// Ollama defaults `num_ctx` to 2048 tokens and SILENTLY truncates:
    /// a context-bearing authoring prompt (~2k tokens) left generation
    /// ~50 tokens of window, cutting the structure response off mid-fence
    /// on every deterministic retry. The request must size the window
    /// explicitly.
    #[test]
    fn request_pins_an_explicit_context_window() {
        let request = build_request("llama3.2", "s", &[], &[], 8192);

        // The window must comfortably hold prompt + completion; 8192 is
        // the floor that fixed the live clipping.
        let num_ctx = request["options"]["num_ctx"].as_u64().unwrap();
        assert_eq!(num_ctx, u64::from(NUM_CTX));
        assert!(num_ctx >= 8192);
    }

    #[test]
    fn tool_free_request_pins_temperature_zero_and_omits_tools() {
        let request = build_request("llama3.2", "s", &[], &[], 4096);

        assert_eq!(request["options"]["temperature"], 0);
        assert!(
            request.get("tools").is_none(),
            "tool-free request must not send a tools array"
        );
    }

    #[test]
    fn tool_bearing_request_maps_tooldefs_and_keeps_sampling_defaults() {
        let tools = vec![tool_def("advance_phase"), tool_def("generate_structure")];
        let request = build_request("llama3.2", "s", &[], &tools, 4096);

        let sent = request["tools"].as_array().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0]["type"], "function");
        assert_eq!(sent[0]["function"]["name"], "advance_phase");
        assert_eq!(
            sent[0]["function"]["description"],
            "advance_phase description"
        );
        // The ToolDef JSON schema maps directly into function.parameters.
        assert_eq!(sent[0]["function"]["parameters"], tools[0].input_schema);
        assert_eq!(sent[1]["function"]["name"], "generate_structure");
        assert!(
            request["options"].get("temperature").is_none(),
            "tool-bearing requests keep the model's default sampling"
        );
    }

    // --- response normalization ---

    #[test]
    fn text_only_response_normalizes_to_end_turn() {
        let value = json!({
            "model": "llama3.2",
            "message": {"role": "assistant", "content": "Hello!"},
            "done": true,
            "done_reason": "stop"
        });

        let response = chat_response_from_value(value).unwrap();

        assert_eq!(response.text, "Hello!");
        assert!(response.tool_uses.is_empty());
        assert_eq!(response.stop_reason, "EndTurn");
        assert!(!response.was_truncated);
    }

    #[test]
    fn tool_calls_normalize_with_provider_id_kept() {
        let value = json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_vc40ik4m",
                    "function": {
                        "index": 0,
                        "name": "advance_phase",
                        "arguments": {"reason": "ready"}
                    }
                }]
            },
            "done": true,
            "done_reason": "stop"
        });

        let response = chat_response_from_value(value).unwrap();

        assert_eq!(response.tool_uses.len(), 1);
        let call = &response.tool_uses[0];
        assert_eq!(call.id, "call_vc40ik4m");
        assert_eq!(call.name, "advance_phase");
        assert_eq!(call.input, json!({"reason": "ready"}));
        // ollama reports done_reason "stop" even for tool calls, so the
        // presence of tool calls drives the normalized stop reason.
        assert_eq!(response.stop_reason, "ToolUse");
    }

    #[test]
    fn tool_calls_without_id_get_synthesized_ids() {
        let value = json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {"function": {"name": "advance_phase", "arguments": {"reason": "a"}}},
                    {"function": {"name": "go_back_phase", "arguments": {"reason": "b"}}}
                ]
            },
            "done": true,
            "done_reason": "stop"
        });

        let response = chat_response_from_value(value).unwrap();

        assert_eq!(response.tool_uses.len(), 2);
        assert_eq!(response.tool_uses[0].id, "ollama_tool_0");
        assert_eq!(response.tool_uses[1].id, "ollama_tool_1");
        assert_eq!(response.tool_uses[1].name, "go_back_phase");
    }

    #[test]
    fn text_alongside_tool_calls_is_kept() {
        let value = json!({
            "message": {
                "role": "assistant",
                "content": "Let me advance the phase.",
                "tool_calls": [
                    {"function": {"name": "advance_phase", "arguments": {"reason": "done"}}}
                ]
            },
            "done": true,
            "done_reason": "stop"
        });

        let response = chat_response_from_value(value).unwrap();

        assert_eq!(response.text, "Let me advance the phase.");
        assert_eq!(response.tool_uses.len(), 1);
        assert_eq!(response.stop_reason, "ToolUse");
    }

    #[test]
    fn length_done_reason_maps_to_max_tokens_and_truncation() {
        let value = json!({
            "message": {"role": "assistant", "content": "1, 2,"},
            "done": true,
            "done_reason": "length"
        });

        let response = chat_response_from_value(value).unwrap();

        assert_eq!(response.stop_reason, "MaxTokens");
        assert!(response.was_truncated);
    }

    #[test]
    fn unknown_done_reason_passes_through_verbatim() {
        let value = json!({
            "message": {"role": "assistant", "content": "x"},
            "done": true,
            "done_reason": "unload"
        });

        let response = chat_response_from_value(value).unwrap();

        assert_eq!(response.stop_reason, "unload");
        assert!(!response.was_truncated);
    }

    #[test]
    fn malformed_response_is_a_clear_error() {
        // No "message" field at all — e.g. an error payload or a breaking
        // API change. Must surface as a provider error, not a panic.
        let value = json!({"error": "model not found"});

        let err = chat_response_from_value(value).unwrap_err();

        assert!(matches!(err, AppError::OllamaApi(_)));
        let msg = err.to_string();
        assert!(msg.contains("malformed"), "got: {msg}");
    }

    #[test]
    fn tool_call_missing_function_name_is_a_clear_error() {
        let value = json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{"function": {"arguments": {}}}]
            },
            "done": true,
            "done_reason": "stop"
        });

        let err = chat_response_from_value(value).unwrap_err();

        assert!(matches!(err, AppError::OllamaApi(_)));
    }

    #[test]
    fn provider_trims_trailing_slash_from_host() {
        let provider = OllamaProvider::new("http://localhost:11434/".to_string(), "m".to_string());
        assert_eq!(provider.host, "http://localhost:11434");
    }
}

/// Live, env-gated smoke test against a local ollama (`just smoke-ollama`).
#[cfg(test)]
mod live_smoke {
    use super::*;
    use crate::services::YamlService;
    use crate::services::generation;

    /// Proves the real provider end to end: a structure-generation prompt to
    /// the local ollama produces fenced YAML that passes schema validation.
    ///
    /// Gated on `ASSESSMENTS_E2E_OLLAMA=1` so plain `cargo test` (and CI)
    /// skips it; run it via `just smoke-ollama`.
    #[tokio::test]
    async fn live_ollama_structure_generation_yaml_parses() {
        if std::env::var("ASSESSMENTS_E2E_OLLAMA").as_deref() != Ok("1") {
            eprintln!(
                "skipping live ollama smoke: set ASSESSMENTS_E2E_OLLAMA=1 \
                 (and run a local ollama with llama3.2) to enable"
            );
            return;
        }

        let host =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string());
        eprintln!("live smoke: {host} / {model}");
        let provider = OllamaProvider::new(host, model);

        let chat_history = "user: We want to assess the engineering maturity of a mid-size \
            software team: delivery practices, code quality, testing, and operations.\n\n\
            assistant: Understood - an engineering maturity assessment covering delivery, \
            quality, testing, and operations.";
        // The trailing instruction keeps small models from dropping the
        // `questions: []` arrays the schema requires on every practice.
        let context = "Keep it small: about 3 domains with 2-3 practices each.\n\n\
            IMPORTANT: every practice object must include the literal line \
            `questions: []` after its risk field.";

        let response = generation::generate_structure(&provider, context, chat_history)
            .await
            .expect("live /api/chat structure generation failed");

        let yaml_pattern = regex::Regex::new(r"```ya?ml\s*\n([\s\S]*?)\n```").unwrap();
        let yaml = yaml_pattern
            .captures(&response)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str())
            .unwrap_or_else(|| panic!("no fenced YAML block in model response:\n{response}"));

        let assessment = YamlService::parse_assessment(yaml)
            .unwrap_or_else(|e| panic!("model YAML failed schema validation: {e}\n---\n{yaml}"));

        assert!(!assessment.domains.is_empty(), "no domains generated");
        assert!(
            assessment.domains.iter().all(|d| !d.practices.is_empty()),
            "a domain has no practices"
        );

        eprintln!(
            "live smoke OK: parsed assessment '{}' — {} domains, {} practices",
            assessment.name,
            assessment.domains.len(),
            assessment
                .domains
                .iter()
                .map(|d| d.practices.len())
                .sum::<usize>()
        );
        eprintln!("--- validated YAML ---\n{yaml}");
    }
}

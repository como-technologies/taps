//! Anthropic implementation of the [`LlmProvider`] seam.
//!
//! Wraps the unofficial `anthropic-sdk-rust` client. Everything
//! Anthropic-specific — SDK types, tool-schema conversion, stop-reason
//! mapping — stays inside this module.

use anthropic_sdk::{Anthropic, ContentBlock, Message, MessageCreateBuilder, Tool};
use async_trait::async_trait;

use crate::error::AppError;
use crate::services::provider::{ChatResponse, LlmProvider, ToolUseBlock};
use crate::services::tools::ToolDef;

/// [`LlmProvider`] backed by the Anthropic API.
pub struct AnthropicProvider {
    client: Anthropic,
    model: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    pub fn new(api_key: &str, model: String) -> Result<Self, AppError> {
        let client = Anthropic::new(api_key)
            .map_err(|e| AppError::ClaudeApi(format!("Failed to create Anthropic client: {e}")))?;
        Ok(Self { client, model })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(
        &self,
        system: &str,
        messages: Vec<(String, String)>,
        tools: Vec<ToolDef>,
        max_tokens: u32,
        model_override: Option<&str>,
    ) -> Result<ChatResponse, AppError> {
        // Use model override if provided, otherwise use default
        let model = model_override.unwrap_or(&self.model);

        // Build the message request
        let mut builder = MessageCreateBuilder::new(model, max_tokens).system(system);

        // Add tools if provided
        if !tools.is_empty() {
            let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
            tracing::info!("Converting {} tools for API: {:?}", tools.len(), tool_names);

            let sdk_tools: Vec<Tool> = tools
                .into_iter()
                .filter_map(|t| {
                    let input_schema: Result<anthropic_sdk::types::tools::ToolInputSchema, _> =
                        serde_json::from_value(t.input_schema.clone());
                    match input_schema {
                        Ok(schema) => {
                            tracing::debug!("Tool '{}' schema converted successfully", t.name);
                            Some(Tool {
                                name: t.name,
                                description: t.description,
                                input_schema: schema,
                            })
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to deserialize tool schema for '{}': {}",
                                t.name,
                                e
                            );
                            tracing::debug!("Schema was: {}", t.input_schema);
                            None
                        }
                    }
                })
                .collect();

            if sdk_tools.is_empty() {
                tracing::warn!("All tools failed to convert - no tools will be passed to API!");
            } else {
                tracing::info!("Passing {} tools to Claude API", sdk_tools.len());
                builder = builder.tools(sdk_tools);
            }
        } else {
            tracing::debug!("No tools provided for this request");
        }

        // Add conversation history
        for (role, content) in messages {
            match role.as_str() {
                "user" => builder = builder.user(content),
                "assistant" => builder = builder.assistant(content),
                _ => {}
            }
        }

        // Send the request
        let params = builder.build();

        // Log request details for debugging
        if let Ok(request_json) = serde_json::to_value(&params) {
            let tools_count = request_json
                .get("tools")
                .and_then(|t| t.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            tracing::info!(
                "Sending request to Claude: model={}, tools={}, messages={}",
                model,
                tools_count,
                request_json
                    .get("messages")
                    .and_then(|m| m.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0)
            );
            if tools_count > 0 {
                tracing::debug!(
                    "Request tools: {:?}",
                    request_json
                        .get("tools")
                        .and_then(|t| t.as_array())
                        .map(|arr| arr
                            .iter()
                            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                            .collect::<Vec<_>>())
                );
            }
        }

        let response: Message = self
            .client
            .messages()
            .create(params)
            .await
            .map_err(|e| AppError::ClaudeApi(e.to_string()))?;

        // Log response details
        tracing::info!(
            "Claude response: stop_reason={:?}, content_blocks={}",
            response.stop_reason,
            response.content.len()
        );

        // Log each content block type
        for (i, block) in response.content.iter().enumerate() {
            match block {
                ContentBlock::Text { text } => {
                    let preview = if text.len() > 100 {
                        let mut end = 100;
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}...", &text[..end])
                    } else {
                        text.clone()
                    };
                    tracing::debug!(
                        "Content block {}: Text({} chars): {}",
                        i,
                        text.len(),
                        preview
                    );
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tracing::info!(
                        "Content block {}: ToolUse(id={}, name={}, input={})",
                        i,
                        id,
                        name,
                        input
                    );
                }
                _ => {
                    tracing::debug!("Content block {}: Other type", i);
                }
            }
        }

        // Normalize the response
        Ok(chat_response_from_message(response))
    }
}

/// Normalize an SDK [`Message`] into the provider-agnostic [`ChatResponse`].
fn chat_response_from_message(message: Message) -> ChatResponse {
    let mut text = String::new();
    let mut tool_uses = Vec::new();

    for block in message.content {
        match block {
            ContentBlock::Text { text: t } => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&t);
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_uses.push(ToolUseBlock { id, name, input });
            }
            _ => {}
        }
    }

    let stop_reason = message
        .stop_reason
        .map(|r| format!("{:?}", r))
        .unwrap_or_default();
    let was_truncated = stop_reason == "MaxTokens";

    ChatResponse {
        text,
        tool_uses,
        stop_reason,
        was_truncated,
    }
}

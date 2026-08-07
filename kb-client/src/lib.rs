//! Shared KB transport client for taps tools.
//!
//! A tool finds the KB one way: you tell it the door. `KB_URL` is the
//! appliance's streamable-HTTP MCP endpoint (e.g. `http://kb:8080/mcp`);
//! `KB_WIKI` optionally names the target space (omitted → the appliance's
//! default). Nothing here reads the engine's registry or touches a space's
//! filesystem — the transport surface is the only door.

use anyhow::{Context, Result, bail};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;

/// Where the KB is: endpoint URL plus optional target space name.
#[derive(Debug, Clone)]
pub struct KbTarget {
    /// Streamable-HTTP MCP endpoint, e.g. `http://localhost:8080/mcp`.
    pub url: String,
    /// Target space name; `None` uses the appliance's default space.
    pub wiki: Option<String>,
}

impl KbTarget {
    /// Read `KB_URL` / `KB_WIKI` from the environment. `None` when `KB_URL`
    /// is unset or blank — the tool works standalone and KB surfaces should
    /// degrade with a clear message, not an error.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("KB_URL").ok()?;
        let url = url.trim();
        if url.is_empty() {
            return None;
        }
        Some(Self {
            url: url.to_string(),
            wiki: std::env::var("KB_WIKI")
                .ok()
                .map(|w| w.trim().to_string())
                .filter(|w| !w.is_empty()),
        })
    }
}

/// A connected MCP client session against one appliance.
pub struct KbClient {
    service: RunningService<RoleClient, ()>,
    wiki: Option<String>,
}

impl KbClient {
    /// Connect and complete the MCP handshake.
    pub async fn connect(target: &KbTarget) -> Result<Self> {
        let transport = StreamableHttpClientTransport::from_uri(target.url.clone());
        let service = ()
            .serve(transport)
            .await
            .with_context(|| format!("failed to connect to KB at {}", target.url))?;
        Ok(Self {
            service,
            wiki: target.wiki.clone(),
        })
    }

    /// Call a wiki tool with JSON arguments; returns the joined text content.
    /// The target space (when set) is injected as the `wiki` argument unless
    /// the caller already provided one. Tool-level errors become `Err`.
    pub async fn call(&self, tool: &str, args: serde_json::Value) -> Result<String> {
        let mut arguments = match args {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            other => bail!("tool arguments must be a JSON object, got: {other}"),
        };
        if let Some(wiki) = &self.wiki
            && !arguments.contains_key("wiki")
        {
            arguments.insert("wiki".into(), serde_json::Value::String(wiki.clone()));
        }

        let result = self
            .service
            .call_tool(CallToolRequestParams::new(tool.to_string()).with_arguments(arguments))
            .await
            .with_context(|| format!("KB call failed: {tool}"))?;

        let text = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_error.unwrap_or(false) {
            bail!("{tool} returned an error: {text}");
        }
        Ok(text)
    }

    /// Call a wiki tool and parse its text content as JSON.
    pub async fn call_json(
        &self,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let text = self.call(tool, args).await?;
        serde_json::from_str(&text)
            .with_context(|| format!("{tool} returned non-JSON content: {text}"))
    }

    /// Close the session cleanly.
    pub async fn close(self) -> Result<()> {
        self.service.cancel().await.ok();
        Ok(())
    }
}

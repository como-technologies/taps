//! Shared KB transport client for taps tools.
//!
//! A tool finds the KB one way: you tell it the door. `KB_URL` is the
//! appliance's streamable-HTTP MCP endpoint (e.g. `http://kb:8080/mcp`);
//! `KB_WIKI` optionally names the target space (omitted → the appliance's
//! default). Nothing here reads the engine's registry or touches a space's
//! filesystem — the transport surface is the only door.
//!
//! Where the pair *lives* is one suite-wide discovery order, owned here so
//! every tool answers identically ([`load_env`] / [`KbTarget::discover`]):
//! the process environment wins, then a `.env` in the working directory,
//! then the user-level `~/.config/taps/env` — written once when an
//! appliance stands up, inherited by every tool ever after.

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

/// Load the suite's configuration layers into the process environment:
/// a `.env` in the working directory, then the user-level config
/// (`$XDG_CONFIG_HOME`/`~/.config` + `taps/env`). Neither load overrides a
/// variable that is already set, which is exactly what produces the
/// discovery order: process env > cwd `.env` > user config. Call once at
/// startup (before clap reads `env =` defaults) or lean on
/// [`KbTarget::discover`], which calls it.
pub fn load_env() {
    dotenvy::dotenv().ok();
    if let Some(dirs) = directories::ProjectDirs::from("", "", "taps") {
        dotenvy::from_path(dirs.config_dir().join("env")).ok();
    }
}

impl KbTarget {
    /// Resolve the target through the suite's discovery order
    /// ([`load_env`]), then read the pair. The one call sites want.
    pub fn discover() -> Option<Self> {
        load_env();
        Self::from_env()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// One sequential body: environment and working directory are
    /// process-global, so every stage of the discovery order is asserted
    /// here in sequence rather than across racing tests.
    #[test]
    fn discovery_order_is_process_env_then_cwd_dotenv_then_user_config() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().join("cwd");
        let xdg = tmp.path().join("xdg");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(xdg.join("taps")).unwrap();
        std::fs::write(
            xdg.join("taps/env"),
            "KB_URL=http://config-level/mcp\nKB_WIKI=configspace\n",
        )
        .unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &xdg);
            std::env::remove_var("KB_URL");
            std::env::remove_var("KB_WIKI");
        }
        std::env::set_current_dir(&cwd).unwrap();

        // The user config alone answers.
        let t = KbTarget::discover().unwrap();
        assert_eq!(t.url, "http://config-level/mcp");
        assert_eq!(t.wiki.as_deref(), Some("configspace"));

        // A cwd .env outranks the user config; a variable it doesn't set
        // still falls through to the config file.
        unsafe {
            std::env::remove_var("KB_URL");
            std::env::remove_var("KB_WIKI");
        }
        std::fs::write(cwd.join(".env"), "KB_URL=http://cwd-level/mcp\n").unwrap();
        let t = KbTarget::discover().unwrap();
        assert_eq!(t.url, "http://cwd-level/mcp");
        assert_eq!(t.wiki.as_deref(), Some("configspace"));

        // The process environment outranks both files.
        unsafe {
            std::env::set_var("KB_URL", "http://process-level/mcp");
        }
        let t = KbTarget::discover().unwrap();
        assert_eq!(t.url, "http://process-level/mcp");

        unsafe {
            std::env::remove_var("KB_URL");
            std::env::remove_var("KB_WIKI");
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }
}

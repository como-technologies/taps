//! `amaker mcp` — the same command surface, served to agents over MCP stdio.
//!
//! Every tool here is a ~5-line wrapper over a [`crate::surface`] core
//! function: the parameter structs (and their doc comments) are shared with
//! the clap definition, so `--help` and the MCP tool list describe the same
//! surface by construction. The web apps are deliberately absent — they are
//! human seats; this is the automation door.
//!
//! The server resolves `DATA_DIR` and `KB_URL`/`KB_WIKI` exactly as the
//! terminal commands do (dotenv from the working directory), so launch it
//! with cwd at the amaker checkout — e.g. in `.mcp.json`:
//! `"command": "bash", "args": ["-lc", "cd ~/taps/assessments && exec amaker mcp"]`.

use std::path::PathBuf;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, ErrorData, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};

use crate::surface::{
    ExportParams, ImportParams, PublishParams, SchemaParams, StatusParams, ValidateParams,
};

/// MCP server over the amaker command surface.
#[derive(Clone)]
pub struct AmakerServer {
    /// Storage root, resolved once at startup (the CLI's `DATA_DIR`).
    data_dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

/// A core-function error, surfaced with its full context chain.
fn err(e: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(format!("{e:#}"), None)
}

/// A core-function report, returned as pretty JSON text content.
fn ok_json(v: serde_json::Value) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(&v).map_err(|e| err(e.into()))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[tool_router]
impl AmakerServer {
    /// Create a server rooted at `data_dir`.
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            tool_router: Self::tool_router(),
        }
    }

    /// Validate an assessment file against the JSON Schema and the
    /// degeneracy gate (placeholder-echo content fails even when
    /// schema-valid). Returns the machine summary: name, domains,
    /// practices (each with its domain — the join key against adroit's
    /// import seeds), and question counts.
    #[tool]
    fn validate(
        &self,
        Parameters(p): Parameters<ValidateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        crate::surface::validate_core(&p)
            .map_err(err)
            .and_then(ok_json)
    }

    /// Import an assessment file as a new project with a published
    /// version — the headless authoring door (validate → import →
    /// respondents answer). Returns the new project_id.
    #[tool]
    async fn import(
        &self,
        Parameters(p): Parameters<ImportParams>,
    ) -> Result<CallToolResult, ErrorData> {
        crate::surface::import_core(&self.data_dir, &p)
            .await
            .map_err(err)
            .and_then(ok_json)
    }

    /// Export a stored project's assessment (yaml, json, or toml). Content
    /// is returned inline unless `out` names a file to write.
    #[tool]
    async fn export(
        &self,
        Parameters(p): Parameters<ExportParams>,
    ) -> Result<CallToolResult, ErrorData> {
        crate::surface::export_core(&self.data_dir, &p)
            .await
            .map_err(err)
            .and_then(ok_json)
    }

    /// Publish a project's assessment + analysis to the KB as
    /// amaker-owned pages (needs KB_URL; KB_WIKI or `wiki` names the
    /// space). Returns the publish report: pages written, schemas
    /// registered, ingest verdict.
    #[tool]
    async fn publish(
        &self,
        Parameters(p): Parameters<PublishParams>,
    ) -> Result<CallToolResult, ErrorData> {
        crate::surface::publish_core(&self.data_dir, &p)
            .await
            .map_err(err)
            .and_then(ok_json)
    }

    /// The assessment JSON Schema an assessment file must satisfy.
    #[tool]
    fn schema(&self, Parameters(p): Parameters<SchemaParams>) -> Result<CallToolResult, ErrorData> {
        crate::surface::schema_core(&p)
            .map_err(err)
            .and_then(ok_json)
    }

    /// List every project with its latest published version.
    #[tool]
    async fn list(&self) -> Result<CallToolResult, ErrorData> {
        crate::surface::list_core(&self.data_dir)
            .await
            .map_err(err)
            .and_then(ok_json)
    }

    /// One project's published versions and the respondent's progress —
    /// `response.complete` is the signal that the human has answered and
    /// the project is ready to publish.
    #[tool]
    async fn status(
        &self,
        Parameters(p): Parameters<StatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        crate::surface::status_core(&self.data_dir, &p)
            .await
            .map_err(err)
            .and_then(ok_json)
    }
}

impl AmakerServer {
    /// Names of every tool the router serves — the introspection tests
    /// pin the surface with this.
    pub fn tool_names() -> Vec<String> {
        Self::tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }
}

#[tool_handler]
impl ServerHandler for AmakerServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("amaker", env!("CARGO_PKG_VERSION")))
    }
}

/// Serve the surface on stdio until the client hangs up.
pub async fn serve(data_dir: PathBuf) -> anyhow::Result<()> {
    let service = AmakerServer::new(data_dir)
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| anyhow::anyhow!("failed to start MCP stdio server: {e}"))?;
    service.waiting().await?;
    Ok(())
}

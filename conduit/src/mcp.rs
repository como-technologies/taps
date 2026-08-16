//! `conduit mcp` — the work-item surface, served to harness sessions over
//! MCP stdio.
//!
//! Every tool is a thin wrapper over a [`crate::surface`] core function
//! with `Actor::Harness` — the parameter structs (and their doc comments)
//! are shared with the clap definition, so `--help` and the MCP tool list
//! describe the same surface by construction.
//!
//! **`signoff` is deliberately absent.** Sign-off is a human seat; through
//! this door it is not refused, it does not exist. Project `close` exists
//! here but carries `Actor::Harness`, so the lifecycle table refuses it —
//! the harness can close stories, never projects. `complete` is exposed
//! because the merge door is mechanical: anyone may knock, only the checks
//! decide.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, ErrorData, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};

use crate::surface::{self, IdParams, ListParams, NewParams, ShowParams};
use crate::work::KbWorkStore;
use crate::workitem::Actor;

/// MCP server over the conduit work-item surface.
#[derive(Clone)]
pub struct ConduitServer {
    /// Working directory (internal repos live under it).
    dir: std::path::PathBuf,
    /// Merge-door gate deadline.
    gate_timeout: std::time::Duration,
    tool_router: ToolRouter<Self>,
}

fn err(e: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(format!("{e:#}"), None)
}

fn ok_json(v: serde_json::Value) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(&v).map_err(|e| err(e.into()))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

impl ConduitServer {
    pub fn new(dir: std::path::PathBuf, gate_timeout: std::time::Duration) -> Self {
        Self {
            dir,
            gate_timeout,
            tool_router: Self::tool_router(),
        }
    }

    /// Names of every tool the router serves — the introspection tests pin
    /// the surface (and `signoff`'s absence) with this.
    pub fn tool_names() -> Vec<String> {
        Self::tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// One KB session per tool call: connect, run the core, close.
    async fn with_store<F, Fut>(&self, f: F) -> Result<CallToolResult, ErrorData>
    where
        F: FnOnce(KbWorkStore) -> Fut,
        Fut: Future<Output = (KbWorkStore, anyhow::Result<serde_json::Value>)>,
    {
        let store = KbWorkStore::connect().await.map_err(err)?;
        let (store, out) = f(store).await;
        store.close().await.ok();
        out.map_err(err).and_then(ok_json)
    }
}

#[tool_router]
impl ConduitServer {
    /// Create a draft work item (project/story/task). The body is the
    /// contract: the goal at this altitude plus its verification form
    /// (executive terms / BDD scenarios / the test set). Drafts are not
    /// executable — a human signs off before any work starts.
    #[tool(name = "new")]
    async fn new_item(
        &self,
        Parameters(p): Parameters<NewParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s| {
            let out = surface::new_core(&s, &p).await;
            (s, out)
        })
        .await
    }

    /// The work tree as rows — class, status, parent, and each item's seal
    /// state (intact/broken/unsealed).
    #[tool]
    async fn list(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s| {
            let out = surface::list_core(&s, &p).await;
            (s, out)
        })
        .await
    }

    /// One work item in full — frontmatter, body, seal state, children.
    #[tool]
    async fn show(
        &self,
        Parameters(p): Parameters<ShowParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s| {
            let out = surface::show_core(&s, &p).await;
            (s, out)
        })
        .await
    }

    /// Reopen a signed item: strip the approval seal and return it (and its
    /// signed descendants) to draft. Use when a signed contract needs
    /// revision or is inconsistent with the KB — only a human seat can
    /// re-sign afterwards.
    #[tool]
    async fn bounce(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s| {
            let out = surface::bounce_core(&s, Actor::Harness, &p).await;
            (s, out)
        })
        .await
    }

    /// Claim a ready task for execution: verifies the seal, provisions the
    /// project's internal repo and the task branch, and returns the clone
    /// hint. Work happens in your workspace; push to the branch.
    #[tool]
    async fn claim(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let dir = self.dir.clone();
        self.with_store(async |s| {
            let out = surface::claim_core(&s, &dir, Actor::Harness, &p).await;
            (s, out)
        })
        .await
    }

    /// Ask the mechanical merge door to land a claimed task: seal intact +
    /// the project's gate green on the branch, then one squash commit onto
    /// main, telemetry written, task done. The door's checks decide — not
    /// the caller.
    #[tool]
    async fn complete(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let (dir, timeout) = (self.dir.clone(), self.gate_timeout);
        self.with_store(async |s| {
            let out = surface::complete_core(&s, &dir, timeout, &p).await;
            (s, out)
        })
        .await
    }

    /// Close a story whose children are all terminal (at least one done).
    /// Projects close at the human seat only — through this door the
    /// lifecycle refuses them.
    #[tool]
    async fn close(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s| {
            let out = surface::close_core(&s, Actor::Harness, &p).await;
            (s, out)
        })
        .await
    }

    /// Cancel an item and every non-terminal descendant — abandoning work,
    /// not completing it. Terminal items never move.
    #[tool]
    async fn cancel(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s| {
            let out = surface::cancel_core(&s, Actor::Harness, &p).await;
            (s, out)
        })
        .await
    }
}

// rmcp's default router expression is `Self::tool_router()` (a fresh router
// per call); point it at the one built at construction instead.
#[tool_handler(router = self.tool_router)]
impl ServerHandler for ConduitServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("conduit", env!("CARGO_PKG_VERSION")))
    }
}

/// Serve the surface on stdio until the client hangs up.
pub async fn serve(
    dir: std::path::PathBuf,
    gate_timeout: std::time::Duration,
) -> anyhow::Result<()> {
    let service = ConduitServer::new(dir, gate_timeout)
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|e| anyhow::anyhow!("failed to start MCP stdio server: {e}"))?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_harness_door_has_no_signoff() {
        let mut names = ConduitServer::tool_names();
        names.sort();
        assert_eq!(
            names,
            vec![
                "bounce", "cancel", "claim", "close", "complete", "list", "new", "show",
            ],
            "signoff must be physically absent from the harness door"
        );
    }
}

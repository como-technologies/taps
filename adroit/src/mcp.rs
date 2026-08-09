//! `adroit mcp` — the same command surface, served to agents over MCP stdio.
//!
//! Every tool here is a ~5-line wrapper over a [`crate::surface`] core
//! function: the parameter structs (and their doc comments) are shared with
//! the clap definition, so `--help` and the MCP tool list describe the same
//! surface by construction. This is the door prescribing agents come
//! through — reading gap reports (or anything else) as ordinary pages via
//! the engine's own tools, then recording judgment here.
//!
//! The server resolves `KB_URL`/`KB_WIKI` exactly as the terminal commands
//! do (dotenv from the working directory); each tool call opens and closes
//! its own KB session, so the server holds no connection state.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, ErrorData, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};

use crate::naming::NamingScheme;
use crate::store::KbStore;
use crate::surface::{
    self, EditParams, LintParams, ListParams, NewParams, PlanParams, SetReviewParams,
    SetStatusParams, ShowParams, SupersedeParams,
};

/// MCP server over the adroit command surface.
#[derive(Clone)]
pub struct AdroitServer {
    /// The naming default, resolved once at startup (`ADROIT_NAMING`).
    naming: NamingScheme,
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

impl AdroitServer {
    /// Create a server with the given naming default.
    pub fn new(naming: NamingScheme) -> Self {
        Self {
            naming,
            tool_router: Self::tool_router(),
        }
    }

    /// Names of every tool the router serves — the introspection tests pin
    /// the surface with this.
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
        F: FnOnce(KbStore, NamingScheme) -> Fut,
        Fut: Future<Output = (KbStore, anyhow::Result<serde_json::Value>)>,
    {
        let store = KbStore::connect().await.map_err(err)?;
        let (store, out) = f(store, self.naming).await;
        store.close().await.ok();
        out.map_err(err).and_then(ok_json)
    }
}

#[tool_router]
impl AdroitServer {
    /// Create a proposed decision page: title, one-line summary, and
    /// markdown body from your judgment, with `relates` recording where the
    /// decision came from (slugs/ids of source pages — provenance edges the
    /// engine's broken-link lint keeps honest). Registers adroit's schemas
    /// on first contact and returns the allocated reference, slug, id, and
    /// the admission gate's report.
    #[tool(name = "new")]
    async fn new_decision(
        &self,
        Parameters(p): Parameters<NewParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::new_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// The decision corpus as rows (reference, slug, id, title, status,
    /// review state), optionally filtered by status.
    #[tool]
    async fn list(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::list_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// One decision in full — frontmatter, body, and whether a stored plan
    /// exists — resolved from a reference, slug, or page id.
    #[tool]
    async fn show(
        &self,
        Parameters(p): Parameters<ShowParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::show_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// The mechanical authoring-quality gate on one decision's body:
    /// prompt-echo sections, missing negative consequences, single-option
    /// records, duplicated skeletons, bracket placeholders. Deterministic —
    /// no provider.
    #[tool]
    async fn lint(
        &self,
        Parameters(p): Parameters<LintParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::lint_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// Replace a proposed decision's body — the refine seat. Takes the
    /// whole replacement body (not a patch); frontmatter and every foreign
    /// key survive the rewrite. Decided records are superseded, not edited.
    #[tool]
    async fn edit(
        &self,
        Parameters(p): Parameters<EditParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::edit_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// A lifecycle transition in place: proposed → accepted/rejected,
    /// accepted → deprecated. Superseded goes through `supersede`. The
    /// rewrite preserves every frontmatter key adroit doesn't own.
    #[tool]
    async fn set_status(
        &self,
        Parameters(p): Parameters<SetStatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::set_status_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// Set or clear a decision's review-by date (YYYY-MM-DD).
    #[tool]
    async fn set_review(
        &self,
        Parameters(p): Parameters<SetReviewParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::set_review_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// Link a new decision over an old one: both sides' frontmatter in one
    /// stroke (by page id), the old side's status becomes superseded.
    #[tool]
    async fn supersede(
        &self,
        Parameters(p): Parameters<SupersedeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::supersede_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// The stored-plan contract: read the marker-bracketed
    /// `## Implementation` plan (deterministic, provider-free), or with
    /// `save` splice one in. Returns the pinned envelope
    /// `{reference, title, plan, stored}`.
    #[tool]
    async fn plan(
        &self,
        Parameters(p): Parameters<PlanParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::plan_core(&s, n, &p).await;
            (s, out)
        })
        .await
    }

    /// Corpus invariants: supersession integrity (refs resolve, statuses
    /// agree, both sides point at each other) and duplicate
    /// references/ids/titles. Link integrity is the engine's lint.
    #[tool]
    async fn check(&self) -> Result<CallToolResult, ErrorData> {
        self.with_store(async |s, n| {
            let out = surface::check_core(&s, n).await;
            (s, out)
        })
        .await
    }
}

// rmcp 1.8's default router expression is `Self::tool_router()` (a fresh
// router per call); point it at the one built at construction instead.
#[tool_handler(router = self.tool_router)]
impl ServerHandler for AdroitServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("adroit", env!("CARGO_PKG_VERSION")))
    }
}

/// Serve the surface on stdio until the client hangs up.
pub async fn serve(naming: NamingScheme) -> anyhow::Result<()> {
    let service = AdroitServer::new(naming)
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
    fn the_tool_surface_is_pinned() {
        // One definition, three doors: the MCP tool list is the verb table
        // from taps #93, exactly.
        let mut names = AdroitServer::tool_names();
        names.sort();
        assert_eq!(
            names,
            vec![
                "check",
                "edit",
                "lint",
                "list",
                "new",
                "plan",
                "set_review",
                "set_status",
                "show",
                "supersede",
            ]
        );
    }
}

//! The KB door for work items: pages live in a wiki, reached over the
//! transport.
//!
//! Mirrors adroit's store seam: [`WorkStore`] is the narrow set of
//! operations the door verbs need, so the surface tests run against an
//! in-memory fake; [`KbWorkStore`] is the real thing over `como-kb-client`
//! (`KB_URL` / `KB_WIKI`). conduit is the only writer of
//! `project`/`story`/`task` pages and registers their schemas on first
//! contact, idempotently — pages enter through the same admission gates as
//! everything else (`wiki_ingest`), and the gate's report is surfaced, not
//! assumed.

use anyhow::{Result, bail};
use async_trait::async_trait;
use como_kb_client::{KbClient, KbTarget};
use serde_json::json;

use crate::item::{Loaded, WORK_ROOT, WorkCorpus};
use crate::workitem::SCHEMAS;

/// The store operations the door verbs are written against.
#[async_trait]
pub trait WorkStore: Send + Sync {
    /// First contact: register conduit's page classes (`project`, `story`,
    /// `task`), idempotently. Returns per-schema outcomes; a conflict
    /// (someone else's type under our name) is fatal.
    async fn ensure_schemas(&self) -> Result<Vec<String>>;
    /// Slugs of every page of the given work-item type.
    async fn slugs(&self, type_name: &str) -> Result<Vec<String>>;
    /// Raw page content (frontmatter + body) by slug.
    async fn read(&self, slug: &str) -> Result<String>;
    /// Write raw page content by slug.
    async fn write(&self, slug: &str, content: &str) -> Result<()>;
    /// Admit `work/` through the gates (validate, commit, index) and return
    /// the gate's report.
    async fn ingest(&self) -> Result<serde_json::Value>;
}

/// Load and parse every work-item page. A page that fails to parse is a
/// hard error — the admission gate validated it on the way in, so a broken
/// page means the corpus needs a human, not a silent skip.
pub async fn load_corpus(store: &dyn WorkStore) -> Result<WorkCorpus> {
    let mut entries = Vec::new();
    for (type_name, _) in SCHEMAS {
        for slug in store.slugs(type_name).await? {
            let text = store.read(&slug).await?;
            let item = crate::item::Item::parse(&slug, &text)?;
            entries.push(Loaded { slug, text, item });
        }
    }
    entries.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(WorkCorpus { entries })
}

/// The transport store: one connected appliance session.
pub struct KbWorkStore {
    kb: KbClient,
}

impl KbWorkStore {
    /// Connect via the suite pair (`KB_URL` / `KB_WIKI`), resolved through
    /// the suite-wide discovery order (env, cwd `.env`,
    /// `~/.config/taps/env`). Fails visibly when no KB is configured — the
    /// KB is the work-item store, there is nothing to fall back to.
    pub async fn connect() -> Result<Self> {
        let Some(target) = KbTarget::discover() else {
            bail!(
                "no KB configured: set KB_URL to your appliance's MCP endpoint \
                 (e.g. http://localhost:8080/mcp; `just kb-dev` serves one) and \
                 optionally KB_WIKI for the target wiki — in the environment, \
                 a .env here, or ~/.config/taps/env"
            );
        };
        Ok(Self {
            kb: KbClient::connect(&target).await?,
        })
    }

    /// Close the session cleanly.
    pub async fn close(self) -> Result<()> {
        self.kb.close().await
    }
}

#[async_trait]
impl WorkStore for KbWorkStore {
    async fn ensure_schemas(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for (type_name, schema) in SCHEMAS {
            let report = self
                .kb
                .call_json(
                    "wiki_admin_schema_register",
                    json!({"type": type_name, "schema": schema}),
                )
                .await?;
            out.push(format!(
                "{type_name}: {}",
                report["status"].as_str().unwrap_or("?")
            ));
        }
        Ok(out)
    }

    async fn slugs(&self, type_name: &str) -> Result<Vec<String>> {
        // Page through the listing until the reported total is collected.
        let mut slugs = Vec::new();
        let mut page = 1;
        loop {
            let list = self
                .kb
                .call_json(
                    "wiki_list",
                    json!({"type": type_name, "page": page, "page_size": 200}),
                )
                .await?;
            let batch = list["pages"]
                .as_array()
                .map(|pages| {
                    pages
                        .iter()
                        .filter_map(|p| p["slug"].as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let total = list["total"].as_u64().unwrap_or(0) as usize;
            let got = batch.len();
            slugs.extend(batch);
            if got == 0 || slugs.len() >= total {
                break;
            }
            page += 1;
        }
        Ok(slugs)
    }

    async fn read(&self, slug: &str) -> Result<String> {
        self.kb
            .call("wiki_content_read", json!({"uri": slug}))
            .await
    }

    async fn write(&self, slug: &str, content: &str) -> Result<()> {
        self.kb
            .call(
                "wiki_content_write",
                json!({"uri": slug, "content": content}),
            )
            .await?;
        Ok(())
    }

    async fn ingest(&self) -> Result<serde_json::Value> {
        self.kb
            .call_json("wiki_ingest", json!({"path": WORK_ROOT}))
            .await
    }
}

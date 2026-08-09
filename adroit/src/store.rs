//! The KB door: decision pages live in a wiki, reached over the transport.
//!
//! The KB is the state store — there is no filesystem mode; local dev is a
//! local appliance (`just kb-dev`). [`PageStore`] is the narrow set of
//! operations the verbs need, so the lifecycle oracle can run against an
//! in-memory fake; [`KbStore`] is the real thing over `como-kb-client`
//! (`KB_URL` / `KB_WIKI`). Ownership is a write boundary: adroit is the only
//! writer of `decision`/`plan` pages, and it registers their schemas on
//! first contact, idempotently — pages enter through the same admission
//! gates as everything else (`wiki_ingest`), and the gate's report is
//! surfaced, not assumed.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use como_kb_client::{KbClient, KbTarget};
use serde_json::json;

use crate::naming::{AdrRef, NamingScheme};
use crate::page::{self, Decision, DecisionId};

/// The wiki section adroit's pages live under.
pub const DECISIONS_ROOT: &str = "decisions";

const DECISION_SCHEMA: &str = include_str!("../schemas/decision.json");
const PLAN_SCHEMA: &str = include_str!("../schemas/plan.json");

/// The store operations the verbs are written against.
#[async_trait]
pub trait PageStore: Send + Sync {
    /// First contact: register adroit's page classes (`decision`, `plan`),
    /// idempotently. Returns per-schema outcomes (`registered` /
    /// `unchanged`); a conflict (someone else's type under our name) is
    /// fatal.
    async fn ensure_schemas(&self) -> Result<Vec<String>>;
    /// Slugs of every `decision` page in the wiki.
    async fn decision_slugs(&self) -> Result<Vec<String>>;
    /// Raw page content (frontmatter + body) by slug.
    async fn read(&self, slug: &str) -> Result<String>;
    /// Write raw page content by slug.
    async fn write(&self, slug: &str, content: &str) -> Result<()>;
    /// Admit `decisions/` through the gates (validate, commit, index) and
    /// return the gate's report.
    async fn ingest(&self) -> Result<serde_json::Value>;
}

/// The transport store: one connected appliance session.
pub struct KbStore {
    kb: KbClient,
}

impl KbStore {
    /// Connect via the suite pair (`KB_URL` / `KB_WIKI`), resolved through
    /// the suite-wide discovery order (env, cwd `.env`, `~/.config/taps/env`).
    /// Fails visibly when no KB is configured — the KB is the state store,
    /// there is nothing to fall back to.
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
impl PageStore for KbStore {
    async fn ensure_schemas(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for (type_name, schema) in [("decision", DECISION_SCHEMA), ("plan", PLAN_SCHEMA)] {
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

    async fn decision_slugs(&self) -> Result<Vec<String>> {
        // Page through the listing until the reported total is collected.
        let mut slugs = Vec::new();
        let mut page = 1;
        loop {
            let list = self
                .kb
                .call_json(
                    "wiki_list",
                    json!({"type": "decision", "page": page, "page_size": 200}),
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
            .call_json("wiki_ingest", json!({"path": DECISIONS_ROOT}))
            .await
    }
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

/// One loaded decision page.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The page's slug (`decisions/<stem>`).
    pub slug: String,
    pub decision: Decision,
}

impl Entry {
    /// The persisted display reference (always assigned on a loaded page).
    pub fn reference(&self) -> String {
        self.decision
            .display_reference()
            .unwrap_or_else(|_| self.slug.clone())
    }

    /// True when this entry is what a supersession/link `target` string (an
    /// id, a slug, or a persisted display reference) points at.
    pub fn is_target_of(&self, target: &str) -> bool {
        let t = target.trim();
        self.slug == t
            || self.slug.strip_prefix(&format!("{DECISIONS_ROOT}/")) == Some(t)
            || DecisionId::parse(t) == Some(self.decision.id)
            || self.reference() == t
    }
}

/// Every decision page in the wiki, loaded through the store.
#[derive(Debug, Default)]
pub struct Corpus {
    pub entries: Vec<Entry>,
}

impl Corpus {
    /// Load and parse every decision page. A page that fails to parse is a
    /// hard error — the admission gate validated it on the way in, so a
    /// broken page means the corpus needs a human, not a silent skip.
    pub async fn load(store: &dyn PageStore) -> Result<Self> {
        let mut entries = Vec::new();
        for slug in store.decision_slugs().await? {
            let content = store.read(&slug).await?;
            let decision = page::deserialize(&content)
                .with_context(|| format!("cannot parse decision page {slug}"))?;
            entries.push(Entry { slug, decision });
        }
        // Stable order for lists and allocation: numeric references first
        // (ascending), then slugs alphabetically.
        entries.sort_by(|a, b| {
            let key = |e: &Entry| match &e.decision.reference {
                Some(AdrRef::Number(n)) => (0, *n, String::new()),
                Some(AdrRef::Slug(s)) => (1, 0, s.clone()),
                None => (2, 0, e.slug.clone()),
            };
            key(a).cmp(&key(b))
        });
        Ok(Self { entries })
    }

    /// The references already present (for allocation).
    pub fn refs(&self) -> Vec<AdrRef> {
        self.entries
            .iter()
            .filter_map(|e| e.decision.reference.clone())
            .collect()
    }

    /// Resolve a caller-supplied identifier — a display reference
    /// (`ADR-0007`, `7`), a slug (with or without the `decisions/` prefix),
    /// or a page id (ULID) — to exactly one entry.
    pub fn resolve(&self, scheme: NamingScheme, input: &str) -> Result<&Entry> {
        let t = input.trim();
        if t.is_empty() {
            bail!("empty decision identifier");
        }

        // Address identities first (slug, id) — they are exact.
        if let Some(e) = self
            .entries
            .iter()
            .find(|e| e.slug == t || e.slug.strip_prefix(&format!("{DECISIONS_ROOT}/")) == Some(t))
        {
            return Ok(e);
        }
        if let Some(id) = DecisionId::parse(t)
            && let Some(e) = self.entries.iter().find(|e| e.decision.id == id)
        {
            return Ok(e);
        }

        // Then the scheme's display reference (`ADR-0007`, `7`, a date slug,
        // a uuid prefix).
        let matches: Vec<&Entry> = match scheme.parse_ref(t) {
            Some(query) => self
                .entries
                .iter()
                .filter(|e| {
                    e.decision
                        .reference
                        .as_ref()
                        .is_some_and(|stored| scheme.ref_matches(stored, &query))
                })
                .collect(),
            None => Vec::new(),
        };
        match matches.as_slice() {
            [one] => Ok(one),
            [] => bail!(
                "no decision matches '{input}' (known: {})",
                self.known_references()
            ),
            many => bail!(
                "'{input}' is ambiguous — it matches {}",
                many.iter()
                    .map(|e| e.reference())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    fn known_references(&self) -> String {
        if self.entries.is_empty() {
            return "none — the wiki has no decisions yet".into();
        }
        self.entries
            .iter()
            .map(Entry::reference)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

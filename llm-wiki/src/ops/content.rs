use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::Serialize;
use tantivy::{
    Searcher, Term,
    query::TermQuery,
    schema::{IndexRecordOption, Value},
};

use crate::config;
use crate::engine::EngineState;
use crate::git;
use crate::index_schema::IndexSchema;
use crate::markdown;
use crate::slug::{ReadTarget, Slug, WikiUri, resolve_read_target};

/// A page that links to a given target — slug and display title.
#[derive(Debug, Clone, Serialize)]
pub struct BacklinkRef {
    /// Slug of the linking page.
    pub slug: String,
    /// Title of the linking page.
    pub title: String,
}

/// Query the index for all pages that contain a link to `target_slug`.
pub fn backlinks_query(
    searcher: &Searcher,
    is: &IndexSchema,
    target_slug: &str,
) -> Result<Vec<BacklinkRef>> {
    let f_body_links = is.field("body_links");
    let f_slug = is.field("slug");
    let f_title = is.field("title");

    let term = Term::from_field_text(f_body_links, target_slug);
    let query = TermQuery::new(term, IndexRecordOption::Basic);

    let doc_addrs = searcher.search(&query, &tantivy::collector::DocSetCollector)?;

    let mut refs: Vec<BacklinkRef> = doc_addrs
        .into_iter()
        .filter_map(|addr| {
            let doc: tantivy::TantivyDocument = searcher.doc(addr).ok()?;
            let slug = doc
                .get_first(f_slug)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = doc
                .get_first(f_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if slug.is_empty() {
                None
            } else {
                Some(BacklinkRef { slug, title })
            }
        })
        .collect();

    refs.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(refs)
}

/// Return all pages linking to `target_slug` in the named wiki — by slug
/// or by the target page's stable id.
pub fn backlinks_for(
    engine: &EngineState,
    wiki_name: &str,
    target_slug: &str,
) -> Result<Vec<BacklinkRef>> {
    let wiki = engine.wiki(wiki_name)?;
    let searcher = wiki.index_manager.searcher()?;
    let is = &wiki.index_schema;

    let mut refs = backlinks_query(&searcher, is, target_slug)?;
    if let Some(id) = crate::search::id_for_slug(&searcher, is, target_slug)? {
        let by_id = backlinks_query(&searcher, is, &id.to_string())?;
        let seen: std::collections::HashSet<String> = refs.iter().map(|r| r.slug.clone()).collect();
        refs.extend(by_id.into_iter().filter(|r| !seen.contains(&r.slug)));
        refs.sort_by(|a, b| a.slug.cmp(&b.slug));
    }
    Ok(refs)
}

/// Result of a content read — page text, asset list, or binary asset.
#[derive(Debug)]
pub enum ContentReadResult {
    /// Page markdown content (possibly with frontmatter stripped).
    Page(String),
    /// List of co-located asset filenames.
    Assets(Vec<String>),
    /// The resolved target is a binary file — read it directly from disk.
    Binary,
}

/// Read a wiki page or list its co-located assets.
pub fn content_read(
    engine: &EngineState,
    uri: &str,
    wiki_flag: Option<&str>,
    no_frontmatter: bool,
    list_assets: bool,
) -> Result<ContentReadResult> {
    let (entry, slug) = engine.resolve_address(uri, wiki_flag)?;
    let content_root = engine.wiki(&entry.name)?.content_root.clone();

    if list_assets {
        let assets = markdown::list_assets(&slug, &content_root)?;
        return Ok(ContentReadResult::Assets(assets));
    }

    match resolve_read_target(slug.as_str(), &content_root)? {
        ReadTarget::Page(_) => {
            let wiki_cfg = config::load_wiki(&PathBuf::from(&entry.path)).unwrap_or_default();
            let resolved = config::resolve(&engine.config, &wiki_cfg);
            let strip = no_frontmatter || resolved.read.no_frontmatter;
            let content = markdown::read_page(&slug, &content_root, strip)?;
            Ok(ContentReadResult::Page(content))
        }
        ReadTarget::Asset(parent_slug, filename) => {
            let parent = Slug::try_from(parent_slug.as_str())?;
            let bytes = markdown::read_asset(&parent, &filename, &content_root)?;
            match String::from_utf8(bytes) {
                Ok(text) => Ok(ContentReadResult::Page(text)),
                Err(_) => Ok(ContentReadResult::Binary),
            }
        }
    }
}

/// Result of a content write operation.
pub struct WriteResult {
    /// Number of bytes written to disk.
    pub bytes_written: usize,
    /// Absolute path of the written file.
    pub path: PathBuf,
}

/// Write content to a wiki page identified by slug or URI.
pub fn content_write(
    engine: &EngineState,
    uri: &str,
    wiki_flag: Option<&str>,
    content: &str,
) -> Result<WriteResult> {
    let (_entry, slug) = engine.resolve_address(uri, wiki_flag)?;
    let content_root = engine.wiki(&_entry.name)?.content_root.clone();
    let path = markdown::write_page(slug.as_str(), content, &content_root)?;
    Ok(WriteResult {
        bytes_written: content.len(),
        path,
    })
}

/// Result of creating a new wiki page or section.
#[derive(Debug)]
pub struct ContentNewResult {
    /// `wiki://` URI for the created page.
    pub uri: String,
    /// Slug of the created page.
    pub slug: String,
    /// Stable page id written to frontmatter, if one was assigned.
    pub id: Option<ulid::Ulid>,
    /// Absolute filesystem path of the created file.
    pub path: PathBuf,
    /// Absolute path to the wiki root directory.
    pub content_root: PathBuf,
    /// True if the page was created as a bundle (folder + index.md).
    pub bundle: bool,
}

/// Create a new wiki page or section with scaffolded frontmatter.
#[allow(clippy::too_many_arguments)]
pub fn content_new(
    engine: &EngineState,
    uri: &str,
    wiki_flag: Option<&str>,
    section: bool,
    bundle: bool,
    name: Option<&str>,
    type_: Option<&str>,
    id: Option<ulid::Ulid>,
) -> Result<ContentNewResult> {
    let (entry, slug) = WikiUri::resolve(uri, wiki_flag, &engine.config)?;
    let repo_root = PathBuf::from(&entry.path);
    let content_root = engine.wiki(&entry.name)?.content_root.clone();

    if section && id.is_some() {
        bail!("sections do not carry a page id");
    }

    let type_name = if section {
        "section"
    } else {
        type_.unwrap_or("page")
    };
    let body_template = resolve_body_template(&repo_root, type_name);

    let path = if section {
        markdown::create_section(&slug, &content_root, body_template.as_deref())?
    } else {
        markdown::create_page(
            &slug,
            bundle,
            &content_root,
            name,
            type_,
            id,
            body_template.as_deref(),
        )?
    };

    Ok(ContentNewResult {
        uri: format!("wiki://{}/{slug}", entry.name),
        slug: slug.as_str().to_string(),
        id,
        path,
        content_root,
        bundle,
    })
}

/// Resolve a body template for a type.
/// 1. `schemas/<type>.md` in the wiki repo
/// 2. Embedded default template
/// 3. None
fn resolve_body_template(repo_root: &Path, type_name: &str) -> Option<String> {
    let template_path = repo_root.join("schemas").join(format!("{type_name}.md"));
    if template_path.is_file() {
        return std::fs::read_to_string(&template_path).ok();
    }
    crate::default_schemas::embedded_body_template(type_name).map(|s| s.to_string())
}

/// Result of committing pending changes: the admission event plus what
/// the index consumer did with it.
#[derive(Debug, serde::Serialize)]
pub struct CommitReport {
    /// Commit hash, or empty string if there was nothing to commit.
    pub commit: String,
    /// Pages (re)indexed by the post-commit index update.
    pub indexed: usize,
    /// Index entries removed by the post-commit index update.
    pub index_deleted: usize,
    /// Non-fatal problems (an un-indexed commit is reported here, not hidden).
    pub warnings: Vec<String>,
}

/// Commit specified slugs (or all uncommitted files) to git, then update
/// the search index — engine commits never fire the managed hooks (git2
/// doesn't run them), so this path is both the admission gate and the
/// index consumer.
pub fn content_commit(
    engine: &EngineState,
    manager: &crate::engine::WikiEngine,
    wiki_name: &str,
    slugs: &[String],
    all: bool,
    message: Option<&str>,
) -> Result<CommitReport> {
    let wiki = engine.wiki(wiki_name)?;

    if slugs.is_empty() && !all {
        bail!("specify slugs or --all");
    }

    // Resolve the file set first — it feeds the gate and then the commit.
    let mut paths = Vec::new();
    if all {
        for rel in git::changed_worktree_paths(&wiki.repo_root)? {
            let p = wiki.repo_root.join(rel);
            if p.starts_with(&wiki.content_root) {
                paths.push(p);
            }
        }
    } else {
        for s in slugs {
            let slug = Slug::try_from(s.as_str())?;
            let resolved = slug.resolve(&wiki.content_root)?;
            if resolved.file_name() == Some(std::ffi::OsStr::new("index.md")) {
                let bundle_dir = resolved.parent().unwrap();
                for entry in walkdir::WalkDir::new(bundle_dir)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.path().is_file() {
                        paths.push(entry.path().to_path_buf());
                    }
                }
            } else {
                paths.push(resolved);
            }
        }
    }

    // The validation gate is the managed pre-commit hook — which engine
    // commits never fire. So the gate runs here, in the write path itself,
    // with the same validation ingest applies before its commit. Hard
    // failures refuse the whole commit; warnings ride in the report.
    let resolved_cfg = wiki.resolved_config(&engine.config);
    let mut gate_warnings = Vec::new();
    let mut gate_errors = Vec::new();
    for p in &paths {
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        // A file that no longer exists is a deletion — nothing to validate.
        let Ok(raw) = std::fs::read_to_string(p) else {
            continue;
        };
        let content = crate::ingest::normalize_line_endings(&raw);
        let page = crate::frontmatter::parse(&content);
        let name = p
            .strip_prefix(&wiki.content_root)
            .unwrap_or(p)
            .display()
            .to_string();
        if page.frontmatter.is_empty() {
            gate_warnings.push(format!("{name}: no frontmatter found"));
            continue;
        }
        match wiki
            .type_registry
            .validate(&page.frontmatter, &resolved_cfg.validation.type_strictness)
        {
            Ok(ws) => gate_warnings.extend(ws.into_iter().map(|w| format!("{name}: {w}"))),
            Err(e) => gate_errors.push(format!("{name}: {e}")),
        }
    }
    if !gate_errors.is_empty() {
        bail!(
            "refusing to commit — validation failed:\n  {}",
            gate_errors.join("\n  ")
        );
    }

    let hash = if all {
        let msg = message.unwrap_or("commit: all");
        git::commit(&wiki.repo_root, msg)?
    } else {
        let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        let default_msg = format!("commit: {}", slugs.join(", "));
        let msg = message.unwrap_or(&default_msg);
        git::commit_paths(&wiki.repo_root, &path_refs, msg)?
    };

    let mut report = CommitReport {
        commit: hash,
        indexed: 0,
        index_deleted: 0,
        warnings: gate_warnings,
    };
    if report.commit.is_empty() {
        return Ok(report);
    }

    match manager.refresh_index(wiki_name) {
        Ok(r) => {
            report.indexed = r.updated;
            report.index_deleted = r.deleted;
        }
        Err(e) => report.warnings.push(format!(
            "committed but index update failed ({e}) — search is stale; run wiki_admin_index_rebuild"
        )),
    }
    Ok(report)
}

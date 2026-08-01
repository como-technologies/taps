use std::path::{Path, PathBuf};

use crate::adr::{Adr, Number, Status};
use crate::config::{Config, DateSource};
use crate::naming::{AdrRef, NamingScheme};

/// Errors that can occur during ADR storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("ADR directory not found: {0}")]
    NotFound(PathBuf),

    /// The `--dir` target is not a KB space (ADR-0020): every command operates
    /// against a space root carrying a `wiki.toml`. The message names the
    /// bootstrap path.
    #[error(
        "not a KB space (no wiki.toml): {0} — create one with `llm-wiki spaces create` \
         (or scaffold wiki.toml + wiki/decisions) and seed it with \
         `adroit seed --from <legacy-dir>`"
    )]
    NotASpace(PathBuf),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Serializing/parsing a KB decision page failed. Carries the typed
    /// `FrontmatterError` so callers can inspect the cause.
    #[error("failed to parse ADR: {0}")]
    Frontmatter(#[from] crate::frontmatter::FrontmatterError),

    /// A structural problem the store detected itself — distinct from a page
    /// (de)serialization error.
    #[error("{0}")]
    Parse(String),

    #[error("no ADR found with number {0}")]
    NumberNotFound(Number),
}

/// Outcome of a [`Store::relink`] pass.
#[derive(Debug, Clone, Default)]
pub struct RelinkReport {
    /// Number of files whose content was (or would be) rewritten.
    pub files_changed: usize,
    /// Total cross-ADR links rewritten across those files.
    pub links_rewritten: usize,
    /// Files (relative to the store root) that changed — for dry-run display.
    pub changed_files: Vec<PathBuf>,
}

/// Which typed relational link to add/remove via [`Store::set_links_ref`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    RelatesTo,
    DependsOn,
    Refines,
}

/// Outcome of [`Store::renumber`].
#[derive(Debug, Default)]
pub struct RenumberReport {
    pub from: u32,
    pub to: u32,
    /// Files rewritten (the renamed ADR + every file with an inbound reference).
    pub files_updated: usize,
}

/// How a [`Store`] is configured (the KB decision page is the one on-disk
/// profile, flat is the one layout — ADR-0020).
#[derive(Debug, Clone, Default)]
pub struct StoreOptions {
    /// Age (in days) past which a still-`Proposed` ADR is flagged review-due
    /// even with no explicit `review_by`. `None` disables age-based flagging
    /// (deadline-only). Carried from config so the shared query layer can apply
    /// it identically across surfaces.
    pub review_overdue_days: Option<u32>,
    /// Where the query layer reads ADR dates/lifecycle from (carried from config).
    pub date_source: DateSource,
    /// How ADR identifiers / filenames are formed (carried from config). Drives
    /// `write`/`read` identity + filename via the `naming` seam.
    pub naming: NamingScheme,
}

impl StoreOptions {
    /// Build options from resolved [`Config`] — the single place every surface
    /// (CLI, TUI, web) maps config to store options, so they open the store
    /// identically.
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            review_overdue_days: (cfg.review_overdue_days > 0).then_some(cfg.review_overdue_days),
            date_source: cfg.date_source,
            naming: cfg.naming,
        }
    }
}

/// Parse the leading zero-padded number from a filename like `0006-foo.md`.
fn number_from_filename(path: &Path) -> Option<Number> {
    let name = path.file_name()?.to_str()?;
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().ok().map(Number::new)
}

/// True if this directory entry is an ADR file (`*.md`, not `README.md`).
fn is_adr_file(path: &Path) -> bool {
    if path.extension().is_none_or(|ext| ext != "md") {
        return false;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    !name.eq_ignore_ascii_case("README.md") && !name.eq_ignore_ascii_case("adr-template.md")
}

/// Manages reading and writing ADRs on disk.
#[derive(Debug)]
pub struct Store {
    /// The corpus root: `<space>/<wiki_root>/decisions`.
    root: PathBuf,
    opts: StoreOptions,
}

/// Resolve the corpus root for a `--dir` target (ADR-0020).
///
/// The corpus is a KB space: `dir` must carry a `wiki.toml`, and decision
/// pages live at `<space>/<wiki_root>/decisions` (default `wiki/decisions`,
/// honoring the space's configured `wiki_root`). A directory without
/// `wiki.toml` is a hard error naming the bootstrap path — path mode retired
/// with the markdown profile.
fn corpus_root(dir: PathBuf) -> Result<PathBuf, StoreError> {
    let wiki_toml = dir.join("wiki.toml");
    if !wiki_toml.is_file() {
        return Err(StoreError::NotASpace(dir));
    }
    #[derive(serde::Deserialize, Default)]
    struct SpaceConfig {
        #[serde(default)]
        wiki_root: Option<String>,
    }
    let wiki_root = std::fs::read_to_string(&wiki_toml)
        .ok()
        .and_then(|raw| toml::from_str::<SpaceConfig>(&raw).ok())
        .and_then(|cfg| cfg.wiki_root)
        .unwrap_or_else(|| "wiki".to_string());
    Ok(dir.join(wiki_root).join("decisions"))
}

impl Store {
    /// Open an existing ADR store at the given space root with default options.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        Self::open_with(root, StoreOptions::default())
    }

    /// Open an existing ADR store with explicit options.
    ///
    /// `root` is the KB space root (it must contain a `wiki.toml`); the corpus
    /// root is `<wiki_root>/decisions` inside the space (ADR-0020). A missing
    /// decisions directory is a hard error — only the scaffolding verbs
    /// (`new` / `import` / forge `init` / `seed`) create it.
    pub fn open_with(root: impl Into<PathBuf>, opts: StoreOptions) -> Result<Self, StoreError> {
        let root = corpus_root(root.into())?;
        if !root.is_dir() {
            return Err(StoreError::NotFound(root));
        }
        Ok(Self { root, opts })
    }

    /// Open an ADR store with default options, creating the decisions dir
    /// inside the space if missing. Never creates the space itself.
    pub fn open_or_create(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        Self::open_or_create_with(root, StoreOptions::default())
    }

    /// Open an ADR store with explicit options, creating the decisions dir
    /// inside the space if missing. Never creates the space itself (`root`
    /// must already carry a `wiki.toml`).
    pub fn open_or_create_with(
        root: impl Into<PathBuf>,
        opts: StoreOptions,
    ) -> Result<Self, StoreError> {
        let root = corpus_root(root.into())?;
        if !root.is_dir() {
            std::fs::create_dir_all(&root)?;
        }
        Ok(Self { root, opts })
    }

    /// Return the corpus root of this store (`<space>/<wiki_root>/decisions`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Return the store options.
    pub fn options(&self) -> &StoreOptions {
        &self.opts
    }

    /// List all ADR files in the store, sorted by number then name.
    pub fn list_files(&self) -> Result<Vec<PathBuf>, StoreError> {
        let mut files = read_md_files(&self.root)?;
        files.sort_by(|a, b| {
            let na = number_from_filename(a);
            let nb = number_from_filename(b);
            na.cmp(&nb).then_with(|| a.cmp(b))
        });
        Ok(files)
    }

    /// Return the next available ADR number: max + 1.
    ///
    /// Test-only: production allocates identity through the naming seam
    /// (`next_ref`); this numeric helper backs unit tests, so it's
    /// `#[cfg(test)]` and never compiled into the binary.
    #[cfg(test)]
    fn next_number(&self) -> Result<Number, StoreError> {
        let files = self.list_files()?;
        let max = files
            .iter()
            .filter_map(|p| number_from_filename(p).map(|n| n.get()))
            .max()
            .unwrap_or(0);
        Ok(Number::new(max + 1))
    }

    /// The identifier the next new ADR would be assigned, under the configured
    /// naming scheme. (`title`/`id_slug` feed the date/uuid schemes.)
    pub fn next_ref(&self, title: &str, id_slug: &str) -> Result<AdrRef, StoreError> {
        let existing: Vec<AdrRef> = self
            .list_with_paths()?
            .iter()
            .map(|(_, a)| a.reference())
            .collect();
        Ok(self
            .opts
            .naming
            .assign(&existing, title, today_local(), id_slug))
    }

    /// The on-disk path an ADR maps to — the corpus root + the scheme
    /// filename. This is the **read-only** half of [`Store::write`], so a
    /// `new --dry-run` can show where the ADR *would* land without creating it.
    /// The ADR must already carry its identity (`number`/`slug`).
    pub fn target_path(&self, adr: &Adr) -> PathBuf {
        self.root
            .join(self.opts.naming.filename(&adr.reference(), &adr.title))
    }

    /// Write an ADR to disk as a KB decision page, using the configured naming
    /// scheme. Assigns an identity (via the scheme) if the ADR doesn't have
    /// one yet.
    pub fn write(&self, adr: &mut Adr) -> Result<PathBuf, StoreError> {
        if adr.number.is_none() && adr.slug.is_none() {
            let r = self.next_ref(&adr.title, &adr.id.slug())?;
            apply_ref(adr, r);
        }
        let content = crate::frontmatter::serialize(adr)?;
        let path = self.target_path(adr);
        if let Some(parent) = path.parent()
            && !parent.is_dir()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Read a single ADR from a file path, setting its identity per the scheme.
    pub fn read(&self, path: &Path) -> Result<Adr, StoreError> {
        let content = std::fs::read_to_string(path)?;
        let mut adr = crate::frontmatter::deserialize(&content)?;
        if let Some(r) = self.opts.naming.parse(path) {
            apply_ref(&mut adr, r);
        }
        Ok(adr)
    }

    /// Find an ADR's file by its scheme identity (number or slug/uuid prefix).
    pub fn find_path_by_ref(&self, r: &AdrRef) -> Result<PathBuf, StoreError> {
        self.list_files()?
            .into_iter()
            .find(|p| {
                self.opts
                    .naming
                    .parse(p)
                    .is_some_and(|stored| self.opts.naming.ref_matches(&stored, r))
            })
            .ok_or_else(|| {
                StoreError::Parse(format!(
                    "no ADR found with id {}",
                    self.opts.naming.display(r)
                ))
            })
    }

    /// Find the file path for an ADR by its sequential number.
    pub fn find_path_by_number(&self, number: Number) -> Result<PathBuf, StoreError> {
        self.list_files()?
            .into_iter()
            .find(|p| number_from_filename(p) == Some(number))
            .ok_or(StoreError::NumberNotFound(number))
    }

    /// List all ADRs in the store, parsed from disk.
    pub fn list(&self) -> Result<Vec<Adr>, StoreError> {
        Ok(self
            .list_with_paths()?
            .into_iter()
            .map(|(_, adr)| adr)
            .collect())
    }

    /// List all ADRs paired with their on-disk file path. The query layer needs
    /// the path to look up each ADR's git history (creation date + lifecycle).
    pub fn list_with_paths(&self) -> Result<Vec<(PathBuf, Adr)>, StoreError> {
        self.list_files()?
            .into_iter()
            .map(|p| {
                let adr = self.read(&p)?;
                Ok((p, adr))
            })
            .collect()
    }

    /// Rewrite every cross-ADR relative link across the store so it points at
    /// the current location of the ADR it references (see [`crate::links`]).
    ///
    /// Idempotent: a file whose links are already canonical is left
    /// byte-identical and not rewritten. Repairs links left stale by a
    /// `renumber` or edits made outside adroit. Duplicate ADR numbers are
    /// skipped (ambiguous — surfaced by `adroit check`).
    ///
    /// With `apply == false` nothing is written — the returned report describes
    /// what *would* change (for `adroit relink --dry-run`).
    pub fn relink(&self, apply: bool) -> Result<RelinkReport, StoreError> {
        let entries = self.list_with_paths()?;
        let by_ref = Self::link_resolver_map(&entries);

        let mut report = RelinkReport::default();
        for (path, _) in &entries {
            let dir = path.parent().unwrap_or_else(|| Path::new(""));
            let original = std::fs::read_to_string(path)?;
            let (rewritten, changed) = crate::links::rewrite_links(&original, dir, |target| {
                self.opts
                    .naming
                    .ref_in_link(target)
                    .and_then(|r| by_ref.get(&r).cloned())
            });
            if changed > 0 && rewritten != original {
                if apply {
                    std::fs::write(path, &rewritten)?;
                }
                report.files_changed += 1;
                report.links_rewritten += changed;
                report.changed_files.push(rel_to(&self.root, path));
            }
        }
        Ok(report)
    }

    /// Map each ADR's scheme identity to its current file, so a link target's
    /// ref (via the seam) resolves to where that ADR now lives. Identities seen
    /// more than once are ambiguous duplicates and are left out (their links are
    /// kept byte-for-byte and flagged by `check`).
    fn link_resolver_map(entries: &[(PathBuf, Adr)]) -> std::collections::HashMap<AdrRef, PathBuf> {
        let mut seen: std::collections::HashMap<AdrRef, usize> = std::collections::HashMap::new();
        for (_, adr) in entries {
            *seen.entry(adr.reference()).or_default() += 1;
        }
        let mut by_ref: std::collections::HashMap<AdrRef, PathBuf> =
            std::collections::HashMap::new();
        for (path, adr) in entries {
            let r = adr.reference();
            if seen.get(&r) == Some(&1) {
                by_ref.insert(r, path.clone());
            }
        }
        by_ref
    }

    /// Renumber a sequential ADR from `old` to `new`: rename the file (slug
    /// preserved), retarget + relabel every inbound reference, then relink.
    /// Resolves a duplicate-number collision. `file` disambiguates when two
    /// files share `old`. Errors if `new` is taken, `old` is missing, or `old`
    /// is ambiguous without `file`.
    pub fn renumber(
        &self,
        old: Number,
        new: Number,
        file: Option<&Path>,
    ) -> Result<RenumberReport, StoreError> {
        let candidates: Vec<PathBuf> = self
            .list_files()?
            .into_iter()
            .filter(|p| number_from_filename(p) == Some(old))
            .collect();
        let old_path = match file {
            Some(f) if candidates.iter().any(|c| c == f) => f.to_path_buf(),
            Some(f) => {
                return Err(StoreError::Parse(format!(
                    "{} is not an ADR-{old} file",
                    f.display()
                )));
            }
            None => match candidates.as_slice() {
                [] => return Err(StoreError::NumberNotFound(old)),
                [one] => one.clone(),
                many => {
                    let list = many
                        .iter()
                        .map(|p| rel_to(&self.root, p).display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(StoreError::Parse(format!(
                        "ADR-{old} is ambiguous ({} files) — pass --file <path>: {list}",
                        many.len()
                    )));
                }
            },
        };
        if self
            .list_files()?
            .iter()
            .any(|p| number_from_filename(p) == Some(new))
        {
            return Err(StoreError::Parse(format!("ADR-{new} already exists")));
        }

        let old_base = old_path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("ADR path has a filename")
            .to_string();
        let ndigits = old_base.chars().take_while(|c| c.is_ascii_digit()).count();
        let new_base = format!("{:04}{}", new.get(), &old_base[ndigits..]);
        let new_path = old_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&new_base);
        let old_label = format!("ADR-{:04}", old.get());
        let new_label = format!("ADR-{:04}", new.get());

        std::fs::rename(&old_path, &new_path)?;
        let mut report = RenumberReport {
            from: old.get(),
            to: new.get(),
            files_updated: 0,
        };

        // The renamed file: update its own `reference:` / self-references.
        let own = std::fs::read_to_string(&new_path)?;
        let own_new = own.replace(&old_label, &new_label);
        if own_new != own {
            std::fs::write(&new_path, own_new)?;
            report.files_updated += 1;
        }

        // Every other file: retarget + relabel inbound links to this ADR.
        for path in self.list_files()? {
            if path == new_path {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            let (mut rewritten, mut n) = crate::links::relabel_links_to(
                &content, &old_base, &new_base, &old_label, &new_label,
            );
            // Supersession + typed-link refs are bare numbers in the YAML
            // block, not markdown links, so the relabel above can't reach
            // them — remap them through the model so a renumber doesn't
            // strand e.g. another ADR's `superseded_by: <old>`.
            if let Some(remapped) = crate::frontmatter::remap_numeric_refs(&rewritten, old, new) {
                rewritten = remapped;
                n += 1;
            }
            if n > 0 && rewritten != content {
                std::fs::write(&path, rewritten)?;
                report.files_updated += 1;
            }
        }

        // Canonicalize any relative-path drift left by the rename.
        self.relink(true)?;
        Ok(report)
    }

    /// Change an ADR's status, rewriting its frontmatter in place (the file
    /// never moves — flat is the only layout, ADR-0020). Returns the path.
    pub fn set_status(&self, number: Number, new_status: Status) -> Result<PathBuf, StoreError> {
        let path = self.find_path_by_number(number)?;
        self.set_status_at(path, new_status, None)
    }

    /// Like [`Store::set_status`] but addressed by the scheme's [`AdrRef`] (so
    /// date/uuid ADRs, which have no number, can change status from the CLI).
    pub fn set_status_ref(&self, r: &AdrRef, new_status: Status) -> Result<PathBuf, StoreError> {
        let path = self.find_path_by_ref(r)?;
        self.set_status_at(path, new_status, None)
    }

    /// Mark `old` as superseded by `new` (both addressed by scheme identity):
    /// status becomes `superseded` and `superseded_by:` records the newer
    /// ADR's ref, rewritten in place.
    pub fn supersede(&self, new: &AdrRef, old: &AdrRef) -> Result<PathBuf, StoreError> {
        // Validate the new ADR exists before mutating the old one.
        self.find_path_by_ref(new)?;
        let old_path = self.find_path_by_ref(old)?;
        self.set_status_at(old_path, Status::Superseded, Some(new.clone()))
    }

    /// Core status-change at a known path (shared by the number- and ref-keyed
    /// public entry points). `superseded_by` carries the superseding ADR's
    /// [`AdrRef`] for the frontmatter field.
    fn set_status_at(
        &self,
        path: PathBuf,
        new_status: Status,
        superseded_by: Option<AdrRef>,
    ) -> Result<PathBuf, StoreError> {
        let mut adr = self.read(&path)?;
        adr.status = new_status;
        if let Some(new) = superseded_by {
            adr.superseded_by = Some(new);
        }
        let content = crate::frontmatter::serialize(&adr)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Replace ONLY an ADR's markdown body, preserving everything the page
    /// profile owns (frontmatter fields).
    ///
    /// This is the single write path for the in-TUI body editor. It mirrors
    /// [`Store::set_status`]/[`Store::supersede`]: read the ADR through the
    /// store, mutate one field (`body`), and re-serialize — so an unedited
    /// round-trip is byte-identical. Returns the path written.
    pub fn set_body(&self, number: Number, new_body: &str) -> Result<PathBuf, StoreError> {
        let path = self.find_path_by_number(number)?;
        self.set_body_at(path, new_body)
    }

    /// Like [`Store::set_body`] but addressed by the scheme's [`AdrRef`].
    pub fn set_body_ref(&self, r: &AdrRef, new_body: &str) -> Result<PathBuf, StoreError> {
        let path = self.find_path_by_ref(r)?;
        self.set_body_at(path, new_body)
    }

    fn set_body_at(&self, path: PathBuf, new_body: &str) -> Result<PathBuf, StoreError> {
        let mut adr = self.read(&path)?;
        adr.body = new_body.to_string();
        let content = crate::frontmatter::serialize(&adr)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Set (or clear) an ADR's optional `review_by` deadline: the field is
    /// updated through the `Adr` model and re-serialized in place. Passing
    /// `None` clears it. Returns the path written.
    pub fn set_review_by(
        &self,
        number: Number,
        review_by: Option<crate::adr::ReviewBy>,
    ) -> Result<PathBuf, StoreError> {
        let path = self.find_path_by_number(number)?;
        self.set_review_by_at(path, review_by)
    }

    /// Like [`Store::set_review_by`] but addressed by the scheme's [`AdrRef`].
    pub fn set_review_by_ref(
        &self,
        r: &AdrRef,
        review_by: Option<crate::adr::ReviewBy>,
    ) -> Result<PathBuf, StoreError> {
        let path = self.find_path_by_ref(r)?;
        self.set_review_by_at(path, review_by)
    }

    /// Add or remove a typed relational link on the ADR addressed by `source`.
    /// Adding validates that `target` exists.
    pub fn set_links_ref(
        &self,
        source: &AdrRef,
        kind: LinkKind,
        target: &AdrRef,
        remove: bool,
    ) -> Result<PathBuf, StoreError> {
        if !remove {
            self.find_path_by_ref(target)?; // refuse linking to a missing ADR
        }
        let path = self.find_path_by_ref(source)?;
        let mut adr = self.read(&path)?;
        let links = match kind {
            LinkKind::RelatesTo => &mut adr.relates_to,
            LinkKind::DependsOn => &mut adr.depends_on,
            LinkKind::Refines => &mut adr.refines,
        };
        if remove {
            links.retain(|r| r != target);
        } else if !links.contains(target) {
            links.push(target.clone());
        }
        let content = crate::frontmatter::serialize(&adr)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    fn set_review_by_at(
        &self,
        path: PathBuf,
        review_by: Option<crate::adr::ReviewBy>,
    ) -> Result<PathBuf, StoreError> {
        let mut adr = self.read(&path)?;
        adr.review_by = review_by;
        let content = crate::frontmatter::serialize(&adr)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }
}

/// Read `*.md` ADR files (excluding README.md / adr-template.md) from a dir.
fn read_md_files(dir: &Path) -> Result<Vec<PathBuf>, StoreError> {
    Ok(std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_adr_file(p))
        .collect())
}

/// `path` relative to `root` (for display), or `path` itself if not under it.
fn rel_to(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// Apply a scheme-assigned [`AdrRef`] to an ADR's identity (public wrapper for
/// callers that assign the ref before `write`, e.g. to render the heading).
pub fn apply_ref_pub(adr: &mut Adr, r: &AdrRef) {
    apply_ref(adr, r.clone());
}

/// Apply a scheme-assigned [`AdrRef`] to an ADR's identity fields.
fn apply_ref(adr: &mut Adr, r: AdrRef) {
    match r {
        AdrRef::Number(n) => {
            adr.number = Some(Number::new(n));
            adr.slug = None;
        }
        AdrRef::Slug(s) => {
            adr.slug = Some(s);
            adr.number = None;
        }
    }
}

/// Today's local date (UTC fallback), for the date naming scheme.
fn today_local() -> time::Date {
    if let Some(d) = crate::config::today_override() {
        return d;
    }
    time::OffsetDateTime::now_local()
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
        .date()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::Adr;

    /// A fresh KB space (wiki.toml + wiki/decisions) with a store over it.
    fn space() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("wiki.toml"), "name = \"test\"\n").unwrap();
        let store = Store::open_or_create(tmp.path()).unwrap();
        (tmp, store)
    }

    #[test]
    fn open_or_create_scaffolds_decisions_inside_a_space() {
        let (tmp, store) = space();
        assert_eq!(store.root(), tmp.path().join("wiki").join("decisions"));
        assert!(store.root().is_dir());
    }

    #[test]
    fn open_missing_directory_errors() {
        let result = Store::open("/tmp/adroit-does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn open_a_non_space_dir_is_a_hard_error_naming_the_bootstrap() {
        let tmp = tempfile::tempdir().unwrap();
        let err = Store::open(tmp.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not a KB space"), "{msg}");
        assert!(msg.contains("llm-wiki spaces create"), "{msg}");
        assert!(msg.contains("adroit seed --from"), "{msg}");
        // open_or_create never creates the space itself.
        assert!(Store::open_or_create(tmp.path()).is_err());
        assert!(!tmp.path().join("wiki").exists());
    }

    #[test]
    fn open_a_space_without_decisions_errors_without_creating_it() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("wiki.toml"), "name = \"test\"\n").unwrap();
        assert!(matches!(
            Store::open(tmp.path()),
            Err(StoreError::NotFound(_))
        ));
        assert!(!tmp.path().join("wiki").join("decisions").exists());
    }

    #[test]
    fn write_and_list_round_trip() {
        let (_tmp, store) = space();

        let mut adr = Adr::new("Use PostgreSQL").unwrap();
        store.write(&mut adr).unwrap();

        let files = store.list_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("0001-use-postgresql.md"));
    }

    #[test]
    fn write_assigns_number_lazily() {
        let (_tmp, store) = space();

        let mut adr = Adr::new("Lazy numbering").unwrap();
        assert!(adr.number.is_none());

        store.write(&mut adr).unwrap();
        assert_eq!(adr.number, Some(Number::new(1)));
    }

    #[test]
    fn write_produces_frontmatter() {
        let (_tmp, store) = space();

        let mut adr = Adr::new("Use PostgreSQL").unwrap();
        let path = store.write(&mut adr).unwrap();
        let content = std::fs::read_to_string(path).unwrap();

        assert!(content.starts_with("---\n"));
        assert!(content.contains("id:"));
        assert!(content.contains("status: proposed"));
    }

    #[test]
    fn write_then_read_round_trip() {
        let (_tmp, store) = space();

        let mut adr = Adr::new("Use PostgreSQL").unwrap();
        let path = store.write(&mut adr).unwrap();
        let parsed = store.read(&path).unwrap();

        assert_eq!(parsed.id, adr.id);
        assert_eq!(parsed.number, adr.number);
        assert_eq!(parsed.title, adr.title);
        assert_eq!(parsed.status, adr.status);
        assert_eq!(parsed.created, adr.created);
    }

    #[test]
    fn next_number_starts_at_one_and_increments() {
        let (_tmp, store) = space();
        assert_eq!(store.next_number().unwrap(), Number::new(1));
        store.write(&mut Adr::new("First").unwrap()).unwrap();
        store.write(&mut Adr::new("Second").unwrap()).unwrap();
        assert_eq!(store.next_number().unwrap(), Number::new(3));
    }

    #[test]
    fn find_path_by_number_found() {
        let (_tmp, store) = space();
        store.write(&mut Adr::new("First").unwrap()).unwrap();
        store.write(&mut Adr::new("Second").unwrap()).unwrap();

        let path = store.find_path_by_number(Number::new(2)).unwrap();
        assert!(path.ends_with("0002-second.md"));
    }

    #[test]
    fn find_path_by_number_not_found() {
        let (_tmp, store) = space();
        assert!(store.find_path_by_number(Number::new(99)).is_err());
    }

    #[test]
    fn list_returns_parsed_adrs_skipping_readme() {
        let (_tmp, store) = space();
        store.write(&mut Adr::new("First").unwrap()).unwrap();
        store.write(&mut Adr::new("Second").unwrap()).unwrap();
        // A README in the decisions dir must be ignored.
        std::fs::write(store.root().join("README.md"), "# x").unwrap();

        let adrs = store.list().unwrap();
        assert_eq!(adrs.len(), 2);
        assert_eq!(adrs[0].title, "First");
        assert_eq!(adrs[1].title, "Second");
    }

    #[test]
    fn set_status_rewrites_frontmatter_in_place() {
        let (_tmp, store) = space();
        let mut adr = Adr::new("Adopt ADRs").unwrap();
        let path = store.write(&mut adr).unwrap();

        let new_path = store.set_status(Number::new(1), Status::Accepted).unwrap();
        assert_eq!(new_path, path, "flat: a status change never moves the file");
        let content = std::fs::read_to_string(&new_path).unwrap();
        assert!(content.contains("status: accepted"));
    }

    #[test]
    fn set_status_to_same_status_is_byte_identical() {
        let (_tmp, store) = space();
        let mut adr = Adr::new("Adopt ADRs").unwrap();
        adr.status = Status::Accepted;
        let path = store.write(&mut adr).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        store.set_status(Number::new(1), Status::Accepted).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn supersede_records_the_ref_in_place() {
        let (_tmp, store) = space();
        store.write(&mut Adr::new("Old Decision").unwrap()).unwrap(); // ADR 1
        store.write(&mut Adr::new("New Decision").unwrap()).unwrap(); // ADR 2

        let old_path = store
            .supersede(&AdrRef::Number(2), &AdrRef::Number(1))
            .unwrap();
        assert!(old_path.ends_with("0001-old-decision.md"));
        let old = store.read(&old_path).unwrap();
        assert_eq!(old.status, Status::Superseded);
        assert_eq!(old.superseded_by, Some(AdrRef::Number(2)));
    }

    #[test]
    fn set_body_rewrites_only_the_body() {
        let (_tmp, store) = space();
        let mut adr = Adr::new("Adopt ADRs").unwrap();
        adr.body = "## Context\n\nWe need a consistent way.".to_string();
        let path = store.write(&mut adr).unwrap();

        let edited = format!("{}\n\nAnd now an extra paragraph.", adr.body);
        let written = store.set_body(Number::new(1), &edited).unwrap();
        assert_eq!(written, path);

        let after = store.read(&written).unwrap();
        assert!(after.body.contains("And now an extra paragraph."));
        assert_eq!(after.title, adr.title);
        assert_eq!(after.status, adr.status);
        assert_eq!(after.id, adr.id);
    }

    #[test]
    fn set_body_unchanged_is_byte_identical() {
        let (_tmp, store) = space();
        let mut adr = Adr::new("Adopt ADRs").unwrap();
        adr.body = "## Context\n\nBody.".to_string();
        let path = store.write(&mut adr).unwrap();
        let original = std::fs::read_to_string(&path).unwrap();

        // Loading via the store and saving the same body must not change a byte.
        let loaded = store.read(&path).unwrap();
        let written = store.set_body(Number::new(1), &loaded.body).unwrap();
        assert_eq!(std::fs::read_to_string(&written).unwrap(), original);
    }

    #[test]
    fn set_review_by_round_trips_and_clears() {
        use crate::adr::ReviewBy;
        let (_tmp, store) = space();
        let path = store
            .write(&mut Adr::new("Use PostgreSQL").unwrap())
            .unwrap();
        let original = std::fs::read_to_string(&path).unwrap();

        let rb: ReviewBy = "2026-09-09".parse().unwrap();
        store.set_review_by(Number::new(1), Some(rb)).unwrap();
        let adr = store.read(&path).unwrap();
        assert_eq!(adr.review_by, Some(rb));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("review_by: 2026-09-09"));

        // Clearing restores the original bytes.
        store.set_review_by(Number::new(1), None).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn relink_dry_run_reports_without_writing() {
        let (_tmp, store) = space();
        store.write(&mut Adr::new("A").unwrap()).unwrap();
        store.write(&mut Adr::new("B").unwrap()).unwrap();
        // A stale link: ADR 2 now lives beside ADR 1, not in `../elsewhere/`.
        store
            .set_body(Number::new(1), "See [ADR-0002](../elsewhere/0002-b.md).")
            .unwrap();
        let p1 = store.find_path_by_number(Number::new(1)).unwrap();

        let before = std::fs::read_to_string(&p1).unwrap();
        let r = store.relink(false).unwrap();
        assert_eq!(r.files_changed, 1);
        assert_eq!(
            std::fs::read_to_string(&p1).unwrap(),
            before,
            "dry run must not write"
        );
        store.relink(true).unwrap();
        assert!(
            std::fs::read_to_string(&p1)
                .unwrap()
                .contains("./0002-b.md")
        );
    }

    #[test]
    fn a_custom_wiki_root_is_honored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("wiki.toml"),
            "name = \"test\"\nwiki_root = \"content\"\n",
        )
        .unwrap();
        assert_eq!(
            corpus_root(dir.path().into()).unwrap(),
            dir.path().join("content").join("decisions")
        );
    }
}

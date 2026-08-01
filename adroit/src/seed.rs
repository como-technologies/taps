//! `adroit seed` — the one-way bootstrap of a **legacy corpus** into a fresh
//! KB space (ADR-0020).
//!
//! A legacy corpus is the pre-KB shape: `# ADR-NNNN:`-style markdown documents,
//! either in by_status subdirectories (`proposed/ accepted/ rejected/
//! superseded/ deprecated/` — the directory names the status) or in one flat
//! directory (status from the `## Status` section). Each document is read
//! through the retained legacy parser ([`crate::format::parse_markdown`]) and
//! written into the target space as a KB decision page: number → `reference`,
//! title, status, supersession refs, and `review_by` move into frontmatter, a
//! legacy `Created:` date maps into the page's `created` (midnight UTC), and
//! the body is the document minus its H1 / banner / `## Status` region.
//!
//! Seed **refuses a target that already contains any ADR** — spaces are
//! ephemeral-first (stood up per checkout, seeded from the committed corpus),
//! so seeding is only ever into a fresh space; the refusal makes it safe.

use std::path::{Path, PathBuf};

use crate::adr::{Adr, Status};
use crate::store::{Store, StoreError};

/// Errors raised planning or applying a seed.
#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    #[error("legacy corpus directory not found: {0}")]
    SourceNotFound(PathBuf),

    #[error("no legacy ADR documents found under {0}")]
    Empty(PathBuf),

    /// The refusal that makes seed safe: it only fills a fresh space.
    #[error(
        "target space already contains {count} ADR(s) — `seed` only fills a fresh \
         (ephemeral) space; point --dir at a new one"
    )]
    TargetNotEmpty { count: usize },

    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: crate::format::FormatError,
    },

    #[error("{0}: legacy document has no `# ADR-NNNN:` number")]
    MissingNumber(PathBuf),

    #[error("duplicate legacy number ADR-{number:04}: {a} and {b}")]
    DuplicateNumber { number: u32, a: PathBuf, b: PathBuf },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Store(#[from] StoreError),
}

/// One legacy document seeded (or, on a dry run, planned) into the space.
#[derive(Debug, Clone)]
pub struct SeededPage {
    /// Display reference (`ADR-NNNN`).
    pub reference: String,
    pub title: String,
    pub status: Status,
    /// The target page path (planned on a dry run, written otherwise).
    pub path: PathBuf,
}

/// Outcome of a [`seed`] run.
#[derive(Debug, Default)]
pub struct SeedReport {
    pub pages: Vec<SeededPage>,
    /// `false` on a dry run (nothing written).
    pub applied: bool,
    /// Cross-ADR links healed by the post-seed relink (legacy by_status
    /// paths canonicalized to the flat space). Zero on a dry run.
    pub links_rewritten: usize,
}

/// The by_status directory names a legacy corpus may use (lowercase status
/// names — the pre-KB default; the directory supplies `dir_status`).
const LEGACY_STATUS_DIRS: [Status; 5] = Status::ALL;

/// Seed the legacy corpus at `from` into `store` (a fresh space). With
/// `apply == false` nothing is written — the report carries the plan.
pub fn seed(store: &Store, from: &Path, apply: bool) -> Result<SeedReport, SeedError> {
    if !from.is_dir() {
        return Err(SeedError::SourceNotFound(from.to_path_buf()));
    }
    let existing = store.list_files()?;
    if !existing.is_empty() {
        return Err(SeedError::TargetNotEmpty {
            count: existing.len(),
        });
    }

    // Parse every legacy document, tracking numbers for the collision guard.
    let mut parsed: Vec<(PathBuf, Adr)> = Vec::new();
    let mut by_number: std::collections::HashMap<u32, PathBuf> = std::collections::HashMap::new();
    for (path, dir_status) in discover(from)? {
        let content = std::fs::read_to_string(&path)?;
        let adr = crate::format::parse_markdown(&content, dir_status).map_err(|source| {
            SeedError::Parse {
                path: path.clone(),
                source,
            }
        })?;
        let Some(number) = adr.number else {
            return Err(SeedError::MissingNumber(path));
        };
        if let Some(other) = by_number.insert(number.get(), path.clone()) {
            return Err(SeedError::DuplicateNumber {
                number: number.get(),
                a: other,
                b: path,
            });
        }
        parsed.push((path, adr));
    }
    if parsed.is_empty() {
        return Err(SeedError::Empty(from.to_path_buf()));
    }
    parsed.sort_by_key(|(_, adr)| adr.number);

    let scheme = store.options().naming;
    let mut report = SeedReport {
        pages: Vec::with_capacity(parsed.len()),
        applied: apply,
        links_rewritten: 0,
    };
    for (_, mut adr) in parsed {
        let target = if apply {
            store.write(&mut adr)?
        } else {
            store.target_path(&adr)
        };
        report.pages.push(SeededPage {
            reference: scheme.display(&adr.reference()),
            title: adr.title.clone(),
            status: adr.status,
            path: target,
        });
    }
    // Legacy corpora carry ADR links written for the by_status directory
    // layout (`../accepted/NNNN-x.md`); the seeded space is flat, so heal
    // them in the same pass. Non-ADR links are untouched (and advisory in
    // `check`).
    if apply {
        report.links_rewritten = store
            .relink(true)
            .map_err(SeedError::Store)?
            .links_rewritten;
    }
    Ok(report)
}

/// Discover the legacy documents under `from`: `.md` files directly in the
/// directory (flat legacy — status from the `## Status` section) plus files in
/// any of the by_status subdirectories (the directory name supplies the
/// status). `README.md` / `adr-template.md` are skipped, as ever.
fn discover(from: &Path) -> Result<Vec<(PathBuf, Option<Status>)>, SeedError> {
    let mut out: Vec<(PathBuf, Option<Status>)> =
        md_files(from)?.into_iter().map(|p| (p, None)).collect();
    for status in LEGACY_STATUS_DIRS {
        let dir = from.join(status.to_string().to_lowercase());
        if dir.is_dir() {
            out.extend(md_files(&dir)?.into_iter().map(|p| (p, Some(status))));
        }
    }
    Ok(out)
}

/// `.md` files (excluding README/adr-template) directly in `dir`, sorted.
fn md_files(dir: &Path) -> Result<Vec<PathBuf>, SeedError> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|ext| ext == "md")
                && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    !n.eq_ignore_ascii_case("README.md")
                        && !n.eq_ignore_ascii_case("adr-template.md")
                })
        })
        .collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::AdrRef;
    use crate::store::Store;

    /// A fresh KB space with a store over it.
    fn space() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("wiki.toml"), "name = \"test\"\n").unwrap();
        let store = Store::open_or_create(tmp.path()).unwrap();
        (tmp, store)
    }

    /// A legacy by_status corpus with supersession, provenance, and review_by.
    fn legacy_by_status() -> tempfile::TempDir {
        let src = tempfile::tempdir().unwrap();
        for d in ["proposed", "accepted", "superseded"] {
            std::fs::create_dir_all(src.path().join(d)).unwrap();
        }
        std::fs::write(
            src.path().join("accepted/0006-adopt-adrs.md"),
            "# ADR-0006: Adopt ADRs\n\n> State: Accepted\n\n## Status\n\nAccepted\nCreated: 2026-04-10\n\nSupersedes [ADR-0002](../superseded/0002-old.md)\n\n## Context and Problem Statement\n\nWe need a record.\n",
        )
        .unwrap();
        std::fs::write(
            src.path().join("superseded/0002-old.md"),
            "# ADR-0002: Old Way\n\n## Status\n\nSuperseded by [ADR-0006](../accepted/0006-adopt-adrs.md)\n\n## Context\n\nOld prose.\n",
        )
        .unwrap();
        std::fs::write(
            src.path().join("proposed/0009-use-redis.md"),
            "# ADR-0009: Use Redis\n\n## Status\n\nProposed\n\nReview by: 2026-07-15\n\n## Context\n\nCache.\n",
        )
        .unwrap();
        // Noise that must be skipped.
        std::fs::write(src.path().join("README.md"), "# not an ADR\n").unwrap();
        src
    }

    #[test]
    fn seed_maps_a_by_status_corpus_into_kb_pages() {
        let (_tmp, store) = space();
        let src = legacy_by_status();
        let report = seed(&store, src.path(), true).unwrap();
        assert!(report.applied);
        assert_eq!(report.pages.len(), 3);
        // Sorted by number, references preserved.
        let refs: Vec<&str> = report.pages.iter().map(|p| p.reference.as_str()).collect();
        assert_eq!(refs, ["ADR-0002", "ADR-0006", "ADR-0009"]);

        // The superseded page: status from the directory, supersession ref
        // and stripped body in frontmatter form.
        let old = store
            .read(
                &store
                    .find_path_by_number(crate::adr::Number::new(2))
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(old.status, Status::Superseded);
        assert_eq!(old.superseded_by, Some(AdrRef::Number(6)));
        assert!(old.body.starts_with("## Context"));
        assert!(!old.body.contains("## Status"));

        // Legacy `Created:` provenance lands in `created` (midnight UTC).
        let adopted = store
            .read(
                &store
                    .find_path_by_number(crate::adr::Number::new(6))
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            adopted.created.get().date(),
            time::macros::date!(2026 - 04 - 10)
        );
        assert_eq!(adopted.supersedes, Some(AdrRef::Number(2)));

        // review_by survives.
        let redis = store
            .read(
                &store
                    .find_path_by_number(crate::adr::Number::new(9))
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(redis.review_by, Some("2026-07-15".parse().unwrap()));
    }

    #[test]
    fn seed_tolerates_a_flat_legacy_dir_reading_section_status() {
        let (_tmp, store) = space();
        let src = tempfile::tempdir().unwrap();
        std::fs::write(
            src.path().join("0001-flat.md"),
            "# ADR-0001: Flat One\n\n## Status\n\nAccepted\n\n## Context\n\nx.\n",
        )
        .unwrap();
        let report = seed(&store, src.path(), true).unwrap();
        assert_eq!(report.pages.len(), 1);
        assert_eq!(report.pages[0].status, Status::Accepted);
    }

    #[test]
    fn seed_dry_run_writes_nothing() {
        let (_tmp, store) = space();
        let src = legacy_by_status();
        let report = seed(&store, src.path(), false).unwrap();
        assert!(!report.applied);
        assert_eq!(report.pages.len(), 3);
        assert!(store.list_files().unwrap().is_empty());
    }

    #[test]
    fn seed_refuses_a_non_empty_target() {
        let (_tmp, store) = space();
        store
            .write(&mut crate::adr::Adr::new("Existing").unwrap())
            .unwrap();
        let src = legacy_by_status();
        assert!(matches!(
            seed(&store, src.path(), true),
            Err(SeedError::TargetNotEmpty { count: 1 })
        ));
    }

    #[test]
    fn seed_errors_on_an_unnumbered_or_unparseable_document() {
        let (_tmp, store) = space();
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("0001-no-h1.md"), "just prose\n").unwrap();
        assert!(matches!(
            seed(&store, src.path(), true),
            Err(SeedError::Parse { .. })
        ));
        std::fs::write(
            src.path().join("0001-no-h1.md"),
            "# No Number Here\n\n## Status\n\nAccepted\n",
        )
        .unwrap();
        assert!(matches!(
            seed(&store, src.path(), true),
            Err(SeedError::MissingNumber(_))
        ));
    }

    #[test]
    fn seed_refuses_duplicate_legacy_numbers() {
        let (_tmp, store) = space();
        let src = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join("proposed")).unwrap();
        std::fs::write(
            src.path().join("0004-a.md"),
            "# ADR-0004: A\n\n## Status\n\nAccepted\n",
        )
        .unwrap();
        std::fs::write(
            src.path().join("proposed/0004-b.md"),
            "# ADR-0004: B\n\n## Status\n\nProposed\n",
        )
        .unwrap();
        assert!(matches!(
            seed(&store, src.path(), true),
            Err(SeedError::DuplicateNumber { number: 4, .. })
        ));
    }

    #[test]
    fn seed_errors_on_an_empty_source() {
        let (_tmp, store) = space();
        let src = tempfile::tempdir().unwrap();
        assert!(matches!(
            seed(&store, src.path(), true),
            Err(SeedError::Empty(_))
        ));
        assert!(matches!(
            seed(&store, &src.path().join("nope"), true),
            Err(SeedError::SourceNotFound(_))
        ));
    }
}

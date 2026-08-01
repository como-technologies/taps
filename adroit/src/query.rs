//! The shared **read API** over [`Store`]. Builds the serde view types in
//! [`crate::view`] from parsed ADRs, so every surface (CLI, future TUI, future
//! web) derives list/search/stats/graph **once**, identically.
//!
//! This layer never writes; write logic stays in the `Store` write path used by
//! the CLI (and, later, the TUI). It reuses existing `Store` methods
//! (`list`, `read`, `find_path_by_number`, …) and does no file I/O of its own.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};

use crate::adr::{Adr, Number, Status};
use crate::config::DateSource;
use crate::history;
use crate::naming::{AdrRef, NamingScheme};
use crate::store::{Store, StoreError};
use crate::view::{
    AdrDetail, AdrSummary, CheckReport, CreatedBucket, EdgeKind, Graph, GraphEdge, GraphNode,
    Problem, ProblemFile, ProblemKind, ProposedAge, RelatedLink, Severity, Stats, StatusCount,
};

/// Errors from the query layer.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error(transparent)]
    Store(#[from] StoreError),

    #[error("could not read {0}")]
    Io(String),
}

/// How to filter and sort a list of [`AdrSummary`].
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Only include ADRs with this status. `None` means all statuses.
    pub status: Option<Status>,
    /// Sort order applied to the result.
    pub sort: Sort,
}

/// Sort order for [`summaries`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Ascending by ADR number (the on-disk listing order). The default.
    #[default]
    NumberAsc,
    /// Descending by ADR number (newest first).
    NumberDesc,
    /// Newest creation date first.
    CreatedDesc,
    /// Alphabetical by title (case-insensitive).
    TitleAsc,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Today's date (local, falling back to UTC), used to evaluate review deadlines.
/// An `ADROIT_TODAY` override (ISO `YYYY-MM-DD`) pins it for tests / CI.
fn today() -> Date {
    if let Some(d) = crate::config::today_override() {
        return d;
    }
    OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .date()
}

/// List ADR summaries, filtered and sorted per `filter`.
pub fn summaries(store: &Store, filter: &Filter) -> Result<Vec<AdrSummary>, QueryError> {
    let resolved = load_resolved(store)?;
    let today = today();
    let overdue = store.options().review_overdue_days;
    let scheme = store.options().naming;
    let mut rows: Vec<AdrSummary> = resolved
        .iter()
        .filter(|r| filter.status.is_none_or(|s| r.adr.status == s))
        .map(|r| summary_of(&r.adr, r.created, today, overdue, scheme))
        .collect();
    sort_summaries(&mut rows, filter.sort);
    Ok(rows)
}

/// Full detail for a single ADR by number.
pub fn detail(store: &Store, number: u32) -> Result<AdrDetail, QueryError> {
    let path = store.find_path_by_number(Number::new(number))?;
    detail_at(store, &path)
}

/// Full detail for a single ADR at a known path — the scheme-agnostic core, so
/// the CLI can resolve a slug/uuid ADR via the naming seam and still get detail.
pub fn detail_at(store: &Store, path: &Path) -> Result<AdrDetail, QueryError> {
    let path = path.to_path_buf();
    let adr = store.read(&path)?;
    let repo = open_history(store);
    let hist = repo.as_ref().and_then(|r| r.history(&path));
    let (created, last_modified) = resolve_dates(&adr, &path, hist);
    let summary = summary_of(
        &adr,
        created,
        today(),
        store.options().review_overdue_days,
        store.options().naming,
    );
    let related = related_links(&adr, store.options().naming);
    // The stored implementation plan, when the document carries one (ADR-0008).
    let plan = crate::plan::extract(&adr.body).map(str::to_string);
    Ok(AdrDetail {
        summary,
        body: adr.body,
        // TODO(step4): render markdown -> HTML server-side for the web surface.
        body_html: None,
        plan,
        related,
        last_modified: last_modified.and_then(|d| d.format(&Rfc3339).ok()),
    })
}

/// Case-insensitive search over title + body. Returns matching summaries in
/// the default (number-ascending) order.
pub fn search(store: &Store, term: &str) -> Result<Vec<AdrSummary>, QueryError> {
    let needle = term.to_lowercase();
    let resolved = load_resolved(store)?;
    let today = today();
    let overdue = store.options().review_overdue_days;
    let scheme = store.options().naming;
    let rows = resolved
        .iter()
        .filter(|r| {
            let haystack = format!("{} {}", r.adr.title, r.adr.body).to_lowercase();
            haystack.contains(&needle)
        })
        .map(|r| summary_of(&r.adr, r.created, today, overdue, scheme))
        .collect();
    Ok(rows)
}

/// Aggregate statistics across all ADRs.
pub fn stats(store: &Store) -> Result<Stats, QueryError> {
    let resolved = load_resolved(store)?;
    let now = OffsetDateTime::now_utc();
    let today = today();

    // Counts per status, every status present (including zeroes) in order.
    let by_status: Vec<StatusCount> = Status::ALL
        .into_iter()
        .map(|status| StatusCount {
            status,
            count: resolved.iter().filter(|r| r.adr.status == status).count(),
        })
        .collect();

    // Age of each still-Proposed ADR (from its git-derived creation), oldest first.
    let scheme = store.options().naming;
    let overdue = store.options().review_overdue_days;
    let mut proposed_age: Vec<ProposedAge> = resolved
        .iter()
        .filter(|r| r.adr.status == Status::Proposed)
        .map(|r| {
            let rf = r.adr.reference();
            ProposedAge {
                number: r.adr.number.map(Number::get),
                reference: scheme.display(&rf),
                address: rf.addr(),
                title: r.adr.title.clone(),
                age_days: Some((now - r.created).whole_days()),
                review_due: summary_of(&r.adr, r.created, today, overdue, scheme).review_due,
            }
        })
        .collect();
    proposed_age.sort_by_key(|p| std::cmp::Reverse(p.age_days));

    // Created-over-time, bucketed by calendar month (YYYY-MM), oldest first.
    let mut months: BTreeMap<String, usize> = BTreeMap::new();
    for r in &resolved {
        let d = r.created;
        let key = format!("{:04}-{:02}", d.year(), u8::from(d.month()));
        *months.entry(key).or_default() += 1;
    }
    let created_over_time: Vec<CreatedBucket> = months
        .into_iter()
        .map(|(month, count)| CreatedBucket { month, count })
        .collect();

    // ADRs flagged review-due: still Proposed and past their `review_by` date,
    // or aged past the configured staleness threshold.
    let review_due: Vec<AdrSummary> = resolved
        .iter()
        .map(|r| summary_of(&r.adr, r.created, today, overdue, scheme))
        .filter(|s| s.review_due)
        .collect();

    Ok(Stats {
        total: resolved.len(),
        by_status,
        proposed_age,
        review_due,
        created_over_time,
    })
}

/// The duplicate-detection / existence key for an ADR identity, so [`check`]
/// groups and looks ADRs up uniformly across naming schemes.
fn ident_key(r: &AdrRef) -> String {
    match r {
        AdrRef::Number(n) => format!("n:{n}"),
        AdrRef::Slug(s) => format!("s:{s}"),
    }
}

/// Line and byte counts for a file, for the duplicate-check size hints. Returns
/// `(0, metadata_len_or_0)` for a file that can't be read as UTF-8 text.
fn file_stats(path: &Path) -> (usize, u64) {
    match std::fs::read_to_string(path) {
        Ok(s) => (s.lines().count(), s.len() as u64),
        Err(_) => (0, std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)),
    }
}

/// Validate the ADR repo, returning a structured [`CheckReport`].
///
/// The shared engine behind `adroit check` and the web dashboard's repo-health
/// panel:
///
/// 1. Duplicate ADR identifiers (scheme-aware).
/// 2. Unparseable ADR pages.
/// 3. Broken supersession refs (referenced ADR doesn't exist).
/// 4. Broken / stale cross-ADR relative links.
/// 5. Duplicate titles (advisory).
///
/// Problems are returned sorted by severity (errors first) then message; the
/// CLI renders `problem.message` verbatim, so its output is unchanged.
pub fn check(store: &Store) -> Result<CheckReport, QueryError> {
    let files = store.list_files()?;
    let scheme = store.options().naming;
    let mut problems: Vec<Problem> = Vec::new();

    // First read-loop: flag unparseable files, group identities/titles, and
    // collect supersession refs.
    let corpus = collect_corpus(store, &files, &mut problems);

    // The remaining checks all read over the grouped corpus; order is irrelevant
    // because problems are sorted by (severity, message) before returning.
    check_broken_supersession(scheme, &corpus, &mut problems);
    check_links(store, &files, scheme, &corpus.by_ident, &mut problems);
    check_duplicate_ids(store, scheme, &corpus.by_ident, &mut problems);
    check_duplicate_titles(store, &corpus.by_title, &mut problems);

    problems.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.message.cmp(&b.message))
    });
    Ok(CheckReport {
        checked: files.len(),
        problems,
    })
}

/// A path relative to the store root (or the path itself if it isn't under it),
/// as a display string — the form every `check` problem message uses.
fn rel_of(store: &Store, path: &Path) -> String {
    path.strip_prefix(store.root())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Build the `ProblemFile` list (with size hints) and the comma-joined display
/// of their paths, shared by the duplicate-id and duplicate-title problems.
fn problem_files(store: &Store, paths: &[std::path::PathBuf]) -> (Vec<ProblemFile>, String) {
    let files: Vec<ProblemFile> = paths
        .iter()
        .map(|p| {
            let path = rel_of(store, p);
            let (lines, bytes) = file_stats(p);
            ProblemFile { path, lines, bytes }
        })
        .collect();
    let list = files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    (files, list)
}

/// The groupings the first read-loop produces, shared by the later checks.
struct Corpus {
    /// Scheme identity → the files carrying it (duplicate / link resolution).
    by_ident: BTreeMap<String, Vec<std::path::PathBuf>>,
    /// Normalized title → (original-case title, files sharing it).
    by_title: BTreeMap<String, (String, Vec<std::path::PathBuf>)>,
    /// Frontmatter supersession refs (rel path, supersedes, superseded_by).
    fm_supersession: Vec<(String, Option<AdrRef>, Option<AdrRef>)>,
}

/// First read-loop: flag `(2)` Unparseable files, group files into `by_ident`
/// and `by_title`, and collect `(3)` supersession refs.
fn collect_corpus(
    store: &Store,
    files: &[std::path::PathBuf],
    problems: &mut Vec<Problem>,
) -> Corpus {
    // Group paths by the scheme's identity (to flag duplicates, and to resolve
    // cross-ADR links / supersession refs — works for every naming scheme, not
    // just the numeric ones).
    let mut by_ident: BTreeMap<String, Vec<std::path::PathBuf>> = BTreeMap::new();
    // Group by normalized title for the duplicate-title check (value: the
    // original-case title + the files that share it).
    let mut by_title: BTreeMap<String, (String, Vec<std::path::PathBuf>)> = BTreeMap::new();
    // Frontmatter supersession refs (YAML fields, not markdown links), collected
    // for a broken-supersession check mirroring the markdown one below.
    let mut fm_supersession: Vec<(String, Option<AdrRef>, Option<AdrRef>)> = Vec::new();

    for path in files {
        let rel = rel_of(store, path);

        // (2) Unparseable page.
        let adr = match store.read(path) {
            Ok(adr) => adr,
            Err(e) => {
                problems.push(Problem {
                    severity: Severity::Error,
                    kind: ProblemKind::Unparseable,
                    label: rel.clone(),
                    summary: format!("failed to parse ({e})"),
                    paths: Vec::new(),
                    message: format!("{rel}: failed to parse ({e})"),
                });
                continue;
            }
        };
        // Group by the scheme's identity for duplicate detection. A numeric ADR
        // with no number, or a file with no parseable identity, is skipped so
        // stray notes don't register as collisions.
        let r = adr.reference();
        let track = matches!(r, AdrRef::Slug(_)) || adr.number.is_some();
        if track {
            by_ident
                .entry(ident_key(&r))
                .or_default()
                .push(path.clone());
        }
        let norm_title = adr.title.trim().to_lowercase();
        if !norm_title.is_empty() {
            by_title
                .entry(norm_title)
                .or_insert_with(|| (adr.title.trim().to_string(), Vec::new()))
                .1
                .push(path.clone());
        }
        fm_supersession.push((
            rel.clone(),
            adr.supersedes.clone(),
            adr.superseded_by.clone(),
        ));
    }

    Corpus {
        by_ident,
        by_title,
        fm_supersession,
    }
}

/// Check `(3)`: the frontmatter supersession fields (`supersedes:` /
/// `superseded_by:`) against the identity set — a referenced ADR that doesn't
/// exist is a broken supersession.
fn check_broken_supersession(scheme: NamingScheme, corpus: &Corpus, problems: &mut Vec<Problem>) {
    let by_ident = &corpus.by_ident;
    for (rel, supersedes, superseded_by) in &corpus.fm_supersession {
        for (kind, r) in [("Supersedes", supersedes), ("Superseded by", superseded_by)] {
            if let Some(r) = r
                && !by_ident.contains_key(&ident_key(r))
            {
                let disp = scheme.display(r);
                problems.push(Problem {
                    severity: Severity::Error,
                    kind: ProblemKind::BrokenSupersession,
                    label: rel.clone(),
                    summary: format!("frontmatter says {kind} {disp} but no such ADR exists"),
                    paths: Vec::new(),
                    message: format!(
                        "{rel}: frontmatter says {kind} {disp} but no such ADR exists"
                    ),
                });
            }
        }
    }
}

/// Phase `(4)`: cross-ADR relative links — each must resolve to an existing
/// file, and a link should point at where the ADR it names currently lives.
/// **Scheme-aware** (mirrors `relink`): the link target is resolved to an
/// `AdrRef` and looked up in the identity set, so date/uuid links classify
/// *stale* (moved → warning) vs *broken* (no ADR → error) correctly.
fn check_links(
    store: &Store,
    files: &[std::path::PathBuf],
    scheme: NamingScheme,
    by_ident: &BTreeMap<String, Vec<std::path::PathBuf>>,
    problems: &mut Vec<Problem>,
) {
    for path in files {
        let rel = rel_of(store, path);
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        // The two stale-link problems (missing-target arm and moved-target arm)
        // are byte-identical, so build both from one closure.
        let stale = |target: &str, disp: &str, want: &str| Problem {
            severity: Severity::Warning,
            kind: ProblemKind::StaleLink,
            label: rel.clone(),
            summary: format!(
                "stale link [{target}] — {disp} is now [{want}] (run `adroit relink`)"
            ),
            paths: Vec::new(),
            message: format!(
                "{rel}: stale link [{target}] — {disp} is now [{want}] (run `adroit relink`)"
            ),
        };
        for target in crate::links::relative_md_targets(&content) {
            let pathpart = target.split('#').next().unwrap_or(target);
            let resolved = dir.join(pathpart);
            // The ADR this link names, and where it currently lives (unambiguously).
            let link_ref = scheme.ref_in_link(target);
            let canon: Option<&std::path::PathBuf> = link_ref
                .as_ref()
                .and_then(|r| by_ident.get(&ident_key(r)))
                .filter(|paths| paths.len() == 1)
                .map(|paths| &paths[0]);
            // Message label: keep `ADR-N` (un-padded) for numeric schemes so the
            // output stays byte-identical; the scheme display otherwise.
            let disp = link_ref.as_ref().map(|r| match r.as_number() {
                Some(n) => format!("ADR-{n}"),
                None => scheme.display(r),
            });

            if !resolved.exists() {
                // The literal target is missing. If the link names an ADR that
                // still exists elsewhere in the repo, it's a STALE link a
                // `relink` will heal (a warning — so a deferred-relink PR branch,
                // whose inbound links haven't been canonicalized yet, still
                // passes `check`). A link that names no existing ADR is truly
                // BROKEN (an error).
                if let (Some(disp), Some(canon)) = (&disp, canon) {
                    let want = crate::links::rel_link(dir, canon);
                    problems.push(stale(target, disp, &want));
                } else if link_ref.is_some() {
                    problems.push(Problem {
                        severity: Severity::Error,
                        kind: ProblemKind::BrokenLink,
                        label: rel.clone(),
                        summary: format!("broken link [{target}] — target file not found"),
                        paths: Vec::new(),
                        message: format!("{rel}: broken link [{target}] — target file not found"),
                    });
                } else {
                    // Not an ADR link at all (a book page, an asset, …):
                    // outside the corpus contract, so advisory only. In a
                    // seeded ephemeral space (ADR-0020) such targets can
                    // never resolve — validating them is the owning repo's
                    // job, not adroit's.
                    problems.push(Problem {
                        severity: Severity::Warning,
                        kind: ProblemKind::ExternalLink,
                        label: rel.clone(),
                        summary: format!(
                            "external link [{target}] does not resolve here — outside the corpus, not validated"
                        ),
                        paths: Vec::new(),
                        message: format!(
                            "{rel}: external link [{target}] does not resolve here — outside the corpus, not validated"
                        ),
                    });
                }
                continue;
            }
            // Resolved file exists: stale only if it isn't the ADR's current home.
            if let (Some(disp), Some(canon)) = (&disp, canon)
                && let (Ok(rp), Ok(cp)) = (
                    std::fs::canonicalize(&resolved),
                    std::fs::canonicalize(canon),
                )
                && rp != cp
            {
                let want = crate::links::rel_link(dir, canon);
                problems.push(stale(target, disp, &want));
            }
        }
    }
}

/// Phase `(1)`: duplicate identifiers (scheme-aware). The wording stays "number"
/// for numeric schemes (byte-identical message) and "identifier" otherwise.
fn check_duplicate_ids(
    store: &Store,
    scheme: NamingScheme,
    by_ident: &BTreeMap<String, Vec<std::path::PathBuf>>,
    problems: &mut Vec<Problem>,
) {
    let noun = if scheme.is_numeric() {
        "number"
    } else {
        "identifier"
    };
    for (key, paths) in by_ident {
        if paths.len() > 1 {
            // Numeric identity → `ADR-NNNN` (from the key, so the message is
            // byte-identical); slug identity → the scheme's display string.
            let disp = if let Some(num) = key.strip_prefix("n:") {
                format!("ADR-{:04}", num.parse::<u32>().unwrap_or(0))
            } else {
                scheme
                    .parse(&paths[0])
                    .map(|r| scheme.display(&r))
                    .unwrap_or_else(|| key.trim_start_matches("s:").to_string())
            };
            let (files, list) = problem_files(store, paths);
            let message = format!("{disp}: duplicate {noun} used by {list}");
            problems.push(Problem {
                severity: Severity::Error,
                kind: ProblemKind::DuplicateId,
                label: disp,
                summary: format!("duplicate {noun}"),
                paths: files,
                message,
            });
        }
    }
}

/// Phase `(5)`: duplicate titles (advisory). Titles may legitimately repeat, so
/// this is a Warning — `check` still exits 0 — but it surfaces the accidental
/// `new`.
fn check_duplicate_titles(
    store: &Store,
    by_title: &BTreeMap<String, (String, Vec<std::path::PathBuf>)>,
    problems: &mut Vec<Problem>,
) {
    for (title, paths) in by_title.values() {
        if paths.len() > 1 {
            let (files, list) = problem_files(store, paths);
            problems.push(Problem {
                severity: Severity::Warning,
                kind: ProblemKind::DuplicateTitle,
                label: title.clone(),
                summary: "duplicate title".to_string(),
                paths: files,
                message: format!("duplicate title \"{title}\" used by {list}"),
            });
        }
    }
}

/// The supersession / relationship graph across all ADRs.
///
/// Nodes are every ADR. Edges are derived from `supersedes` / `superseded_by`
/// fields and from markdown links to other ADRs found in each body.
pub fn graph(store: &Store) -> Result<Graph, QueryError> {
    let adrs = store.list()?;
    let scheme = store.options().naming;

    let nodes: Vec<GraphNode> = adrs
        .iter()
        .map(|a| {
            let r = a.reference();
            let addressable = a.number.is_some() || a.slug.is_some();
            GraphNode {
                reference: scheme.display(&r),
                address: addressable.then(|| r.addr()),
                title: a.title.clone(),
                status: a.status,
            }
        })
        .collect();

    let mut edges: Vec<GraphEdge> = Vec::new();
    for a in &adrs {
        let from = scheme.display(&a.reference());
        // Supersession from explicit fields. `from supersedes to`.
        if let Some(r) = &a.supersedes {
            push_unique(
                &mut edges,
                from.clone(),
                scheme.display(r),
                EdgeKind::Supersedes,
            );
        }
        // `superseded_by` means the *other* ADR supersedes this one.
        if let Some(r) = &a.superseded_by {
            push_unique(
                &mut edges,
                scheme.display(r),
                from.clone(),
                EdgeKind::Supersedes,
            );
        }
        // Typed relational links (frontmatter): one directed edge per entry.
        for (targets, kind) in typed_links(a) {
            for r in targets {
                push_unique(&mut edges, from.clone(), scheme.display(r), kind);
            }
        }
        // Markdown links to other ADRs in the body become `Related` edges,
        // unless that pair already has a more specific edge (supersession or a
        // typed link).
        for r in linked_refs(&a.body, scheme) {
            let to = scheme.display(&r);
            if to == from {
                continue;
            }
            if edges
                .iter()
                .any(|e| e.kind != EdgeKind::Related && pair_matches(e, &from, &to))
            {
                continue;
            }
            push_unique(&mut edges, from.clone(), to, EdgeKind::Related);
        }
    }

    Ok(Graph { nodes, edges })
}

/// The typed relational links of an ADR, paired with their edge kind.
fn typed_links(a: &Adr) -> [(&[crate::naming::AdrRef], EdgeKind); 3] {
    [
        (&a.depends_on, EdgeKind::DependsOn),
        (&a.refines, EdgeKind::Refines),
        (&a.relates_to, EdgeKind::RelatesTo),
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// An ADR paired with its resolved creation date, so every list/stats path
/// reports the same git-derived date. (The full lifecycle/last-modified is only
/// needed by [`detail`], which resolves a single ADR directly.)
struct Resolved {
    adr: Adr,
    /// Best-available creation timestamp (git first-add, else fallback).
    created: OffsetDateTime,
}

/// Warn at most once per process when strict `date_source = git` can't deliver.
static GIT_STRICT_WARNED: AtomicBool = AtomicBool::new(false);

/// Open the git repo for date resolution, honoring the configured
/// [`DateSource`]: `Filesystem` never shells git; `Auto` uses git when present
/// (silent fallback); `Git` is strict — it warns once (then still falls back)
/// when git history is unavailable or the clone is shallow, so a CI
/// misconfiguration is visible rather than silently producing wrong dates.
fn open_history(store: &Store) -> Option<history::GitRepo> {
    let source = store.options().date_source;
    if source == DateSource::Filesystem {
        return None;
    }
    let repo = history::open(store.root());
    if source == DateSource::Git {
        let warning = match &repo {
            None => Some(
                "date_source=git but this isn't a git work tree (or git isn't \
                 installed) — falling back to filesystem dates",
            ),
            Some(r) if r.is_shallow() => Some(
                "date_source=git on a shallow clone — ADR creation dates may be \
                 wrong; fetch full history (e.g. actions/checkout fetch-depth: 0)",
            ),
            Some(_) => None,
        };
        if let Some(msg) = warning
            && !GIT_STRICT_WARNED.swap(true, Ordering::Relaxed)
        {
            eprintln!("adroit: {msg}");
        }
    }
    repo
}

/// Load every ADR and resolve its creation date from git (once per call).
///
/// The git repository is probed a single time; each file's history is then one
/// `git log`. Outside a git repo the per-file lookup returns `None` and the date
/// falls back (see [`resolve_dates`]).
fn load_resolved(store: &Store) -> Result<Vec<Resolved>, QueryError> {
    let repo = open_history(store);
    let resolved = store
        .list_with_paths()?
        .into_iter()
        .map(|(path, adr)| {
            let hist = repo.as_ref().and_then(|r| r.history(&path));
            let (created, _) = resolve_dates(&adr, &path, hist);
            Resolved { adr, created }
        })
        .collect();
    Ok(resolved)
}

/// Resolve an ADR's creation and last-modified dates from its git history when
/// available, else from non-git sources.
///
/// Precedence for `created`: 1) git first-add date (the real history — a clone
/// resets mtime, so git wins where present); 2) the page's authored `created:`
/// frontmatter field, which is rewrite-stable provenance on a non-git corpus.
fn resolve_dates(
    adr: &Adr,
    path: &Path,
    hist: Option<history::AdrHistory>,
) -> (OffsetDateTime, Option<OffsetDateTime>) {
    match hist {
        Some(h) => (h.created, Some(h.last_modified)),
        None => (adr.created.get(), file_mtime(path)),
    }
}

/// Filesystem modification time of `path`, if readable.
fn file_mtime(path: &Path) -> Option<OffsetDateTime> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .map(OffsetDateTime::from)
}

/// Build an [`AdrSummary`] from a parsed [`Adr`] and its resolved creation
/// date, evaluating review-due against `today`.
///
/// An ADR is **review-due** when it is still `Proposed` and either: it has a
/// `review_by` deadline on or before `today`; or `overdue_days` is set and the
/// ADR has been sitting (since `created`) at least that many days — so an aging
/// backlog surfaces without anyone stamping each ADR with a deadline.
fn summary_of(
    adr: &Adr,
    created: OffsetDateTime,
    today: Date,
    overdue_days: Option<u32>,
    scheme: NamingScheme,
) -> AdrSummary {
    let proposed = adr.status == Status::Proposed;
    let past_deadline = adr.review_by.is_some_and(|rb| rb.get() <= today);
    let stale =
        overdue_days.is_some_and(|days| (today - created.date()).whole_days() >= i64::from(days));
    let review_due = proposed && (past_deadline || stale);
    AdrSummary {
        number: adr.number.map(Number::get),
        number_display: adr
            .number
            .map(|n| n.to_string())
            .unwrap_or_else(|| "????".to_string()),
        reference: scheme.display(&adr.reference()),
        address: adr.reference().addr(),
        title: adr.title.clone(),
        status: adr.status,
        created: created.format(&Rfc3339).ok(),
        supersedes: adr
            .supersedes
            .as_ref()
            .map(|r| scheme.display(r))
            .into_iter()
            .collect(),
        superseded_by: adr.superseded_by.as_ref().map(|r| scheme.display(r)),
        review_due,
        forge_data: None,
    }
}

/// Resolve related links for the detail view from fields + body links.
fn related_links(adr: &Adr, scheme: NamingScheme) -> Vec<RelatedLink> {
    let mut out: Vec<RelatedLink> = Vec::new();
    if let Some(r) = &adr.supersedes {
        push_related(&mut out, scheme, r, EdgeKind::Supersedes);
    }
    if let Some(r) = &adr.superseded_by {
        push_related(&mut out, scheme, r, EdgeKind::Supersedes);
    }
    // Typed relational links (frontmatter).
    for (targets, kind) in typed_links(adr) {
        for r in targets {
            push_related(&mut out, scheme, r, kind);
        }
    }
    let self_ref = adr.reference();
    for r in linked_refs(&adr.body, scheme) {
        if r == self_ref {
            continue;
        }
        let address = r.addr();
        // A plain body link is the weakest edge; skip if a more specific one
        // (supersession or a typed link) already covers this target.
        if out
            .iter()
            .any(|x| x.address == address && x.kind != EdgeKind::Related)
        {
            continue;
        }
        push_related(&mut out, scheme, &r, EdgeKind::Related);
    }
    out
}

/// Push a [`RelatedLink`], skipping exact duplicates (by addressing token).
fn push_related(
    out: &mut Vec<RelatedLink>,
    scheme: NamingScheme,
    r: &crate::naming::AdrRef,
    kind: EdgeKind,
) {
    let address = r.addr();
    if !out.iter().any(|x| x.address == address && x.kind == kind) {
        out.push(RelatedLink {
            reference: scheme.display(r),
            address,
            kind,
        });
    }
}

fn sort_summaries(rows: &mut [AdrSummary], sort: Sort) {
    match sort {
        Sort::NumberAsc => rows.sort_by_key(|a| a.number),
        Sort::NumberDesc => rows.sort_by_key(|a| std::cmp::Reverse(a.number)),
        // `created` is `Option<String>` (not `Copy`); the comparator reverse
        // avoids cloning the key per element.
        Sort::CreatedDesc => rows.sort_by(|a, b| b.created.cmp(&a.created)),
        Sort::TitleAsc => rows.sort_by_key(|a| a.title.to_lowercase()),
    }
}

fn push_unique(edges: &mut Vec<GraphEdge>, from: String, to: String, kind: EdgeKind) {
    if !edges
        .iter()
        .any(|e| e.from == from && e.to == to && e.kind == kind)
    {
        edges.push(GraphEdge { from, to, kind });
    }
}

fn pair_matches(e: &GraphEdge, a: &str, b: &str) -> bool {
    (e.from == a && e.to == b) || (e.from == b && e.to == a)
}

/// Extract the ADR references targeted by markdown links in `body`, resolved
/// through the naming `scheme` (e.g. `[ADR-0006](../accepted/0006-foo.md)` →
/// `Number(6)`, or `[x](20260601-foo.md)` → `Slug(..)`).
fn linked_refs(body: &str, scheme: NamingScheme) -> Vec<crate::naming::AdrRef> {
    // Reuse the one markdown `](target)` scanner (`links::for_each_link`) rather
    // than a second copy; resolve each target to an ADR ref via the naming seam.
    let mut out = Vec::new();
    crate::links::for_each_link(body, |target, _, _| {
        if let Some(r) = scheme.ref_in_link(target)
            && !out.contains(&r)
        {
            out.push(r);
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::{Adr, Status};
    use crate::store::{Store, StoreOptions};
    use std::path::PathBuf;

    /// A fresh KB space with a store over it (filesystem dates — no git, so
    /// the authored `created:` is authoritative and tests stay hermetic).
    fn space() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("wiki.toml"), "name = \"test\"\n").unwrap();
        let store = Store::open_or_create_with(
            tmp.path(),
            StoreOptions {
                date_source: crate::config::DateSource::Filesystem,
                ..StoreOptions::default()
            },
        )
        .unwrap();
        (tmp, store)
    }

    /// Write a decision page through the store; returns its path.
    fn page(store: &Store, status: Status, title: &str, body: &str) -> PathBuf {
        let mut adr = Adr::new(title).unwrap();
        adr.status = status;
        adr.body = body.to_string();
        store.write(&mut adr).unwrap()
    }

    fn seed(store: &Store) {
        page(
            store,
            Status::Accepted,
            "Use Postgres",
            "## Context\n\nWe need a database.",
        );
        page(
            store,
            Status::Proposed,
            "Use Redis",
            "## Context\n\nWe need a cache for sessions.",
        );
        page(
            store,
            Status::Proposed,
            "Adopt GraphQL",
            "## Context\n\nSee [ADR-0001](./0001-use-postgres.md).",
        );
    }

    #[test]
    fn check_clean_repo_has_no_problems() {
        let (_tmp, store) = space();
        seed(&store);
        let report = check(&store).unwrap();
        assert_eq!(report.checked, 3);
        assert!(report.problems.is_empty());
    }

    #[test]
    fn check_flags_duplicate_number_as_error() {
        let (_tmp, store) = space();
        let p1 = page(&store, Status::Proposed, "Alpha", "## Context\n\nx.");
        // A second file carrying the same leading number is a duplicate id.
        std::fs::copy(&p1, p1.with_file_name("0001-beta.md")).unwrap();
        let report = check(&store).unwrap();
        let dups: Vec<_> = report
            .problems
            .iter()
            .filter(|p| p.kind == ProblemKind::DuplicateId)
            .collect();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].severity, Severity::Error);
        assert_eq!(dups[0].label, "ADR-0001");
        assert_eq!(dups[0].summary, "duplicate number");
        assert_eq!(dups[0].paths.len(), 2);
        // Size hints are populated so the UI can flag a stub vs. a full ADR.
        assert!(dups[0].paths.iter().all(|f| f.lines > 0 && f.bytes > 0));
        assert!(dups[0].message.contains("duplicate number used by"));
    }

    #[test]
    fn check_flags_broken_supersession_ref() {
        let (_tmp, store) = space();
        let p = page(&store, Status::Superseded, "Old", "## Context\n\nx.");
        let mut adr = store.read(&p).unwrap();
        adr.superseded_by = Some(AdrRef::Number(99));
        std::fs::write(&p, crate::frontmatter::serialize(&adr).unwrap()).unwrap();

        let report = check(&store).unwrap();
        let broken: Vec<_> = report
            .problems
            .iter()
            .filter(|p| p.kind == ProblemKind::BrokenSupersession)
            .collect();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].severity, Severity::Error);
        assert!(
            broken[0].message.contains("ADR-0099"),
            "{}",
            broken[0].message
        );
    }

    #[test]
    fn check_classifies_stale_vs_broken_links() {
        let (_tmp, store) = space();
        seed(&store);
        // A link naming an existing ADR at the wrong path is STALE (warning);
        // one naming no ADR is BROKEN (error).
        store
            .set_body(
                Number::new(2),
                "See [ADR-0001](../elsewhere/0001-use-postgres.md) and [gone](./0099-gone.md).",
            )
            .unwrap();
        let report = check(&store).unwrap();
        let kinds: Vec<ProblemKind> = report.problems.iter().map(|p| p.kind).collect();
        assert!(kinds.contains(&ProblemKind::StaleLink), "{kinds:?}");
        assert!(kinds.contains(&ProblemKind::BrokenLink), "{kinds:?}");
    }

    #[test]
    fn summaries_returns_all_by_default() {
        let (_tmp, store) = space();
        seed(&store);
        let rows = summaries(&store, &Filter::default()).unwrap();
        assert_eq!(rows.len(), 3);
        // Default sort: number ascending.
        assert_eq!(rows[0].number, Some(1));
        assert_eq!(rows[2].number, Some(3));
        assert_eq!(rows[0].number_display, "0001");
    }

    #[test]
    fn summaries_filters_by_status() {
        let (_tmp, store) = space();
        seed(&store);
        let filter = Filter {
            status: Some(Status::Proposed),
            ..Default::default()
        };
        let rows = summaries(&store, &filter).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.status == Status::Proposed));
    }

    #[test]
    fn summaries_sort_number_desc_and_title_asc() {
        let (_tmp, store) = space();
        seed(&store);
        let rows = summaries(
            &store,
            &Filter {
                status: None,
                sort: Sort::NumberDesc,
            },
        )
        .unwrap();
        assert_eq!(rows[0].number, Some(3));
        assert_eq!(rows[2].number, Some(1));

        let rows = summaries(
            &store,
            &Filter {
                status: None,
                sort: Sort::TitleAsc,
            },
        )
        .unwrap();
        assert_eq!(rows[0].title, "Adopt GraphQL");
        assert_eq!(rows[1].title, "Use Postgres");
        assert_eq!(rows[2].title, "Use Redis");
    }

    #[test]
    fn search_is_case_insensitive_over_title_and_body() {
        let (_tmp, store) = space();
        seed(&store);

        let by_title = search(&store, "REDIS").unwrap();
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].title, "Use Redis");

        // "database" appears only in the Postgres body.
        let by_body = search(&store, "database").unwrap();
        assert_eq!(by_body.len(), 1);
        assert_eq!(by_body[0].number, Some(1));

        assert!(search(&store, "nonexistent-term").unwrap().is_empty());
    }

    #[test]
    fn detail_includes_raw_body_and_related_links() {
        let (_tmp, store) = space();
        seed(&store);

        let d = detail(&store, 3).unwrap();
        assert_eq!(d.summary.number, Some(3));
        assert!(d.body.contains("See [ADR-0001]"));
        assert!(d.body_html.is_none());
        // ADR-0003 links to ADR-0001 -> a Related edge.
        assert_eq!(d.related.len(), 1);
        assert_eq!(d.related[0].reference, "ADR-0001");
        assert_eq!(d.related[0].address, "1");
        assert_eq!(d.related[0].kind, EdgeKind::Related);
    }

    #[test]
    fn detail_missing_number_errors() {
        let (_tmp, store) = space();
        seed(&store);
        assert!(detail(&store, 99).is_err());
    }

    #[test]
    fn stats_counts_by_status_and_total() {
        let (_tmp, store) = space();
        seed(&store);

        let s = stats(&store).unwrap();
        assert_eq!(s.total, 3);
        let count = |status: Status| {
            s.by_status
                .iter()
                .find(|c| c.status == status)
                .map(|c| c.count)
                .unwrap()
        };
        assert_eq!(count(Status::Accepted), 1);
        assert_eq!(count(Status::Proposed), 2);
        assert_eq!(count(Status::Rejected), 0);
        // Two proposed ADRs -> two age rows.
        assert_eq!(s.proposed_age.len(), 2);
        // Fresh pages aren't review-due.
        assert!(s.review_due.is_empty());
        // Every status is represented in lifecycle order.
        assert_eq!(s.by_status.len(), Status::ALL.len());
        assert_eq!(s.by_status[0].status, Status::Proposed);
    }

    #[test]
    fn graph_emits_one_supersedes_edge_from_both_directions() {
        // Both reciprocal fields round-trip; the graph collapses them into one
        // (newer -> older) edge.
        let (_tmp, store) = space();
        page(&store, Status::Superseded, "Old decision", ""); // ADR 1
        page(&store, Status::Accepted, "New decision", ""); // ADR 2
        store
            .supersede(&AdrRef::Number(2), &AdrRef::Number(1))
            .unwrap();
        let p2 = store.find_path_by_number(Number::new(2)).unwrap();
        let mut newer = store.read(&p2).unwrap();
        newer.supersedes = Some(AdrRef::Number(1));
        std::fs::write(&p2, crate::frontmatter::serialize(&newer).unwrap()).unwrap();

        let g = graph(&store).unwrap();
        assert_eq!(g.nodes.len(), 2);
        let supersedes: Vec<_> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Supersedes)
            .collect();
        assert_eq!(supersedes.len(), 1, "one logical supersession -> one edge");
        assert_eq!(supersedes[0].from, "ADR-0002");
        assert_eq!(supersedes[0].to, "ADR-0001");
    }

    #[test]
    fn stats_flags_past_due_proposed_and_excludes_accepted() {
        use crate::adr::ReviewBy;
        let (_tmp, store) = space();
        // Proposed + past-due review date -> review_due.
        page(&store, Status::Proposed, "Past due", "");
        store
            .set_review_by(
                Number::new(1),
                Some("2000-01-01".parse::<ReviewBy>().unwrap()),
            )
            .unwrap();
        // Accepted + past date -> NOT review_due (only Proposed counts).
        page(&store, Status::Accepted, "Accepted old", "");
        store
            .set_review_by(
                Number::new(2),
                Some("2000-01-01".parse::<ReviewBy>().unwrap()),
            )
            .unwrap();
        // Proposed + far-future date -> NOT review_due.
        let future = ReviewBy::new(
            time::OffsetDateTime::now_utc()
                .date()
                .saturating_add(time::Duration::days(3650)),
        );
        page(&store, Status::Proposed, "Future", "");
        store.set_review_by(Number::new(3), Some(future)).unwrap();

        let s = stats(&store).unwrap();
        assert_eq!(s.review_due.len(), 1);
        assert_eq!(s.review_due[0].number, Some(1));
    }

    #[test]
    fn graph_derives_related_edges_from_body_links() {
        let (_tmp, store) = space();
        seed(&store);
        let g = graph(&store).unwrap();
        // ADR-0003's body links to ADR-0001 -> exactly one Related edge.
        let related: Vec<_> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Related)
            .collect();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].from, "ADR-0003");
        assert_eq!(related[0].to, "ADR-0001");
    }

    #[test]
    fn graph_emits_typed_link_edges() {
        let (_tmp, store) = space();
        let mut base = Adr::new("Base").unwrap();
        store.write(&mut base).unwrap(); // ADR 1
        let mut dependent = Adr::new("Dependent").unwrap();
        dependent.depends_on = vec![AdrRef::Number(1)];
        store.write(&mut dependent).unwrap(); // ADR 2 depends_on ADR 1

        let g = graph(&store).unwrap();
        let deps: Vec<_> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::DependsOn)
            .collect();
        assert_eq!(deps.len(), 1, "one depends_on edge");
        assert_eq!(deps[0].from, "ADR-0002");
        assert_eq!(deps[0].to, "ADR-0001");
    }

    #[test]
    fn linked_refs_parses_forms() {
        assert_eq!(
            linked_refs(
                "see [x](../accepted/0006-foo.md) and [y](0012-bar.md)",
                NamingScheme::Sequential
            ),
            vec![AdrRef::Number(6), AdrRef::Number(12)]
        );
        assert_eq!(
            linked_refs("no links here", NamingScheme::Sequential),
            Vec::<AdrRef>::new()
        );
        // Date scheme resolves slug targets.
        assert_eq!(
            linked_refs("[x](../accepted/20260601-foo.md)", NamingScheme::Date),
            vec![AdrRef::Slug("20260601-foo".into())]
        );
    }

    #[test]
    fn review_due_flags_stale_proposed_even_without_a_deadline() {
        use time::{Date, Month};

        let mut adr = Adr::new("Aging proposal").unwrap();
        adr.status = Status::Proposed;
        let today = Date::from_calendar_date(2026, Month::June, 1).unwrap();
        // 40 days old, no `review_by`.
        let old = Date::from_calendar_date(2026, Month::April, 22)
            .unwrap()
            .midnight()
            .assume_utc();
        let recent = Date::from_calendar_date(2026, Month::May, 28)
            .unwrap()
            .midnight()
            .assume_utc();

        let seq = NamingScheme::Sequential;
        // Aged past the 30-day threshold -> review-due, no deadline needed.
        assert!(summary_of(&adr, old, today, Some(30), seq).review_due);
        // Age-based flagging disabled (None) and no deadline -> not due.
        assert!(!summary_of(&adr, old, today, None, seq).review_due);
        // A recent proposal is not stale.
        assert!(!summary_of(&adr, recent, today, Some(30), seq).review_due);
        // Non-proposed ADRs never count, however old.
        adr.status = Status::Accepted;
        assert!(!summary_of(&adr, old, today, Some(30), seq).review_due);
    }
}

//! Git-derived ADR dates: the real creation date and last-modified date,
//! reconstructed from the repository's commit log.
//!
//! **Why git.** A fresh clone resets every file's mtime to checkout time, so
//! the filesystem can't tell you when an ADR was first added; git can. (The
//! old by_status status *timeline* — reconstructed from directory renames —
//! retired with the layouts: in a flat KB space a status change is an in-place
//! frontmatter rewrite, so lifecycle history would have to come from content
//! diffs; ADR-0020 accepts its absence.)
//!
//! This module only **reads** git (via `git log`) and degrades gracefully:
//! outside a git repo, or for an untracked file, the lookups return `None` and
//! callers fall back to other date sources.

use std::path::{Path, PathBuf};
use std::process::Command;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// A handle to the git repository containing an ADR tree. Created once via
/// [`open`] so per-file lookups don't each re-probe for a repository.
#[derive(Debug, Clone)]
pub struct GitRepo {
    /// Directory to run `git -C` in (the resolved ADR dir; git walks up to the
    /// enclosing work tree on its own).
    dir: PathBuf,
}

/// The git-derived history of a single ADR file.
#[derive(Debug, Clone)]
pub struct AdrHistory {
    /// When the file was first added to the repo (oldest commit touching it).
    pub created: OffsetDateTime,
    /// The most recent commit that touched the file.
    pub last_modified: OffsetDateTime,
}

/// Probe for a git repository at (or above) `dir`. Returns `None` when git is
/// unavailable or `dir` is not inside a work tree — callers then fall back to
/// non-git date sources. Run once; reuse the handle for many files.
pub fn open(dir: &Path) -> Option<GitRepo> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .ok()?;
    if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true" {
        Some(GitRepo {
            dir: dir.to_path_buf(),
        })
    } else {
        None
    }
}

impl GitRepo {
    /// Whether this is a shallow clone (`git clone --depth=…`). On a shallow
    /// clone `git log --follow` can't see a file's true first commit, so
    /// creation dates are unreliable — callers in strict `git` mode warn.
    pub fn is_shallow(&self) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["rev-parse", "--is-shallow-repository"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
            .unwrap_or(false)
    }

    /// Git-derived dates for `file`, or `None` if it is untracked / has no
    /// commits.
    ///
    /// Performance: this runs one `git log` per file. ADR repos are small
    /// (dozens of files), so the per-file cost is fine; a single-pass log over
    /// the whole tree could be a future optimization if a repo grows large.
    pub fn history(&self, file: &Path) -> Option<AdrHistory> {
        // `--follow` links a rename (e.g. `renumber`) to the same logical file.
        // `%x1f` (unit separator) prefixes each commit header line so headers
        // can't be confused with any other output.
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["log", "--follow", "--format=%x1f%aI"])
            .arg("--")
            .arg(file)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_log(&String::from_utf8_lossy(&out.stdout))
    }
}

/// Parse `git log --follow --format=%x1f%aI` output (newest commit first) into
/// an [`AdrHistory`]. Split out from the git call so it can be unit-tested
/// without a repository. Returns `None` if no commits are present (untracked
/// file).
fn parse_log(text: &str) -> Option<AdrHistory> {
    let mut newest: Option<OffsetDateTime> = None;
    let mut oldest: Option<OffsetDateTime> = None;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('\u{1f}') {
            let date = OffsetDateTime::parse(rest.trim(), &Rfc3339).ok()?;
            newest.get_or_insert(date);
            oldest = Some(date);
        }
    }

    Some(AdrHistory {
        created: oldest?,
        last_modified: newest?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const US: char = '\u{1f}';

    #[test]
    fn parses_created_and_last_modified() {
        // Newest first: a later edit, then the original add.
        let log = format!(
            "{US}2026-05-07T09:00:00-04:00\n\
             {US}2026-04-10T14:10:45-04:00\n"
        );
        let h = parse_log(&log).unwrap();
        assert_eq!(
            h.created,
            OffsetDateTime::parse("2026-04-10T14:10:45-04:00", &Rfc3339).unwrap()
        );
        assert_eq!(
            h.last_modified,
            OffsetDateTime::parse("2026-05-07T09:00:00-04:00", &Rfc3339).unwrap()
        );
    }

    #[test]
    fn empty_log_is_none() {
        assert!(parse_log("").is_none());
    }

    #[test]
    fn end_to_end_against_a_real_repo() {
        // Skip gracefully if git isn't on PATH (keeps the suite green anywhere).
        if Command::new("git").arg("--version").output().is_err() {
            eprintln!("git not available; skipping end_to_end_against_a_real_repo");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Commit dates are pinned (deterministic-rerun lesson): on a
        // clock-stepping host (e.g. NTP under WSL2) two wall-clock commits can
        // land out of order, flaking `created <= last_modified` — the
        // iteration-2 integration gate hit exactly that.
        let git = |args: &[&str], date: &str| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(root)
                // Deterministic identity so commits succeed in any environment;
                // signing off so a contributor's global commit.gpgsign can't fail it.
                .args([
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "user.name=t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .env("GIT_AUTHOR_DATE", date)
                .env("GIT_COMMITTER_DATE", date)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?} failed");
        };
        const T1: &str = "2026-01-01T00:00:00Z";
        const T2: &str = "2026-01-02T00:00:00Z";
        git(&["init", "-q"], T1);
        std::fs::write(root.join("0001-x.md"), "---\ntitle: X\n---\n").unwrap();
        git(&["add", "."], T1);
        git(&["commit", "-q", "-m", "propose"], T1);
        std::fs::write(
            root.join("0001-x.md"),
            "---\ntitle: X\nstatus: accepted\n---\n",
        )
        .unwrap();
        git(&["add", "."], T2);
        git(&["commit", "-q", "-m", "accept"], T2);

        let repo = open(root).expect("temp dir is a git work tree");
        let h = repo
            .history(&root.join("0001-x.md"))
            .expect("file is tracked");
        assert!(h.created < h.last_modified);
    }
}

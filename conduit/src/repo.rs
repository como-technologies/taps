//! Internal git: bare repos as local remotes (portfolio ADR-0015 point 3)
//! and the mechanical merge door's git half.
//!
//! One bare repo per project lives beside the store
//! (`.conduit/repos/<project-stem>.git`), provisioned on first claim with
//! an empty root commit so branches always have a base. The workspace
//! clones from and pushes to it like any remote; mirroring to external
//! forges is a later integration, never product code here.
//!
//! [`merge_task`] is the squash half of the merge door: verify the branch
//! exists, run the project's gate command against a fresh checkout of the
//! branch, and on green squash-merge it onto `main` as ONE commit whose
//! message is the task title with a `work-item:` trailer — main reads as a
//! sequence of tasks, and the trailer points back at the KB page while the
//! page's `merge_commit` points at the sha. Everything shells out to the
//! `git` CLI (the house has no libgit dependency), each call
//! deadline-bounded is not needed here — but the GATE run is, via the
//! shared process harness: a hung gate must not hang the door.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};

/// Where a project's bare repo lives, relative to the working directory.
pub fn repo_path(dir: &Path, project_stem: &str) -> PathBuf {
    dir.join(".conduit")
        .join("repos")
        .join(format!("{project_stem}.git"))
}

/// The branch a task's work lands on.
pub fn branch_name(task_stem: &str) -> String {
    format!("work/{task_stem}")
}

/// Run git with args in `cwd`, capturing output; error carries stderr.
fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .with_context(|| format!("cannot run git {args:?}"))?;
    if !out.status.success() {
        bail!(
            "git {:?} failed ({}): {}",
            args,
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The identity the door commits under — deliberately not a human's: the
/// squash commit is the door's act, the human's act is the sign-off seal.
const DOOR_IDENT: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "conduit"),
    ("GIT_AUTHOR_EMAIL", "conduit@localhost"),
    ("GIT_COMMITTER_NAME", "conduit"),
    ("GIT_COMMITTER_EMAIL", "conduit@localhost"),
];

fn git_as_door(cwd: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    // The door's commits are mechanical and must not depend on operator git
    // config (a global commit.gpgsign would wedge the merge door).
    cmd.args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0");
    for (k, v) in DOOR_IDENT {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .with_context(|| format!("cannot run git {args:?}"))?;
    if !out.status.success() {
        bail!(
            "git {:?} failed ({}): {}",
            args,
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Ensure the project's bare repo exists (init + empty root commit on
/// `main` on first touch). Returns its path. Idempotent.
pub fn ensure_repo(dir: &Path, project_stem: &str) -> Result<PathBuf> {
    let path = repo_path(dir, project_stem);
    if path.join("HEAD").exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(&path).with_context(|| format!("cannot create {}", path.display()))?;
    git(&path, &["init", "--bare", "--initial-branch=main", "."])?;
    // An empty root commit so every branch has a base to fork from and the
    // first squash merge has a parent.
    let tree = git_as_door(&path, &["mktree"])
        .or_else(|_| git_as_door(&path, &["hash-object", "-t", "tree", "/dev/null"]))?;
    let commit = git_as_door(
        &path,
        &["commit-tree", &tree, "-m", "conduit: repo provisioned"],
    )?;
    git(&path, &["update-ref", "refs/heads/main", &commit])?;
    Ok(path)
}

/// Ensure the task's branch exists at the tip of `main` (no-op if present).
/// Returns the branch name.
pub fn ensure_branch(repo: &Path, task_stem: &str) -> Result<String> {
    let branch = branch_name(task_stem);
    let ref_name = format!("refs/heads/{branch}");
    if git(repo, &["show-ref", "--verify", "--quiet", &ref_name]).is_ok() {
        return Ok(branch);
    }
    let main = git(repo, &["rev-parse", "refs/heads/main"])?;
    git(repo, &["update-ref", &ref_name, &main])?;
    Ok(branch)
}

/// What the merge door produced.
#[derive(Debug)]
pub struct Merged {
    /// The single squash commit now at the tip of `main`.
    pub merge_commit: String,
}

/// The mechanical merge: gate first, squash second.
///
/// Clones the bare repo to a scratch workspace, checks out the task branch,
/// runs `gate` (deadline-bounded — a hung gate fails the merge, never hangs
/// the door), and on green squash-merges the branch onto `main` as one
/// commit (`<title>` + `work-item: <id>` trailer), pushed back to the bare
/// repo. Refuses an empty diff — a task that changed nothing has nothing to
/// merge.
pub fn merge_task(
    repo: &Path,
    branch: &str,
    title: &str,
    work_item_id: &str,
    gate: &str,
    gate_timeout: Duration,
    scratch: &Path,
) -> Result<Merged> {
    let ws = scratch.join("merge-ws");
    if ws.exists() {
        std::fs::remove_dir_all(&ws).ok();
    }
    std::fs::create_dir_all(scratch)
        .with_context(|| format!("cannot create {}", scratch.display()))?;
    let repo_str = repo.to_string_lossy();
    git(scratch, &["clone", "--quiet", &repo_str, "merge-ws"])?;

    // The gate runs against the BRANCH content, exactly what would merge.
    git(&ws, &["checkout", "--quiet", branch])?;
    let mut gate_cmd = Command::new("sh");
    gate_cmd.arg("-c").arg(gate).current_dir(&ws);
    let out = crate::proc::run_with_deadline(&mut gate_cmd, gate_timeout)
        .with_context(|| format!("cannot run gate {gate:?}"))?;
    match out.status {
        None => bail!(
            "gate {gate:?} exceeded the {}s deadline (process group killed) — the merge door stays shut",
            gate_timeout.as_secs()
        ),
        Some(s) if !s.success() => bail!(
            "gate {gate:?} failed ({s}) — the merge door stays shut\n--- gate stderr (tail) ---\n{}",
            tail(&out.stderr, 2000)
        ),
        Some(_) => {}
    }

    // Squash: one commit on main, title + work-item trailer.
    git(&ws, &["checkout", "--quiet", "main"])?;
    git(&ws, &["merge", "--squash", "--quiet", branch])?;
    let staged = git(&ws, &["diff", "--cached", "--name-only"])?;
    if staged.is_empty() {
        bail!("branch {branch} has no changes against main — nothing to merge");
    }
    let message = format!("{title}\n\nwork-item: {work_item_id}");
    git_as_door(&ws, &["commit", "--quiet", "-m", &message])?;
    let sha = git(&ws, &["rev-parse", "HEAD"])?;
    git(&ws, &["push", "--quiet", "origin", "main"])?;
    std::fs::remove_dir_all(&ws).ok();
    Ok(Merged { merge_commit: sha })
}

fn tail(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    let s = s.trim_end();
    if s.len() <= max {
        s.to_string()
    } else {
        format!("…{}", &s[s.len() - max..])
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Clone, add a file on the task branch, push — a stand-in for the
    /// execution session's workspace.
    fn push_work(repo: &Path, branch: &str, scratch: &Path, file: &str, content: &str) {
        let ws = scratch.join("work-ws");
        git(
            scratch,
            &["clone", "--quiet", &repo.to_string_lossy(), "work-ws"],
        )
        .unwrap();
        git(&ws, &["checkout", "--quiet", branch]).unwrap();
        std::fs::write(ws.join(file), content).unwrap();
        git(&ws, &["add", "."]).unwrap();
        git_as_door(&ws, &["commit", "--quiet", "-m", "wip 1"]).unwrap();
        std::fs::write(ws.join(file), format!("{content}!")).unwrap();
        git(&ws, &["add", "."]).unwrap();
        git_as_door(&ws, &["commit", "--quiet", "-m", "wip 2"]).unwrap();
        git(&ws, &["push", "--quiet", "origin", branch]).unwrap();
        std::fs::remove_dir_all(&ws).unwrap();
    }

    #[test]
    fn provision_is_idempotent_and_branches_fork_from_main() {
        let d = TempDir::new().unwrap();
        let repo = ensure_repo(d.path(), "project-p").unwrap();
        assert!(repo.join("HEAD").exists());
        assert_eq!(ensure_repo(d.path(), "project-p").unwrap(), repo);
        let b = ensure_branch(&repo, "task-t").unwrap();
        assert_eq!(b, "work/task-t");
        assert_eq!(ensure_branch(&repo, "task-t").unwrap(), b, "idempotent");
        let main = git(&repo, &["rev-parse", "refs/heads/main"]).unwrap();
        let tip = git(&repo, &["rev-parse", "refs/heads/work/task-t"]).unwrap();
        assert_eq!(main, tip);
    }

    #[test]
    fn green_gate_squashes_to_one_commit_with_the_trailer() {
        let d = TempDir::new().unwrap();
        let repo = ensure_repo(d.path(), "project-p").unwrap();
        let branch = ensure_branch(&repo, "task-t").unwrap();
        push_work(&repo, &branch, d.path(), "hello.txt", "hi");

        let merged = merge_task(
            &repo,
            &branch,
            "Say hello",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "test -f hello.txt",
            Duration::from_secs(60),
            &d.path().join("scratch"),
        )
        .unwrap();

        // main advanced exactly one commit past the root; message carries
        // title + trailer; the two wip commits are squashed away.
        let log = git(&repo, &["log", "--format=%H %s", "refs/heads/main"]).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2, "root + one squash commit: {log}");
        assert!(lines[0].starts_with(&merged.merge_commit));
        assert!(lines[0].contains("Say hello"));
        let body = git(&repo, &["log", "-1", "--format=%B", &merged.merge_commit]).unwrap();
        assert!(body.contains("work-item: 01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    }

    #[test]
    fn red_gate_keeps_the_door_shut_and_main_untouched() {
        let d = TempDir::new().unwrap();
        let repo = ensure_repo(d.path(), "project-p").unwrap();
        let branch = ensure_branch(&repo, "task-t").unwrap();
        push_work(&repo, &branch, d.path(), "hello.txt", "hi");
        let main_before = git(&repo, &["rev-parse", "refs/heads/main"]).unwrap();

        let err = merge_task(
            &repo,
            &branch,
            "Say hello",
            "id",
            "exit 3",
            Duration::from_secs(60),
            &d.path().join("scratch"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("merge door stays shut"), "{err}");
        let main_after = git(&repo, &["rev-parse", "refs/heads/main"]).unwrap();
        assert_eq!(main_before, main_after);
    }

    #[test]
    fn empty_branch_has_nothing_to_merge() {
        let d = TempDir::new().unwrap();
        let repo = ensure_repo(d.path(), "project-p").unwrap();
        let branch = ensure_branch(&repo, "task-t").unwrap();
        let err = merge_task(
            &repo,
            &branch,
            "Nothing",
            "id",
            "true",
            Duration::from_secs(60),
            &d.path().join("scratch"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("nothing to merge"), "{err}");
    }

    #[test]
    fn hung_gate_is_group_killed_not_waited_on() {
        let d = TempDir::new().unwrap();
        let repo = ensure_repo(d.path(), "project-p").unwrap();
        let branch = ensure_branch(&repo, "task-t").unwrap();
        push_work(&repo, &branch, d.path(), "hello.txt", "hi");
        let start = std::time::Instant::now();
        let err = merge_task(
            &repo,
            &branch,
            "Hang",
            "id",
            "sleep 300",
            Duration::from_millis(300),
            &d.path().join("scratch"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("deadline"), "{err}");
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the deadline must bound the gate"
        );
    }
}

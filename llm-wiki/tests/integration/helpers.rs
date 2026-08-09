//! Shared harness for the end-to-end suite: builds a two-wiki environment
//! from `tests/fixtures/` and drives the real `llm-wiki` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Path to the compiled `llm-wiki` binary under test.
pub const BIN: &str = env!("CARGO_BIN_EXE_llm-wiki");

/// Root of the shared test fixtures.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Recursively copy a directory tree.
fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Stage everything (including deletions) and commit with a fixed test signature.
pub fn commit_all(repo_root: &Path, message: &str) {
    let repo = git2::Repository::open(repo_root).unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.update_all(["*"].iter(), None).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let sig = git2::Signature::now("test", "test@test.com").unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

/// A hermetic two-wiki environment mirroring the fixture layout:
/// `research` (default, with inbox files) and `notes`, both git repos,
/// registered in a tempdir-local global config.
pub struct WikiEnv {
    dir: tempfile::TempDir,
    /// Global config file passed to every invocation via `--config`.
    pub config: PathBuf,
    /// Repo root of the research wiki.
    pub research: PathBuf,
    /// Repo root of the notes wiki.
    pub notes: PathBuf,
    /// Page content root of the research wiki.
    pub research_wiki: PathBuf,
    /// Inbox directory inside the research wiki.
    pub inbox: PathBuf,
}

impl WikiEnv {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let research = dir.path().join("wikis").join("research");
        let notes = dir.path().join("wikis").join("notes");

        copy_dir(&fixtures().join("wikis/research"), &research);
        copy_dir(&fixtures().join("wikis/notes"), &notes);
        llm_wiki::git::init_repo(&research).unwrap();
        commit_all(&research, "init");
        llm_wiki::git::init_repo(&notes).unwrap();
        commit_all(&notes, "init");

        // Copy inbox fixtures into the research wiki inbox
        let inbox = research.join("content").join("inbox");
        fs::create_dir_all(&inbox).unwrap();
        for entry in fs::read_dir(fixtures().join("inbox")).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), inbox.join(entry.file_name())).unwrap();
        }
        commit_all(&research, "add inbox");

        let env = WikiEnv {
            research_wiki: research.join("content"),
            inbox,
            dir,
            config,
            research,
            notes,
        };

        // Register both wikis; research is the default
        let research_path = env.research.to_str().unwrap().to_string();
        let notes_path = env.notes.to_str().unwrap().to_string();
        env.run(&["admin", "register", "--name", "research", &research_path]);
        env.run(&["admin", "set-default", "research"]);
        env.run(&["admin", "register", "--name", "notes", &notes_path]);

        env
    }

    /// Tempdir root (scratch wiki shared with nothing else).
    pub fn tmp(&self) -> &Path {
        self.dir.path()
    }

    /// Run the binary with `--config`; panics if the command fails.
    pub fn run(&self, args: &[&str]) -> Output {
        let out = self.run_unchecked(args);
        assert!(
            out.status.success(),
            "command failed: {args:?}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        out
    }

    /// Run the binary with `--config`; returns the output without asserting.
    pub fn run_unchecked(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .arg("--config")
            .arg(&self.config)
            .args(args)
            .output()
            .unwrap()
    }

    /// Run with `--format json` appended and parse stdout.
    pub fn json(&self, args: &[&str]) -> serde_json::Value {
        let mut full = args.to_vec();
        full.extend(["--format", "json"]);
        let out = self.run(&full);
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "invalid JSON from {args:?}: {e}\nstdout: {}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }

    /// Rebuild the search index for a wiki.
    pub fn rebuild(&self, wiki: &str) {
        self.run(&["admin", "index", "rebuild", "--wiki", wiki]);
    }
}

/// UTF-8 stdout of a finished command.
pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// UTF-8 stderr of a finished command.
pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

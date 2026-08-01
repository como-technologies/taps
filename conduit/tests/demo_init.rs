//! A4: `client-corpus-init.sh`'s `REPO_NAME` knob templates the generated
//! `conduit.toml` without a manual sed (run-3 wart 3 — the third sighting of
//! the hardcoded-demo-target lesson), and — since the KB-native demo (the
//! adroit pin is KB-only per its ADR-0020) — the script scaffolds and seeds
//! the per-run corpus SPACE (`<workdir>/corpus-space`). These drive the REAL
//! init script against a minimal legacy corpus (the committed fixture) and
//! load the generated config as the oracle, so a regression in the templating
//! or the space seeding fails the gate, not the next live demo.
//!
//! The script runs the pinned adroit's `seed`, so these tests need the pinned
//! install (`just init-adroit` — `just ci` orders adr-check before test for
//! exactly this). Missing pin = skip-with-notice, mirroring the env-gated
//! legs, so a bare `cargo test` on a cold checkout still passes.

use std::path::{Path, PathBuf};
use std::process::Command;

use conduit::config::Config;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The pinned adroit install the init script invokes for `seed`. Path built
/// by segments (never the literal fragment the crate-wide allowlist walker
/// scans for — invoking adroit stays the script's business, not this test's).
fn pinned_adroit() -> PathBuf {
    repo_root().join(".conduit").join("bin").join("adroit")
}

/// Run the real init script against a legacy-shape corpus (the committed
/// fixture, copied under docs/src/adr); return the created RUN_DIR, or None
/// (skip) when the pinned adroit is not installed. COMO_OFFLINE keeps it
/// network-free; CLIENT_CORPUS_DIR is explicit so no llm-wiki resolution or
/// corpus build is exercised.
fn run_init(tmp: &Path, repo_name: Option<&str>) -> Option<PathBuf> {
    if !pinned_adroit().exists() {
        eprintln!("skip: pinned adroit not installed — run `just init-adroit`");
        return None;
    }
    let corpus = tmp.join("corpus");
    let adr = corpus.join("docs/src/adr");
    std::fs::create_dir_all(&adr).unwrap();
    // A real legacy corpus: the committed by-status fixture (3 ADRs), so the
    // script's `adroit seed` has documents to seed into the space.
    copy_tree(&repo_root().join("tests/fixtures/corpus"), &adr);
    let run_dir = tmp.join("run"); // must NOT pre-exist; the script errors if it does

    let mut cmd = Command::new("bash");
    cmd.current_dir(repo_root())
        .arg("demo/client-corpus-init.sh")
        .env("RUN_DIR", &run_dir)
        .env("CLIENT_CORPUS_DIR", &corpus)
        .env("COMO_OFFLINE", "1");
    if let Some(name) = repo_name {
        cmd.env("REPO_NAME", name);
    }
    let out = cmd.output().expect("run client-corpus-init.sh");
    assert!(
        out.status.success(),
        "init script failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(run_dir)
}

fn copy_tree(from: &Path, to: &Path) {
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let dest = to.join(e.file_name());
        if e.path().is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
            copy_tree(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), &dest).unwrap();
        }
    }
}

#[test]
fn repo_name_knob_templates_the_generated_toml_without_sed() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(run_dir) = run_init(tmp.path(), Some("run3-corpus-demo")) else {
        return;
    };

    let toml = std::fs::read_to_string(run_dir.join("conduit.toml")).unwrap();
    assert!(
        !toml.contains("@REPO_NAME@"),
        "placeholder survived:\n{toml}"
    );

    let cfg = Config::load(&run_dir).expect("load generated conduit.toml");
    assert_eq!(cfg.forge.gitea.repo, "run3-corpus-demo");
}

#[test]
fn repo_name_default_preserves_the_client_corpus_demo() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(run_dir) = run_init(tmp.path(), None) else {
        return;
    };
    let cfg = Config::load(&run_dir).expect("load generated conduit.toml");
    assert_eq!(cfg.forge.gitea.repo, "client-corpus");
}

/// The KB-native contract: the workdir carries a seeded corpus SPACE —
/// wiki.toml present, the fixture's decisions seeded into wiki/decisions,
/// and the generated conduit.toml's `[adroit] dir` naming the space.
#[test]
fn corpus_space_is_scaffolded_seeded_and_wired_into_the_config() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(run_dir) = run_init(tmp.path(), Some("anything")) else {
        return;
    };

    let space = run_dir.join("corpus-space");
    assert!(
        space.join("wiki.toml").is_file(),
        "corpus-space carries no wiki.toml"
    );
    let decisions = space.join("wiki/decisions");
    let pages: Vec<_> = std::fs::read_dir(&decisions)
        .expect("wiki/decisions exists")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    assert_eq!(
        pages.len(),
        3,
        "the 3 fixture ADRs seeded as decision pages: {pages:?}"
    );

    let cfg = Config::load(&run_dir).expect("load generated conduit.toml");
    assert!(
        cfg.adroit.dir.ends_with("corpus-space"),
        "adroit dir must name the space: {}",
        cfg.adroit.dir
    );
    assert!(!cfg.adroit.dir.contains("@ADROIT_DIR@"));
    // The configured dir is a live space path (not just a plausible string):
    // it resolves to the same wiki.toml the workdir's space carries.
    assert_eq!(
        Path::new(&cfg.adroit.dir)
            .join("wiki.toml")
            .canonicalize()
            .expect("configured space has a wiki.toml"),
        space.join("wiki.toml").canonicalize().unwrap(),
        "config points at THIS run's space"
    );
}

//! The terminal door's process-level shape: the surface is the door verbs
//! and nothing else, and a session with no KB configured fails loudly with
//! the discovery instructions, never a stack trace or a silent default.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn conduit(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("conduit").unwrap();
    cmd.current_dir(dir.path());
    // Hermetic: no KB from the developer environment, no user-level config
    // (a fake HOME keeps ~/.config/taps/env out of the discovery order).
    cmd.env_remove("KB_URL")
        .env_remove("KB_WIKI")
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path());
    cmd
}

#[test]
fn help_is_the_door_surface() {
    let d = TempDir::new().unwrap();
    let out = conduit(&d).arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&out.stdout).to_string();
    for verb in [
        "new", "list", "show", "signoff", "bounce", "claim", "complete", "close", "cancel", "mcp",
    ] {
        assert!(help.contains(verb), "help must list {verb}");
    }
    for gone in ["forge", "run ", "verify", "demo-transcript"] {
        assert!(!help.contains(gone), "the old surface leaked: {gone}");
    }
}

#[test]
fn no_kb_is_a_loud_typed_failure_naming_the_discovery_order() {
    let d = TempDir::new().unwrap();
    conduit(&d)
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no KB configured"))
        .stderr(predicate::str::contains("KB_URL"));
}

#[test]
fn the_kb_pair_is_discovered_from_a_workspace_dotenv() {
    // A .env in the working directory reaches the doors (the suite-wide
    // discovery order): the failure shifts from "no KB configured" to a
    // connection error against the configured endpoint.
    let d = TempDir::new().unwrap();
    std::fs::write(d.path().join(".env"), "KB_URL=http://127.0.0.1:1/mcp\n").unwrap();
    conduit(&d)
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no KB configured").not());
}

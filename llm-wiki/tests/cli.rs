use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_llm-wiki"))
}

#[test]
fn config_flag_overrides_default_path() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    // Empty TOML is a valid GlobalConfig (all fields have defaults)
    std::fs::write(&config, "").unwrap();

    let out = binary()
        .args(["--config", config.to_str().unwrap(), "admin", "list"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn llm_wiki_config_env_var_overrides_default_path() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("env-config.toml");
    std::fs::write(&config, "").unwrap();

    let out = binary()
        .env("LLM_WIKI_CONFIG", config.to_str().unwrap())
        // Ensure the host's real default config can't leak in
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path())
        .args(["admin", "list"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn default_config_path_is_xdg() {
    let dir = tempfile::tempdir().unwrap();
    // No --config, no LLM_WIKI_CONFIG: the registry resolves to
    // $XDG_CONFIG_HOME/llm-wiki/config.toml (taps #101 — `~/.llm-wiki` dies).
    let xdg = dir.path().join("xdg-config");
    std::fs::create_dir_all(xdg.join("llm-wiki")).unwrap();
    std::fs::write(xdg.join("llm-wiki").join("config.toml"), "").unwrap();
    // A legacy dotdir config that must NOT be read.
    std::fs::create_dir_all(dir.path().join(".llm-wiki")).unwrap();
    std::fs::write(
        dir.path().join(".llm-wiki").join("config.toml"),
        "not toml — reading this file is the bug",
    )
    .unwrap();

    let out = binary()
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .env_remove("LLM_WIKI_CONFIG")
        .args(["admin", "list"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn config_flag_takes_priority_over_env_var() {
    let dir = tempfile::tempdir().unwrap();
    let flag_config = dir.path().join("flag.toml");
    let env_config = dir.path().join("env.toml");
    std::fs::write(&flag_config, "").unwrap();
    std::fs::write(&env_config, "").unwrap();

    let out = binary()
        .args(["--config", flag_config.to_str().unwrap(), "admin", "list"])
        .env("LLM_WIKI_CONFIG", env_config.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

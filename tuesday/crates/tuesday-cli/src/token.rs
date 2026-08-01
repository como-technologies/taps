//! Token sourcing for the forge providers.
//!
//! Resolution order:
//! 1. `--token-file <PATH>` — read and trim; unreadable or blank is an error.
//! 2. The source's environment variable: `GITHUB_TOKEN` / `TUESDAY_GITEA_TOKEN`.
//! 3. For Gitea only: the dogfood contract's documented reviewer-token
//!    path, `${COMO_CONDUIT_DIR:-../conduit}/.secrets/reviewer.token`
//!    (relative to the working directory — the dogfood runs from the
//!    tuesday repo root), if present. Measure reads with the reviewer
//!    identity; the path is gitignored on the conduit side.
//!
//! The token is a runtime secret, minted per forge-up session by conduit's
//! `demo/gitea-init.sh` — per the suite resolution convention it is
//! env-first and NEVER resolved via git (never cloned, never committed,
//! never logged). Only the conduit *checkout location* is resolvable:
//! the `COMO_CONDUIT_DIR` knob, defaulting to the sibling `../conduit`.
//!
//! GitHub with no token anywhere is an error (the GraphQL API requires
//! auth); Gitea with no token anywhere proceeds anonymously (public forges
//! are readable without one).

use crate::cli::SourceKind;
use std::path::{Path, PathBuf};

/// Environment knob naming the conduit checkout (the suite resolution
/// convention's `COMO_<REPO>_DIR` override); the sibling `../conduit` is
/// the default.
pub const CONDUIT_DIR_ENV: &str = "COMO_CONDUIT_DIR";

/// The dogfood contract's documented default reviewer-token path:
/// `${COMO_CONDUIT_DIR:-../conduit}/.secrets/reviewer.token`. The relative
/// default resolves against the working directory — the dogfood runs from
/// the tuesday repo root.
pub fn gitea_default_token_path() -> PathBuf {
    gitea_default_token_path_from(|name| std::env::var(name).ok())
}

/// [`gitea_default_token_path`] with the environment lookup injected, so
/// the `COMO_CONDUIT_DIR` override is testable without process state.
pub fn gitea_default_token_path_from(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    let conduit_dir = env(CONDUIT_DIR_ENV)
        .filter(|dir| !dir.trim().is_empty())
        .unwrap_or_else(|| "../conduit".to_string());
    Path::new(&conduit_dir)
        .join(".secrets")
        .join("reviewer.token")
}

/// Environment variable consulted for `--source github`.
pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";

/// Environment variable consulted for `--source gitea`.
pub const GITEA_TOKEN_ENV: &str = "TUESDAY_GITEA_TOKEN";

/// Resolve a token using process environment and the documented default path.
pub fn resolve_token(
    source: SourceKind,
    token_file: Option<&Path>,
) -> Result<Option<String>, String> {
    resolve_token_with(
        source,
        token_file,
        |name| std::env::var(name).ok(),
        &gitea_default_token_path(),
    )
}

/// [`resolve_token`] with the environment lookup and default token path
/// injected, so the precedence chain is testable without process state.
pub fn resolve_token_with(
    source: SourceKind,
    token_file: Option<&Path>,
    env: impl Fn(&str) -> Option<String>,
    gitea_default_path: &Path,
) -> Result<Option<String>, String> {
    // 1. Explicit --token-file always wins; problems with it are errors.
    if let Some(path) = token_file {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("token file {}: {e}", path.display()))?;
        let token = contents.trim();
        if token.is_empty() {
            return Err(format!("token file {} is empty", path.display()));
        }
        return Ok(Some(token.to_string()));
    }

    // 2. The source's own environment variable (never the other forge's).
    let env_var = match source {
        SourceKind::Github => GITHUB_TOKEN_ENV,
        SourceKind::Gitea => GITEA_TOKEN_ENV,
    };
    if let Some(token) = env(env_var)
        && !token.trim().is_empty()
    {
        return Ok(Some(token.trim().to_string()));
    }

    // 3. Gitea only: the documented dogfood reviewer-token path, if present.
    if source == SourceKind::Gitea {
        if let Ok(contents) = std::fs::read_to_string(gitea_default_path) {
            let token = contents.trim();
            if !token.is_empty() {
                return Ok(Some(token.to_string()));
            }
        }
        // Anonymous reads are legitimate against a public forge.
        return Ok(None);
    }

    Err(format!(
        "github needs a token: pass --token-file <PATH> or set {GITHUB_TOKEN_ENV}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// A unique temp file holding `contents`, cleaned up on drop.
    struct TokenFile {
        path: PathBuf,
    }

    impl TokenFile {
        fn new(label: &str, contents: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("tuesday-cli-token-{}-{label}", std::process::id()));
            let mut file = std::fs::File::create(&path).unwrap();
            file.write_all(contents.as_bytes()).unwrap();
            Self { path }
        }
    }

    impl Drop for TokenFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn missing_path() -> PathBuf {
        std::env::temp_dir().join("tuesday-cli-token-does-not-exist")
    }

    #[test]
    fn token_file_wins_over_env() {
        let file = TokenFile::new("file-wins", "file-token\n");
        let token = resolve_token_with(
            SourceKind::Gitea,
            Some(&file.path),
            |_| Some("env-token".to_string()),
            &missing_path(),
        )
        .unwrap();
        assert_eq!(token, Some("file-token".to_string()));
    }

    #[test]
    fn unreadable_token_file_is_an_error() {
        let err = resolve_token_with(
            SourceKind::Gitea,
            Some(&missing_path()),
            no_env,
            &missing_path(),
        )
        .unwrap_err();
        assert!(err.contains("token file"), "error names the file: {err}");
    }

    #[test]
    fn blank_token_file_is_an_error() {
        let file = TokenFile::new("blank", "  \n");
        let err = resolve_token_with(SourceKind::Gitea, Some(&file.path), no_env, &missing_path())
            .unwrap_err();
        assert!(err.contains("empty"), "error says the file is empty: {err}");
    }

    #[test]
    fn github_env_var_is_used_when_no_file() {
        let env = |name: &str| (name == GITHUB_TOKEN_ENV).then(|| "gh-token".to_string());
        let token = resolve_token_with(SourceKind::Github, None, env, &missing_path()).unwrap();
        assert_eq!(token, Some("gh-token".to_string()));
    }

    #[test]
    fn gitea_env_var_is_used_when_no_file() {
        let env = |name: &str| (name == GITEA_TOKEN_ENV).then(|| "gitea-token".to_string());
        let token = resolve_token_with(SourceKind::Gitea, None, env, &missing_path()).unwrap();
        assert_eq!(token, Some("gitea-token".to_string()));
    }

    #[test]
    fn each_source_reads_only_its_own_env_var() {
        // A GITHUB_TOKEN in the environment must not leak to the gitea forge.
        let env = |name: &str| (name == GITHUB_TOKEN_ENV).then(|| "gh-token".to_string());
        let token = resolve_token_with(SourceKind::Gitea, None, env, &missing_path()).unwrap();
        assert_eq!(token, None);
    }

    #[test]
    fn empty_env_var_is_ignored() {
        let env = |_: &str| Some(String::new());
        let token = resolve_token_with(SourceKind::Gitea, None, env, &missing_path()).unwrap();
        assert_eq!(token, None);
    }

    #[test]
    fn default_token_path_is_under_the_conduit_sibling() {
        let path = gitea_default_token_path_from(|_| None);
        assert_eq!(path, Path::new("../conduit/.secrets/reviewer.token"));
    }

    #[test]
    fn como_conduit_dir_overrides_the_default_token_path() {
        let env = |name: &str| (name == CONDUIT_DIR_ENV).then(|| "/opt/conduit".to_string());
        let path = gitea_default_token_path_from(env);
        assert_eq!(path, Path::new("/opt/conduit/.secrets/reviewer.token"));
    }

    #[test]
    fn blank_como_conduit_dir_falls_back_to_the_sibling_default() {
        let path = gitea_default_token_path_from(|_| Some("  ".to_string()));
        assert_eq!(path, Path::new("../conduit/.secrets/reviewer.token"));
    }

    #[test]
    fn gitea_falls_back_to_the_pinned_default_path() {
        let file = TokenFile::new("pinned-default", "reviewer-token\n");
        let token = resolve_token_with(SourceKind::Gitea, None, no_env, &file.path).unwrap();
        assert_eq!(token, Some("reviewer-token".to_string()));
    }

    #[test]
    fn github_never_reads_the_pinned_gitea_path() {
        let file = TokenFile::new("not-for-github", "reviewer-token\n");
        let err = resolve_token_with(SourceKind::Github, None, no_env, &file.path).unwrap_err();
        assert!(err.contains(GITHUB_TOKEN_ENV), "error tells the fix: {err}");
    }

    #[test]
    fn gitea_with_no_token_anywhere_is_anonymous() {
        let token = resolve_token_with(SourceKind::Gitea, None, no_env, &missing_path()).unwrap();
        assert_eq!(token, None);
    }

    #[test]
    fn github_with_no_token_anywhere_is_an_error() {
        let err =
            resolve_token_with(SourceKind::Github, None, no_env, &missing_path()).unwrap_err();
        assert!(
            err.contains("--token-file") && err.contains(GITHUB_TOKEN_ENV),
            "error lists both ways to supply a token: {err}"
        );
    }
}

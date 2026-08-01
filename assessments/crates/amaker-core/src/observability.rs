//! Tracing setup shared by the three binaries.
//!
//! Two output formats, one selector:
//!
//! - **JSON** — Google Cloud Logging structured format: each line is a
//!   JSON object with a real `severity` field (derived from the `tracing`
//!   level) and an rfc3339 `timestamp`. This is what makes
//!   `gcloud ... --log-filter='severity=ERROR'` agree with the level in
//!   the message — without it, Cloud Run tags every stdout line `INFO`
//!   regardless of content.
//! - **text** — the pretty human-readable formatter, for local dev.
//!
//! Selection: the `LOG_FORMAT` env var (`json` / `text`) wins; otherwise
//! JSON when `K_SERVICE` is set (Cloud Run injects it), text everywhere
//! else. So Cloud Run gets structured logs automatically and a local
//! `cargo run` / `docker run` stays readable.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Initialize the global tracing subscriber. Call once, at startup.
///
/// `log_level` is the base `EnvFilter` directive (the binary's
/// `LOG_LEVEL`). `quiet_targets` are tracing targets pinned to `warn` —
/// the author passes `rig::agent::prompt_request`, whose per-tool-call
/// payloads would otherwise bury everything else. They're emitted before
/// `log_level` so an explicit `LOG_LEVEL` directive for the same target
/// still wins.
pub fn init_tracing(log_level: &str, quiet_targets: &[&str]) {
    let mut directives = String::new();
    for target in quiet_targets {
        directives.push_str(target);
        directives.push_str("=warn,");
    }
    directives.push_str(log_level);
    let filter = EnvFilter::try_new(&directives).unwrap_or_else(|_| EnvFilter::new(log_level));

    let layer = if json_format() {
        tracing_stackdriver::layer().boxed()
    } else {
        tracing_subscriber::fmt::layer().boxed()
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .init();
}

/// Whether to emit Cloud Logging JSON. `LOG_FORMAT` overrides; otherwise
/// the presence of `K_SERVICE` (set by Cloud Run) is the signal.
fn json_format() -> bool {
    match std::env::var("LOG_FORMAT").ok().as_deref() {
        Some("json") => true,
        Some("text") | Some("pretty") => false,
        _ => std::env::var_os("K_SERVICE").is_some(),
    }
}

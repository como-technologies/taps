//! `adroit` binary entry point. The command surface lives in the `adroit`
//! library crate so the lifecycle oracle and integration tests can drive it
//! directly.

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The suite's config layers (cwd .env, ~/.config/taps/env) load before
    // clap parses, so `env =` defaults like ADROIT_NAMING see them too.
    como_kb_client::load_env();
    adroit::run(adroit::Cli::parse()).await
}

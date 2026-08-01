//! `amaker` binary entry point. The command surface lives in the
//! `amaker_cli` library crate so integration tests can drive it directly.

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    amaker_cli::run(amaker_cli::Cli::parse()).await
}

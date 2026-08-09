//! `adroit` binary entry point. The command surface lives in the `adroit`
//! library crate so the lifecycle oracle and integration tests can drive it
//! directly.

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    adroit::run(adroit::Cli::parse()).await
}

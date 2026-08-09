//! adroit — decision records for the taps loop.
//!
//! Greenfield per taps #93: the KB is the state store, adroit is the only
//! writer of `decision`/`plan` pages (ownership is a write boundary —
//! anyone reads them through the engine), and judgment belongs to agents
//! and humans driving the same thin verbs. The standalone-era product
//! lives on at <https://github.com/como-technologies/adroit>.

use clap::Parser;

/// Decision records for the taps loop.
#[derive(Parser, Debug)]
#[command(name = "adroit", version, about)]
struct Cli {}

fn main() {
    Cli::parse();
    eprintln!("adroit is being rebuilt as the loop's Prescribe tool — see taps issue #93");
}

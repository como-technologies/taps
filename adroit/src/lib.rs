//! adroit — decision records for the taps loop.
//!
//! Greenfield per taps #93: the KB is the state store ([`store`]), adroit is
//! the only writer of `decision`/`plan` pages (ownership is a write
//! boundary — anyone reads them through the engine's tools), and judgment
//! belongs to the agents and humans driving the same thin verbs
//! ([`surface`]) through three doors: terminal, `-o json`, and [`mcp`].
//!
//! Config is the suite pair (`KB_URL` / `KB_WIKI`) plus the naming default
//! (`ADROIT_NAMING` / `--naming`). Nothing else.

pub mod lifecycle;
pub mod lint;
pub mod mcp;
pub mod naming;
pub mod page;
pub mod plan;
pub mod store;
pub mod surface;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;

use crate::naming::NamingScheme;
use crate::store::KbStore;
use crate::surface::{
    LintParams, ListParams, NewParams, PlanParams, SetReviewParams, SetStatusParams, ShowParams,
    SupersedeParams,
};

/// How reports are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Output {
    /// Terse, human-shaped lines.
    #[default]
    Human,
    /// The core function's report, pretty-printed — the automation door.
    Json,
}

/// Decision records for the taps loop.
#[derive(Parser, Debug)]
#[command(name = "adroit", version, about)]
pub struct Cli {
    /// Output format for reports
    #[arg(short = 'o', long, global = true, value_enum, default_value = "human")]
    pub output: Output,
    /// Naming default for display references
    #[arg(
        long,
        global = true,
        value_enum,
        env = "ADROIT_NAMING",
        default_value = "sequential"
    )]
    pub naming: NamingScheme,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a proposed decision (title, context, body from your judgment;
    /// --relates records provenance edges)
    New(NewParams),
    /// List the decision corpus
    List(ListParams),
    /// Show one decision in full, reference-resolved
    Show(ShowParams),
    /// Authoring-quality gate on one decision's body (mechanical text rules)
    Lint(LintParams),
    /// Lifecycle transition in place (proposed → accepted/rejected;
    /// accepted → deprecated)
    SetStatus(SetStatusParams),
    /// Set or clear a review-by date
    SetReview(SetReviewParams),
    /// Link a new decision over an old one, both sides' frontmatter,
    /// statuses consistent
    Supersede(SupersedeParams),
    /// Read the stored implementation plan, or splice one in with --save
    Plan(PlanParams),
    /// Corpus invariants: supersession integrity, duplicate ids/titles
    Check,
    /// Serve the same surface to agents over MCP stdio
    Mcp,
}

/// Run the parsed CLI to completion. `lint` and `check` exit non-zero when
/// they find errors — the CI-gate contract.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    // The suite pair comes from the environment; a .env in the working
    // directory (e.g. a kb-workspace) is honored like every taps tool.
    dotenvy::dotenv().ok();

    if let Command::Mcp = cli.command {
        return mcp::serve(cli.naming).await;
    }

    let store = KbStore::connect().await?;
    let result = dispatch(&store, cli.naming, &cli.command).await;
    store.close().await.ok();
    let report = result?;

    match cli.output {
        Output::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        Output::Human => print_human(&cli.command, &report),
    }

    // Gate verbs fail the process when they found errors (warnings advise).
    if matches!(cli.command, Command::Lint(_) | Command::Check)
        && report["errors"].as_u64().unwrap_or(0) > 0
    {
        std::process::exit(1);
    }
    Ok(())
}

async fn dispatch(
    store: &KbStore,
    naming: NamingScheme,
    command: &Command,
) -> anyhow::Result<Value> {
    match command {
        Command::New(p) => surface::new_core(store, naming, p).await,
        Command::List(p) => surface::list_core(store, naming, p).await,
        Command::Show(p) => surface::show_core(store, naming, p).await,
        Command::Lint(p) => surface::lint_core(store, naming, p).await,
        Command::SetStatus(p) => surface::set_status_core(store, naming, p).await,
        Command::SetReview(p) => surface::set_review_core(store, naming, p).await,
        Command::Supersede(p) => surface::supersede_core(store, naming, p).await,
        Command::Plan(p) => surface::plan_core(store, naming, p).await,
        Command::Check => surface::check_core(store, naming).await,
        Command::Mcp => unreachable!("handled before connecting"),
    }
}

/// Terse human rendering of a core report. The JSON shapes are the
/// contract; this is a readable projection of the same keys.
fn print_human(command: &Command, r: &Value) {
    let s = |v: &Value| v.as_str().unwrap_or("?").to_string();
    match command {
        Command::New(_) => {
            println!(
                "created {} ({}) — {}",
                s(&r["reference"]),
                s(&r["slug"]),
                s(&r["status"])
            );
            for line in r["schemas"].as_array().into_iter().flatten() {
                println!("schema {}", s(line));
            }
        }
        Command::List(_) => {
            let rows = r["decisions"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                println!("no decisions in the space yet — start one with `adroit new`");
            }
            for row in rows {
                let due = if row["review_due"].as_bool().unwrap_or(false) {
                    "  [review due]"
                } else {
                    ""
                };
                println!(
                    "{:<10} {:<10} {}{}",
                    s(&row["reference"]),
                    s(&row["status"]),
                    s(&row["title"]),
                    due
                );
            }
        }
        Command::Show(_) => {
            println!(
                "{}: {} ({})",
                s(&r["reference"]),
                s(&r["title"]),
                s(&r["status"])
            );
            println!("slug: {}   id: {}", s(&r["slug"]), s(&r["id"]));
            println!("created: {}", s(&r["created"]));
            if let Some(rb) = r["review_by"].as_str() {
                println!("review by: {rb}");
            }
            for (label, key) in [
                ("supersedes", "supersedes"),
                ("superseded by", "superseded_by"),
            ] {
                if let Some(t) = r[key].as_str() {
                    println!("{label}: {t}");
                }
            }
            for key in ["relates_to", "depends_on", "refines"] {
                let targets = r[key].as_array().cloned().unwrap_or_default();
                if !targets.is_empty() {
                    let list: Vec<String> = targets.iter().map(&s).collect();
                    println!("{key}: {}", list.join(", "));
                }
            }
            let body = s(&r["body"]);
            if !body.is_empty() {
                println!("\n{body}");
            }
        }
        Command::Lint(_) | Command::Check => {
            for f in r["problems"]
                .as_array()
                .or_else(|| r["findings"].as_array())
                .into_iter()
                .flatten()
            {
                let subject = f["subject"]
                    .as_str()
                    .map(|x| format!("{x}: "))
                    .unwrap_or_default();
                println!("{}: {subject}{}", s(&f["severity"]), s(&f["message"]));
            }
            let (e, w) = (
                r["errors"].as_u64().unwrap_or(0),
                r["warnings"].as_u64().unwrap_or(0),
            );
            if e == 0 && w == 0 {
                println!("clean");
            } else {
                println!("{e} error(s), {w} warning(s)");
            }
        }
        Command::SetStatus(_) => {
            if r["changed"].as_bool().unwrap_or(true) {
                println!(
                    "{}: {} → {}",
                    s(&r["reference"]),
                    s(&r["from"]),
                    s(&r["to"])
                );
            } else {
                println!("{}: already {}", s(&r["reference"]), s(&r["to"]));
            }
        }
        Command::SetReview(_) => match r["review_by"].as_str() {
            Some(d) => println!("{}: review by {d}", s(&r["reference"])),
            None => println!("{}: review deadline cleared", s(&r["reference"])),
        },
        Command::Supersede(_) => {
            println!(
                "{} supersedes {} — {} is now superseded",
                s(&r["new"]["reference"]),
                s(&r["old"]["reference"]),
                s(&r["old"]["reference"]),
            );
        }
        Command::Plan(_) => {
            println!("{}", s(&r["plan"]));
        }
        Command::Mcp => {}
    }
}

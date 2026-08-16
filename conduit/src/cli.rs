//! CLI surface: the work-item doors, defined once in [`crate::surface`]
//! and dispatched here. The terminal IS the human seat (`Actor::HumanSeat`
//! on every verb); `conduit mcp` serves the same surface to harness
//! sessions as `Actor::Harness`, minus `signoff`. All wiring lives here —
//! the binary's `main.rs` is clap marshalling only.

use clap::{Parser, Subcommand, ValueEnum};

/// The merge-door gate deadline when `CONDUIT_GATE_TIMEOUT_SECS` doesn't
/// override it.
const DEFAULT_GATE_TIMEOUT_SECS: u64 = 1800;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "conduit",
    version,
    about = "Harness-first execution store for the Adopt stage: work items in the KB, humans gating intent, a mechanical merge door"
)]
pub struct Cli {
    /// Output format for reports.
    #[arg(
        short = 'o',
        long = "output",
        global = true,
        value_enum,
        default_value = "human"
    )]
    pub output: OutputFormat,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a draft work item (project/story/task) — the body is the
    /// contract: a goal at altitude plus its verification form.
    New(crate::surface::NewParams),
    /// The work tree as rows with seal states.
    List(crate::surface::ListParams),
    /// One work item in full — frontmatter, body, seal state, children.
    Show(crate::surface::ShowParams),
    /// Sign off an item at the human seat: seal the body, item goes ready.
    /// Sign-off flows downhill — the parent must be signed first.
    Signoff(crate::surface::SignoffParams),
    /// Reopen a signed item: strip the seal, back to draft, cascading
    /// downhill. Only a human seat can re-sign afterwards.
    Bounce(crate::surface::IdParams),
    /// Claim a ready task: seal verified, internal repo + branch
    /// provisioned, task goes in-progress.
    Claim(crate::surface::IdParams),
    /// The mechanical merge door: seal intact + gate green on the branch ->
    /// one squash commit on main, telemetry written, task done.
    Complete(crate::surface::IdParams),
    /// Close a story/project whose children are all terminal (projects
    /// close at the human seat).
    Close(crate::surface::IdParams),
    /// Cancel an item and every non-terminal descendant.
    Cancel(crate::surface::IdParams),
    /// Serve the work-item surface to harness sessions over MCP stdio
    /// (signoff is absent through that door by design).
    Mcp,
}

/// Entry point: one tokio runtime per invocation, one KB session per verb,
/// `Actor::HumanSeat` — the terminal is the human seat.
pub fn dispatch(cli: Cli) -> anyhow::Result<()> {
    use crate::surface as s;
    use crate::workitem::Actor;
    let cwd = std::env::current_dir()?;
    como_kb_client::load_env();
    let gate_timeout = gate_timeout();
    let rt = tokio::runtime::Runtime::new()?;
    let report = rt.block_on(async {
        if let Command::Mcp = cli.command {
            crate::mcp::serve(cwd.clone(), gate_timeout).await?;
            return anyhow::Ok(None);
        }
        let store = crate::work::KbWorkStore::connect().await?;
        let out = match &cli.command {
            Command::New(p) => s::new_core(&store, p).await,
            Command::List(p) => s::list_core(&store, p).await,
            Command::Show(p) => s::show_core(&store, p).await,
            Command::Signoff(p) => s::signoff_core(&store, Actor::HumanSeat, p).await,
            Command::Bounce(p) => s::bounce_core(&store, Actor::HumanSeat, p).await,
            Command::Claim(p) => s::claim_core(&store, &cwd, Actor::HumanSeat, p).await,
            Command::Complete(p) => s::complete_core(&store, &cwd, gate_timeout, p).await,
            Command::Close(p) => s::close_core(&store, Actor::HumanSeat, p).await,
            Command::Cancel(p) => s::cancel_core(&store, Actor::HumanSeat, p).await,
            Command::Mcp => unreachable!("routed above"),
        };
        store.close().await.ok();
        out.map(Some)
    })?;
    if let Some(report) = report {
        match cli.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
            OutputFormat::Human => print_report(&cli.command, &report),
        }
    }
    Ok(())
}

/// `CONDUIT_GATE_TIMEOUT_SECS` env override, else the default.
fn gate_timeout() -> std::time::Duration {
    let secs = std::env::var("CONDUIT_GATE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_GATE_TIMEOUT_SECS);
    std::time::Duration::from_secs(secs)
}

/// Terse human lines (the `-o json` report is the automation door).
fn print_report(command: &Command, report: &serde_json::Value) {
    let s = |k: &str| report[k].as_str().unwrap_or("?").to_string();
    match command {
        Command::New(_) => println!("created {} ({}, draft)", s("slug"), s("class")),
        Command::List(_) => {
            let empty = vec![];
            let items = report["items"].as_array().unwrap_or(&empty);
            if items.is_empty() {
                println!("no work items");
            }
            for i in items {
                println!(
                    "{:<12} {:<44} {:<12} seal:{:<9} {}",
                    i["class"].as_str().unwrap_or("?"),
                    i["slug"].as_str().unwrap_or("?"),
                    i["status"].as_str().unwrap_or("?"),
                    i["seal"].as_str().unwrap_or("?"),
                    i["title"].as_str().unwrap_or(""),
                );
            }
        }
        Command::Show(_) => println!(
            "{} ({}, {}, seal:{})\n\n{}",
            s("slug"),
            s("class"),
            s("status"),
            s("seal"),
            s("body")
        ),
        Command::Signoff(_) => println!(
            "signed off {} by {} — ready (body sha256 {})",
            s("slug"),
            s("by"),
            s("content_sha256")
        ),
        Command::Bounce(_) => println!("bounced to draft: {}", join(&report["bounced"])),
        Command::Claim(_) => println!(
            "claimed {} — branch {} in {}\n  {}",
            s("slug"),
            s("branch"),
            s("repo"),
            s("hint")
        ),
        Command::Complete(_) => println!(
            "merged {} — commit {} (gate: {}, {} ms of work)",
            s("slug"),
            s("merge_commit"),
            s("gate"),
            report["work_ms"].as_u64().unwrap_or(0)
        ),
        Command::Close(_) => println!("closed {}", s("slug")),
        Command::Cancel(_) => println!("cancelled: {}", join(&report["cancelled"])),
        Command::Mcp => {}
    }
}

fn join(v: &serde_json::Value) -> String {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

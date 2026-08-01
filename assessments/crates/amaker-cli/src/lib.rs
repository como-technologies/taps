//! Headless CLI for amaker — `author`, `export`, `validate`, `schema`.
//!
//! `export`, `validate`, and `schema` never construct an AI provider, so they
//! need no API key — `export` only needs `DATA_DIR` (default `./data`).
//! `author` builds a complete assessment from a brief via the configured
//! provider; `AI_PROVIDER=ollama` runs fully local with no key.
//!
//! `export` serializes through `ExportService::to_data` — the same path the
//! web route uses — so the two are byte-identical by construction; `export`
//! writes the content exactly as produced.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};

use amaker_core::config;
use amaker_core::models::{Assessment, ProjectId};
use amaker_core::services::{DataFormat, ExportService, StorageService};

/// AI-assisted assessment authoring: web app + headless CLI.
#[derive(Parser, Debug)]
#[command(name = "amaker", version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Output format for read subcommands, mirroring adroit's `-o` convention.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// One human-readable summary line
    Human,
    /// A machine-readable JSON summary
    Json,
}

/// Subcommands of the `amaker` binary.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Export a project's assessment (reads DATA_DIR, default ./data)
    Export {
        /// Project ID (UUID) under DATA_DIR/projects/
        project_id: ProjectId,
        /// Output format: yaml, json, or toml
        #[arg(long, default_value = "yaml", value_parser = parse_data_format)]
        format: DataFormat,
        /// Output file (stdout when omitted)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Validate an assessment file against the JSON Schema
    Validate {
        /// Assessment file; format inferred from extension (.yaml/.yml/.json/.toml)
        file: PathBuf,
        /// Output format: `human` (default) or `json`
        #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    /// Write the assessment JSON Schema
    Schema {
        /// Output file (stdout when omitted)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Author a complete assessment from a brief, headlessly (needs an AI
    /// provider: AI_PROVIDER=ollama runs fully local)
    Author {
        /// The brief: a text/markdown file describing what to assess
        #[arg(long)]
        brief: PathBuf,
        /// Supporting context files whose text is wired into every
        /// generation prompt (repeatable)
        #[arg(long = "context")]
        context: Vec<PathBuf>,
        /// Output assessment file (YAML)
        #[arg(long)]
        out: PathBuf,
        /// Concurrent question-generation lanes (1 = serial). Each lane
        /// multiplies the ollama server's KV-cache memory at num_ctx=8192,
        /// and a real speedup also needs OLLAMA_NUM_PARALLEL >= N
        /// server-side — see the book's Headless Authoring page
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=8))]
        jobs: u8,
    },
}

/// Dispatch a parsed [`Cli`] invocation.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Export {
            project_id,
            format,
            out,
        } => {
            dotenvy::dotenv().ok();
            export_cmd(&data_dir_from_env(), project_id, format, out.as_deref()).await
        }
        Command::Validate { file, output } => {
            let assessment = validate_cmd(&file)?;
            match output {
                OutputFormat::Human => println!(
                    "valid: '{}' — {} domains, {} practices, {} questions",
                    assessment.name,
                    assessment.domain_count(),
                    assessment.practice_count(),
                    assessment.question_count()
                ),
                OutputFormat::Json => println!("{}", validate_summary_json(&assessment)?),
            }
            Ok(())
        }
        Command::Schema { out } => schema_cmd(out.as_deref()),
        Command::Author {
            brief,
            context,
            out,
            jobs,
        } => {
            dotenvy::dotenv().ok();
            let config = config::Config::from_env()?;
            let provider = amaker_core::services::build_provider(&config)?;
            author_cmd(provider.as_ref(), &brief, &context, &out, jobs as usize).await
        }
    }
}

/// `author`: headlessly author an assessment from a brief file.
///
/// Reads the brief and any `--context` files up front (failing fast, before
/// any model call), streams progress to stderr with elapsed time, and writes
/// the schema-validated YAML to `out` only on success. `jobs` bounds the
/// concurrent question-generation lanes (1 = serial).
pub async fn author_cmd(
    llm: &dyn amaker_core::services::LlmProvider,
    brief: &Path,
    context_files: &[PathBuf],
    out: &Path,
    jobs: usize,
) -> anyhow::Result<()> {
    use amaker_core::services::{AuthorContext, Progress, author_assessment};

    let brief_text = std::fs::read_to_string(brief)
        .with_context(|| format!("cannot read brief {}", brief.display()))?;

    let mut docs: Vec<(String, String)> = Vec::new();
    for path in context_files {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read context file {}", path.display()))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("context")
            .to_string();
        docs.push((name, text));
    }
    let context = AuthorContext::from_documents(&docs);

    // Elapsed timing uses the monotonic clock (`Instant`), never wall-clock
    // `SystemTime`/`chrono`. On WSL2 the realtime clock can step under load
    // (the ~39s gap noted in iteration-3 run-3 was the operator's wall
    // stopwatch vs this already-monotonic number — this side was correct);
    // conduit's demo kit moved TO a monotonic source to match binaries like
    // this one. Keep elapsed measurement on `Instant` so it can't regress.
    let started = std::time::Instant::now();
    let mut on_progress = |event: Progress| {
        let t = started.elapsed().as_secs_f32();
        match event {
            Progress::Summary => eprintln!("[{t:6.1}s] scoping summary..."),
            Progress::Structure { attempt: 1 } => {
                eprintln!("[{t:6.1}s] generating structure...")
            }
            Progress::Structure { attempt } => {
                eprintln!("[{t:6.1}s] generating structure (attempt {attempt})...")
            }
            Progress::StructureDone {
                name,
                domains,
                practices,
            } => {
                eprintln!("[{t:6.1}s] structure '{name}': {domains} domains, {practices} practices")
            }
            Progress::Questions {
                practice,
                index,
                total,
                attempt,
            } => {
                let retry = if attempt == 1 {
                    String::new()
                } else {
                    format!(" (attempt {attempt})")
                };
                eprintln!("[{t:6.1}s] questions {index}/{total}: {practice}{retry}...")
            }
            Progress::QuestionsDone { count, .. } => {
                eprintln!("[{t:6.1}s]   -> {count} questions")
            }
            Progress::DuplicatePracticesDropped { dropped } => {
                eprintln!(
                    "[{t:6.1}s] warning: dropped duplicate practice(s) after \
                     retries: {}",
                    dropped.join(", ")
                )
            }
        }
    };

    let (assessment, yaml) =
        author_assessment(llm, &brief_text, context.as_ref(), jobs, &mut on_progress)
            .await
            .context(
                "authoring failed — the model could not produce usable output; \
                 try enriching the brief, adding --context files, or using a \
                 more capable model (e.g. a larger OLLAMA_MODEL)",
            )?;

    std::fs::write(out, &yaml).with_context(|| format!("cannot write {}", out.display()))?;
    eprintln!(
        "authored '{}' in {:.1}s — {} domains, {} practices, {} questions -> {}",
        assessment.name,
        started.elapsed().as_secs_f32(),
        assessment.domain_count(),
        assessment.practice_count(),
        assessment.question_count(),
        out.display()
    );
    Ok(())
}

/// `DATA_DIR` with the same default the web app's `Config` uses.
fn data_dir_from_env() -> PathBuf {
    PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()))
}

/// Build a filesystem-backed `StorageService` rooted at `dir` (the CLI's
/// `DATA_DIR`). Local-store construction is effectively infallible.
fn fs_storage(dir: impl AsRef<Path>) -> StorageService {
    let store = amaker_core::build_store(&amaker_core::filesystem(dir))
        .expect("build local filesystem store");
    StorageService::new(store)
}

/// Load a stored project's assessment and serialize it — the CLI half of the
/// shared export path (the web route uses `ExportService::to_data` too, so the
/// output is byte-identical by construction).
async fn export_project(
    storage: &StorageService,
    project_id: ProjectId,
    format: DataFormat,
) -> anyhow::Result<(amaker_core::models::Project, String)> {
    let project = storage.load_project(project_id).await?;
    let yaml = storage
        .load_assessment_yaml(project_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("project '{}' has no assessment to export", project.name))?;
    let assessment = ExportService::validate_and_parse(&yaml, DataFormat::Yaml)?;
    let content = ExportService::to_data(&assessment, format)?;
    Ok((project, content))
}

/// The generated JSON Schema, pretty-printed and newline-terminated.
fn json_schema_pretty() -> String {
    let mut s =
        serde_json::to_string_pretty(&ExportService::json_schema()).expect("serialize JSON schema");
    s.push('\n');
    s
}

/// Parse a `DataFormat` for clap (origin's `DataFormat` isn't a `ValueEnum`).
fn parse_data_format(s: &str) -> Result<DataFormat, String> {
    DataFormat::from_str(s).ok_or_else(|| format!("expected one of yaml|json|toml, got '{s}'"))
}

/// `export`: serialize a stored project's assessment via the shared
/// export path.
pub async fn export_cmd(
    data_dir: &Path,
    project_id: ProjectId,
    format: DataFormat,
    out: Option<&Path>,
) -> anyhow::Result<()> {
    let storage = fs_storage(data_dir);
    let (project, content) = export_project(&storage, project_id, format).await?;
    write_output(out, &content)?;
    if let Some(path) = out {
        eprintln!(
            "exported '{}' as {} -> {}",
            project.name,
            format.extension(),
            path.display()
        );
    }
    Ok(())
}

/// `validate`: schema-validate and parse an assessment file, then apply the
/// degeneracy gate — a schema-valid document whose load-bearing fields echo
/// known prompt-scaffold placeholders (run-1's assessment was literally
/// named "Assessment Name") is rejected with a nonzero exit.
pub fn validate_cmd(file: &Path) -> anyhow::Result<Assessment> {
    let raw_ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let ext = if raw_ext.eq_ignore_ascii_case("yml") {
        "yaml"
    } else {
        raw_ext
    };
    let format = DataFormat::from_str(ext).with_context(|| {
        format!(
            "cannot infer format of {} (expected .yaml/.yml/.json/.toml)",
            file.display()
        )
    })?;
    let content =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let assessment = ExportService::validate_and_parse(&content, format)
        .with_context(|| format!("{} is not a valid assessment", file.display()))?;
    let degenerate = amaker_core::services::quality::degenerate_fields(&assessment);
    if !degenerate.is_empty() {
        anyhow::bail!(
            "{} is degenerate — load-bearing fields echo prompt placeholders \
             instead of content: {}",
            file.display(),
            degenerate.join("; ")
        );
    }
    Ok(assessment)
}

/// `validate -o json`: machine summary of a validated assessment, for
/// scripting the Assess→Prescribe seam. Each `practices[]` entry's
/// `name`/`domain` matches the `title`/`domain` of the seed that
/// `adroit import --from-assessment ... -o json` reports for it, so a
/// script can join the two and assert every practice produced a seed
/// (`just seam-check` does exactly that).
pub fn validate_summary_json(assessment: &Assessment) -> anyhow::Result<String> {
    use serde_json::json;
    let practices: Vec<_> = assessment
        .domains
        .iter()
        .flat_map(|d| {
            d.practices.iter().map(|p| {
                json!({
                    "name": p.name,
                    "domain": d.name,
                    "questions": p.questions.len(),
                })
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&json!({
        "valid": true,
        "name": assessment.name,
        "domains": assessment.domain_count(),
        "practices": practices,
        "questions": assessment.question_count(),
    }))?)
}

/// `schema`: write the generated JSON Schema (pretty, newline-terminated).
pub fn schema_cmd(out: Option<&Path>) -> anyhow::Result<()> {
    write_output(out, &json_schema_pretty())
}

/// Write `content` exactly — byte-for-byte — to the file or stdout.
fn write_output(out: Option<&Path>, content: &str) -> anyhow::Result<()> {
    use std::io::Write;
    match out {
        Some(path) => std::fs::write(path, content)
            .with_context(|| format!("cannot write {}", path.display()))?,
        None => std::io::stdout().write_all(content.as_bytes())?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn export_parses_project_id_format_and_out() {
        let cli = Cli::try_parse_from([
            "amaker",
            "export",
            "550e8400-e29b-41d4-a716-446655440000",
            "--format",
            "json",
            "--out",
            "out.json",
        ])
        .unwrap();
        let Command::Export {
            project_id,
            format,
            out,
        } = cli.command
        else {
            panic!("expected export command");
        };
        assert_eq!(
            project_id.to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(format, DataFormat::Json);
        assert_eq!(out.as_deref(), Some(std::path::Path::new("out.json")));
    }

    #[test]
    fn export_format_defaults_to_yaml_and_out_to_stdout() {
        let cli = Cli::try_parse_from(["amaker", "export", "550e8400-e29b-41d4-a716-446655440000"])
            .unwrap();
        let Command::Export { format, out, .. } = cli.command else {
            panic!("expected export command");
        };
        assert_eq!(format, DataFormat::Yaml);
        assert_eq!(out, None);
    }

    #[test]
    fn export_rejects_unknown_format_and_bad_project_id() {
        assert!(
            Cli::try_parse_from([
                "amaker",
                "export",
                "550e8400-e29b-41d4-a716-446655440000",
                "--format",
                "xml",
            ])
            .is_err()
        );
        assert!(Cli::try_parse_from(["amaker", "export", "not-a-uuid"]).is_err());
    }

    #[test]
    fn validate_takes_a_file_and_defaults_to_human_output() {
        let cli = Cli::try_parse_from(["amaker", "validate", "assessment.yaml"]).unwrap();
        let Command::Validate { file, output } = cli.command else {
            panic!("expected validate command");
        };
        assert_eq!(file, std::path::PathBuf::from("assessment.yaml"));
        assert_eq!(output, OutputFormat::Human);
    }

    #[test]
    fn validate_accepts_output_json_and_rejects_unknown_formats() {
        for flag in [["-o", "json"], ["--output", "json"]] {
            let cli =
                Cli::try_parse_from(["amaker", "validate", "a.yaml", flag[0], flag[1]]).unwrap();
            let Command::Validate { output, .. } = cli.command else {
                panic!("expected validate command");
            };
            assert_eq!(output, OutputFormat::Json);
        }
        assert!(Cli::try_parse_from(["amaker", "validate", "a.yaml", "-o", "xml"]).is_err());
    }

    #[test]
    fn schema_out_is_optional() {
        let cli = Cli::try_parse_from(["amaker", "schema"]).unwrap();
        let Command::Schema { out } = cli.command else {
            panic!("expected schema command");
        };
        assert_eq!(out, None);

        let cli = Cli::try_parse_from(["amaker", "schema", "--out", "schema.json"]).unwrap();
        let Command::Schema { out } = cli.command else {
            panic!("expected schema command");
        };
        assert_eq!(out, Some(std::path::PathBuf::from("schema.json")));
    }

    // ===== Behavior of the headless commands =====

    use amaker_core::models::Project;

    /// Seed a project with an assessment into `data_dir`; returns its ID.
    ///
    /// The stored YAML carries explicit ids, as files written by the real
    /// app always do — ids omitted on parse are minted fresh, so an id-less
    /// stored file would export differently on every read.
    async fn seed_project(data_dir: &std::path::Path) -> amaker_core::models::ProjectId {
        let storage = fs_storage(data_dir);
        storage.init().await.unwrap();
        let project = Project::new("CLI Test".to_string(), None);
        storage.save_project(&project).await.unwrap();
        let yaml = "\
id: 11111111-1111-4111-8111-111111111111
name: CLI Test
description: d
goal: g
created_at: 2026-01-01T00:00:00Z
updated_at: 2026-01-01T00:00:00Z
domains:
- id: 22222222-2222-4222-8222-222222222222
  name: D
  context: c
  value: v
  risk: r
  practices:
  - id: 33333333-3333-4333-8333-333333333333
    name: P
    context: c
    value: v
    risk: r
    questions:
    - id: 44444444-4444-4444-8444-444444444444
      text: Q?
      polarity: positive
";
        storage
            .save_assessment_yaml(project.id, yaml)
            .await
            .unwrap();
        project.id
    }

    #[tokio::test]
    async fn export_cmd_writes_the_exact_export_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let id = seed_project(dir.path()).await;
        let out = dir.path().join("export.json");

        export_cmd(dir.path(), id, DataFormat::Json, Some(&out))
            .await
            .unwrap();

        let storage = fs_storage(dir.path());
        let (_, expected) = export_project(&storage, id, DataFormat::Json)
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), expected);
    }

    #[tokio::test]
    async fn export_cmd_fails_on_unknown_project() {
        let dir = tempfile::tempdir().unwrap();
        let storage = fs_storage(dir.path());
        storage.init().await.unwrap();

        let result = export_cmd(
            dir.path(),
            amaker_core::models::ProjectId::new(),
            DataFormat::Yaml,
            None,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn validate_cmd_accepts_a_valid_export_in_each_format() {
        let dir = tempfile::tempdir().unwrap();
        let id = seed_project(dir.path()).await;

        for format in [DataFormat::Yaml, DataFormat::Json, DataFormat::Toml] {
            let out = dir.path().join(format!("export.{}", format.extension()));
            export_cmd(dir.path(), id, format, Some(&out))
                .await
                .unwrap();
            let assessment = validate_cmd(&out).unwrap();
            assert_eq!(assessment.name, "CLI Test");
        }
    }

    #[tokio::test]
    async fn validate_summary_json_lists_every_practice_with_its_domain() {
        let dir = tempfile::tempdir().unwrap();
        let id = seed_project(dir.path()).await;
        let out = dir.path().join("export.yaml");
        export_cmd(dir.path(), id, DataFormat::Yaml, Some(&out))
            .await
            .unwrap();

        let assessment = validate_cmd(&out).unwrap();
        let summary: serde_json::Value =
            serde_json::from_str(&validate_summary_json(&assessment).unwrap()).unwrap();

        assert_eq!(summary["valid"], serde_json::json!(true));
        assert_eq!(summary["name"], serde_json::json!("CLI Test"));
        assert_eq!(summary["domains"], serde_json::json!(1));
        assert_eq!(summary["questions"], serde_json::json!(1));
        // One entry per practice; `name`/`domain` are the join keys against
        // the `title`/`domain` of adroit's import seed summary.
        assert_eq!(
            summary["practices"],
            serde_json::json!([{"name": "P", "domain": "D", "questions": 1}])
        );
    }

    /// The degeneracy gate: a schema-valid document whose load-bearing
    /// fields echo the prompt scaffold must fail `validate`. The fields here
    /// are verbatim from the iteration-1 run-1 capstone artifact, which
    /// validated cleanly at the time — that is the bug.
    #[test]
    fn validate_cmd_rejects_the_run1_placeholder_echo_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("run1.yaml");
        std::fs::write(
            &file,
            "\
name: Assessment Name
description: What this assessment evaluates
goal: Intended outcome
domains:
- name: Delivery Pipeline
  context: How well does the team manage its delivery pipeline?
  value: Faster time-to-market and reduced risk
  risk: Delayed releases and increased risk of errors
  practices:
  - name: Version Control Discipline
    context: How well does the team use version control?
    value: Improved collaboration and reduced risk of errors
    risk: Lost changes and increased risk of errors
    questions:
    - text: Does your team use version control to manage code changes?
      polarity: positive
",
        )
        .unwrap();

        let err = validate_cmd(&file).unwrap_err();
        let chain = format!("{err:#}");
        assert!(
            chain.contains("Assessment Name"),
            "error must name the placeholder echo: {chain}"
        );
        assert!(
            chain.contains("degenerate"),
            "error must say what kind of failure this is: {chain}"
        );
    }

    /// The degeneracy gate covers question text (fault-injection finding):
    /// a schema-valid document whose question is a bracket placeholder must
    /// fail `validate` just like a placeholder-echo name.
    #[test]
    fn validate_cmd_rejects_bracket_placeholder_question_text() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("placeholders.yaml");
        std::fs::write(
            &file,
            "\
name: Release Readiness
description: How ready releases are
goal: Find release risks
domains:
- name: Delivery
  context: How code ships
  value: Predictable releases
  risk: Outages
  practices:
  - name: Continuous Integration
    context: Merging and verifying changes
    value: Fast feedback
    risk: Broken main
    questions:
    - text: '[Insert question about the release process]'
      polarity: positive
",
        )
        .unwrap();

        let err = validate_cmd(&file).unwrap_err();
        let chain = format!("{err:#}");
        assert!(chain.contains("degenerate"), "{chain}");
        assert!(
            chain.contains("[Insert question"),
            "error must name the placeholder question: {chain}"
        );
    }

    #[test]
    fn validate_cmd_rejects_invalid_documents_and_unknown_extensions() {
        let dir = tempfile::tempdir().unwrap();

        let bad = dir.path().join("bad.yaml");
        std::fs::write(&bad, "name: only-a-name\n").unwrap();
        assert!(validate_cmd(&bad).is_err(), "schema-invalid must fail");

        let txt = dir.path().join("export.txt");
        std::fs::write(&txt, "whatever").unwrap();
        assert!(validate_cmd(&txt).is_err(), "unknown extension must fail");

        let missing = dir.path().join("nope.yaml");
        assert!(validate_cmd(&missing).is_err(), "missing file must fail");
    }

    // ===== author =====

    use amaker_core::services::provider::fake::FakeProvider;

    #[test]
    fn author_parses_brief_repeated_context_and_out() {
        let cli = Cli::try_parse_from([
            "amaker",
            "author",
            "--brief",
            "brief.md",
            "--context",
            "notes.md",
            "--context",
            "more.md",
            "--out",
            "assessment.yaml",
        ])
        .unwrap();
        let Command::Author {
            brief,
            context,
            out,
            jobs,
        } = cli.command
        else {
            panic!("expected author command");
        };
        assert_eq!(brief, PathBuf::from("brief.md"));
        assert_eq!(
            context,
            vec![PathBuf::from("notes.md"), PathBuf::from("more.md")]
        );
        assert_eq!(out, PathBuf::from("assessment.yaml"));
        assert_eq!(jobs, 1, "--jobs defaults to serial");
    }

    #[test]
    fn author_jobs_parses_and_is_bounded() {
        let cli = Cli::try_parse_from([
            "amaker", "author", "--brief", "b.md", "--out", "a.yaml", "--jobs", "2",
        ])
        .unwrap();
        let Command::Author { jobs, .. } = cli.command else {
            panic!("expected author command");
        };
        assert_eq!(jobs, 2);

        // Bounded: 0 lanes is meaningless, and each lane multiplies the
        // ollama KV cache at num_ctx=8192, so the ceiling is hard.
        for bad in ["0", "9", "-1", "two"] {
            assert!(
                Cli::try_parse_from([
                    "amaker", "author", "--brief", "b.md", "--out", "a.yaml", "--jobs", bad,
                ])
                .is_err(),
                "--jobs {bad} must be rejected"
            );
        }
    }

    #[test]
    fn author_requires_brief_and_out_but_not_context() {
        assert!(Cli::try_parse_from(["amaker", "author", "--out", "a.yaml"]).is_err());
        assert!(Cli::try_parse_from(["amaker", "author", "--brief", "b.md"]).is_err());
        let cli = Cli::try_parse_from(["amaker", "author", "--brief", "b.md", "--out", "a.yaml"])
            .unwrap();
        let Command::Author { context, .. } = cli.command else {
            panic!("expected author command");
        };
        assert!(context.is_empty());
    }

    /// Scripted happy-path responses: structure (1 domain, 1 practice) then
    /// questions for that practice.
    fn author_script() -> Vec<amaker_core::services::provider::ChatResponse> {
        vec![
            FakeProvider::text("a scoping summary"),
            FakeProvider::text(
                "```yaml\nname: A\ndescription: d\ngoal: g\ndomains:\n\
                 - name: D\n  context: c\n  value: v\n  risk: r\n  practices:\n\
                 \x20 - name: P\n    context: c\n    value: v\n    risk: r\n    questions: []\n```",
            ),
            FakeProvider::text(
                "```yaml\nquestions:\n  - text: \"Q?\"\n    polarity: positive\n```",
            ),
        ]
    }

    #[tokio::test]
    async fn author_cmd_writes_a_validated_assessment_and_wires_context_files() {
        let dir = tempfile::tempdir().unwrap();
        let brief = dir.path().join("brief.md");
        std::fs::write(&brief, "BRIEF-TEXT").unwrap();
        let notes = dir.path().join("notes.md");
        std::fs::write(&notes, "NOTES-TEXT").unwrap();
        let out = dir.path().join("assessment.yaml");
        let fake = FakeProvider::new(author_script());

        author_cmd(&fake, &brief, &[notes], &out, 1).await.unwrap();

        // The written file passes the same validation `validate` applies.
        let assessment = validate_cmd(&out).unwrap();
        assert_eq!(assessment.name, "A");
        assert_eq!(assessment.question_count(), 1);

        // The brief and the context file text (with its name) reached the
        // prompts — the headless path has no dead uploads.
        let calls = fake.calls();
        let summary_msg = &calls[0].messages[0].1;
        assert!(summary_msg.contains("BRIEF-TEXT"));
        assert!(summary_msg.contains("NOTES-TEXT"));
        assert!(summary_msg.contains("notes.md"));
        let structure_msg = &calls[1].messages[0].1;
        assert!(structure_msg.contains("NOTES-TEXT"));
        let questions_msg = &calls[2].messages[0].1;
        assert!(questions_msg.contains("NOTES-TEXT"));
    }

    /// The leakage gate end to end through the CLI: banned tokens are
    /// derived from the `--context` files themselves (filename + snake_case
    /// JSON keys), a leaky generation is retried with feedback, and the
    /// written file carries no banned token.
    #[tokio::test]
    async fn author_cmd_derives_leak_tokens_from_json_context_and_retries() {
        let dir = tempfile::tempdir().unwrap();
        let brief = dir.path().join("brief.md");
        std::fs::write(&brief, "BRIEF").unwrap();
        let report = dir.path().join("pulse-report.json");
        std::fs::write(&report, r#"{"per_tenant": [{"total_flows": 10}]}"#).unwrap();
        let out = dir.path().join("assessment.yaml");

        // Splice a leaky first questions attempt into the happy-path script.
        let mut script = author_script();
        script.insert(
            2,
            FakeProvider::text(
                "```yaml\nquestions:\n  - text: \"Q?\"\n    polarity: positive\n    \
                 guidance: \"see pulse-report.json under per_tenant\"\n```",
            ),
        );
        let fake = FakeProvider::new(script);

        author_cmd(&fake, &brief, &[report], &out, 1).await.unwrap();

        let yaml = std::fs::read_to_string(&out).unwrap();
        assert!(!yaml.contains("per_tenant"), "leak in output: {yaml}");
        assert!(
            !yaml.contains("pulse-report.json"),
            "leak in output: {yaml}"
        );

        let calls = fake.calls();
        assert_eq!(calls.len(), 4, "summary, structure, leaky attempt, retry");
        let retry_msg = &calls[3].messages[0].1;
        assert!(
            retry_msg.contains("per_tenant") && retry_msg.contains("pulse-report.json"),
            "feedback must name the leaked tokens: {retry_msg}"
        );
        // The background framing reaches prompts on the CLI path too.
        assert!(calls[0].messages[0].1.contains("Background Signal"));
    }

    #[tokio::test]
    async fn author_cmd_fails_before_any_provider_call_when_brief_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeProvider::new(author_script());

        let err = author_cmd(
            &fake,
            &dir.path().join("nope.md"),
            &[],
            &dir.path().join("a.yaml"),
            1,
        )
        .await
        .unwrap_err();

        assert!(
            fake.calls().is_empty(),
            "must fail before calling the model"
        );
        assert!(format!("{err:#}").contains("nope.md"));
    }

    #[tokio::test]
    async fn author_cmd_fails_when_a_context_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let brief = dir.path().join("brief.md");
        std::fs::write(&brief, "b").unwrap();
        let fake = FakeProvider::new(author_script());

        let err = author_cmd(
            &fake,
            &brief,
            &[dir.path().join("missing-notes.md")],
            &dir.path().join("a.yaml"),
            1,
        )
        .await
        .unwrap_err();

        assert!(fake.calls().is_empty());
        assert!(format!("{err:#}").contains("missing-notes.md"));
    }

    #[tokio::test]
    async fn author_cmd_exhausted_retries_leave_no_output_and_explain() {
        let dir = tempfile::tempdir().unwrap();
        let brief = dir.path().join("brief.md");
        std::fs::write(&brief, "b").unwrap();
        let out = dir.path().join("a.yaml");
        let fake = FakeProvider::new(vec![
            FakeProvider::text("summary"),
            FakeProvider::text("junk"),
            FakeProvider::text("junk"),
            FakeProvider::text("junk"),
        ]);

        let err = author_cmd(&fake, &brief, &[], &out, 1).await.unwrap_err();

        assert!(!out.exists(), "no output file on failure");
        let chain = format!("{err:#}");
        assert!(chain.contains("after 3 attempts"), "{chain}");
        assert!(
            chain.contains("authoring failed"),
            "stderr must be actionable: {chain}"
        );
    }

    #[test]
    fn schema_cmd_writes_the_pretty_schema() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("schema.json");
        schema_cmd(Some(&out)).unwrap();
        let written = std::fs::read_to_string(&out).unwrap();
        // Pretty (indented), newline-terminated, valid JSON. Byte-equality
        // can't be asserted — the generated schema embeds freshly-minted UUID
        // `default`s, so two generations differ.
        assert!(
            written.ends_with("}\n"),
            "schema must be newline-terminated"
        );
        assert!(written.contains("\n  "), "schema must be pretty-printed");
        let schema: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(schema["title"], serde_json::json!("Assessment"));
        assert!(schema["$defs"]["Question"].is_object());
    }
}

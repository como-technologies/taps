//! The `mcp` server's tool projection (manifest → MCP tools) and execution.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value, json};

use crate::config::Config;
use crate::manifest::{self, OptionInfo};

/// One verb exposed as an MCP tool.
struct Tool {
    name: String,
    description: Option<String>,
    args: Vec<OptionInfo>,
    /// argv appended to every invocation (ADR-0021: `--no-edit` on the write
    /// slice — an MCP subprocess has no TTY to hand an editor).
    forced: &'static [&'static str],
    /// This tool can mutate the corpus (a write-slice verb, or a read verb
    /// with re-admitted write escalations like `plan --save`). Drives the
    /// `readOnlyHint` / `destructiveHint` annotations.
    destructive: bool,
}

/// The running MCP server: the projected read-verb tools + how to run them.
pub struct Server {
    tools: Vec<Tool>,
    /// On-disk shape forwarded to every tool subprocess so it reads the same
    /// profile this `mcp` invocation resolved (regardless of flag/env/config).
    env: Vec<(&'static str, String)>,
    /// The adroit binary to re-invoke for a `tools/call`.
    exe: PathBuf,
}

impl Server {
    /// Project the manifest's read verbs as tools and capture how to run them.
    /// Read-only — ADR-0015's categorical property is the default and needs no
    /// flag audit.
    pub fn new(cfg: &Config, dir: &Path) -> Self {
        Self::build_with(cfg, dir, false)
    }

    /// The `--allow-write` server (ADR-0021): the read projection plus the
    /// guarded write slice from `manifest::mcp_write_slice`, annotated
    /// destructive. Forge and file-output escalations stay stripped
    /// categorically; only declared corpus-write escalations named by a slice
    /// entry's `admit` list survive.
    pub fn allow_write(cfg: &Config, dir: &Path) -> Self {
        Self::build_with(cfg, dir, true)
    }

    fn build_with(cfg: &Config, dir: &Path, writes: bool) -> Self {
        let tools = manifest::build()
            .commands()
            .iter()
            .filter_map(|c| {
                let spec = if writes {
                    manifest::mcp_write_slice(&c.name)
                } else {
                    None
                };
                let write_tool = c.writes && spec.is_some();
                if !c.is_read_tool() && !write_tool {
                    return None;
                }
                let admit: &[&str] = spec.as_ref().map_or(&[], |s| s.admit);
                let deny: &[&str] = spec.as_ref().map_or(&[], |s| s.deny);
                // Strip every arg the manifest marks escalating (ADR-0006:
                // `review --forge` is a forge write, `--out` an arbitrary file
                // write), so the projected surface honors its annotations
                // flag-set included. Stripping the schema is sufficient:
                // `build_argv` ignores keys it doesn't project. In write mode
                // a slice entry may re-admit *declared corpus-write*
                // escalations (`plan --save`) — never forge / file-output —
                // and deny flags whose semantics need the CLI.
                let args: Vec<OptionInfo> = c
                    .args
                    .iter()
                    .filter(|a| {
                        !deny.contains(&a.name.as_str())
                            && match a.escalates {
                                None => true,
                                Some("writes") => admit.contains(&a.name.as_str()),
                                Some(_) => false,
                            }
                    })
                    .cloned()
                    .collect();
                Some(Tool {
                    name: c.name.clone(),
                    description: c.summary.clone(),
                    destructive: write_tool || args.iter().any(|a| a.escalates.is_some()),
                    forced: spec.as_ref().map_or(&[], |s| s.forced),
                    args,
                })
            })
            .collect();
        Self {
            tools,
            env: vec![
                ("ADROIT_DIR", dir.to_string_lossy().into_owned()),
                ("ADROIT_NAMING", cfg.naming.to_string()),
                ("ADROIT_DATE_SOURCE", cfg.date_source.to_string()),
            ],
            exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("adroit")),
        }
    }

    /// The `tools/list` array — each projected verb as a tool with its
    /// `inputSchema`. Write-slice tools (ADR-0021) announce themselves:
    /// `readOnlyHint: false, destructiveHint: true`, so a client's
    /// confirmation UX (the reopen criterion's other half) has the signal.
    pub(crate) fn tool_list(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": input_schema(&t.args),
                    "annotations": {
                        "readOnlyHint": !t.destructive,
                        "destructiveHint": t.destructive,
                    },
                })
            })
            .collect()
    }

    /// Handle a `tools/call`: look up the tool, build + run its CLI invocation, and
    /// return the JSON output as MCP text content. A verb that ran but failed (bad
    /// input, unmet requirement) becomes a tool error (`isError`), not a protocol
    /// error — so the model can read it.
    pub(crate) fn tools_call(&self, id: Value, msg: &Value) -> String {
        let params = msg.get("params");
        let name = params
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(tool) = self.tools.iter().find(|t| t.name == name) else {
            return super::error(id, -32602, &format!("unknown tool: {name}"));
        };
        let arguments = params.and_then(|p| p.get("arguments"));
        let argv = match build_argv(tool, arguments) {
            Ok(mut a) => {
                // The slice's forced argv (`--no-edit`) rides every call —
                // after the mapped args, before `-o json` (clap accepts flags
                // after positionals).
                a.extend(tool.forced.iter().map(|s| s.to_string()));
                a
            }
            Err(e) => return super::error(id, -32602, &e),
        };
        match self.run_verb(&tool.name, &argv) {
            Ok(out) => super::ok(
                id,
                json!({ "content": [ { "type": "text", "text": out } ] }),
            ),
            Err(e) => super::ok(
                id,
                json!({ "content": [ { "type": "text", "text": e } ], "isError": true }),
            ),
        }
    }

    /// Run `adroit <verb> <argv…> -o json` with the forwarded shape env. Returns
    /// stdout on success; the verb's stderr on a non-zero exit — except `check`,
    /// whose non-zero exit still carries a valid JSON report on stdout.
    fn run_verb(&self, verb: &str, argv: &[String]) -> Result<String, String> {
        let mut cmd = Command::new(&self.exe);
        cmd.arg(verb).args(argv).args(["-o", "json"]);
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        let output = cmd
            .output()
            .map_err(|e| format!("failed to run `{verb}`: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.success() || (verb == "check" && !stdout.trim().is_empty()) {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("`{verb}` failed: {}", stderr.trim()))
        }
    }
}

/// Map a tool call's `arguments` object onto a CLI argv using each arg's metadata
/// (positional vs option vs flag). Read verbs have at most one positional, so
/// ordering is unambiguous; options are emitted before the positional.
fn build_argv(tool: &Tool, arguments: Option<&Value>) -> Result<Vec<String>, String> {
    let empty = Map::new();
    let obj = match arguments {
        Some(Value::Object(o)) => o,
        None | Some(Value::Null) => &empty,
        Some(_) => return Err("`arguments` must be an object".to_string()),
    };
    let mut options = Vec::new();
    let mut positionals = Vec::new();
    for arg in &tool.args {
        let Some(val) = obj.get(&arg.name) else {
            continue;
        };
        if arg.flag {
            if val.as_bool() == Some(true)
                && let Some(long) = &arg.long
            {
                options.push(long.clone());
            }
        } else if arg.positional {
            positionals.push(value_to_string(val));
        } else if let Some(long) = &arg.long {
            options.push(long.clone());
            options.push(value_to_string(val));
        }
    }
    options.extend(positionals);
    Ok(options)
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A JSON Schema (`type: object`) for a verb's args — drives the MCP `inputSchema`.
fn input_schema(args: &[OptionInfo]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for arg in args {
        let mut prop = Map::new();
        if arg.flag {
            prop.insert("type".into(), json!("boolean"));
        } else {
            prop.insert("type".into(), json!("string"));
            if !arg.values.is_empty() {
                prop.insert("enum".into(), json!(arg.values));
            }
        }
        if let Some(help) = &arg.help {
            prop.insert("description".into(), json!(help));
        }
        if let Some(default) = &arg.default {
            prop.insert("default".into(), json!(default));
        }
        properties.insert(arg.name.clone(), Value::Object(prop));
        if arg.required {
            required.push(arg.name.clone());
        }
    }
    json!({ "type": "object", "properties": properties, "required": required })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        let tmp = tempfile::tempdir().unwrap();
        Server::new(&Config::default(), tmp.path())
    }

    fn write_server() -> Server {
        let tmp = tempfile::tempdir().unwrap();
        Server::allow_write(&Config::default(), tmp.path())
    }

    #[test]
    fn allow_write_projects_exactly_the_slice() {
        // ADR-0021: the write mode adds the slice's writers and nothing else —
        // interactive (`draft` / `edit`), bulk (`import` / `seed`), and
        // maintenance writers (`relink` / `renumber` / `index` / `publish` /
        // `link` / `set-review` / `supersede` / `config`) stay CLI-only.
        let names: Vec<String> = write_server()
            .tools
            .iter()
            .map(|t| t.name.clone())
            .collect();
        for added in ["new", "compose", "set-status"] {
            assert!(
                names.contains(&added.to_string()),
                "missing `{added}`: {names:?}"
            );
        }
        for absent in [
            "draft",
            "edit",
            "import",
            "seed",
            "link",
            "set-review",
            "supersede",
            "relink",
            "renumber",
            "index",
            "publish",
            "config",
        ] {
            assert!(
                !names.contains(&absent.to_string()),
                "write mode must not project `{absent}`: {names:?}"
            );
        }
    }

    #[test]
    fn write_tools_never_carry_forge_or_file_output_args() {
        // Categorical, both modes: no projected arg anywhere carries a forge
        // or file-output escalation, and the slice's denied control flags
        // (`--force`, `--interview`, forge apply/quorum, `--no-edit`) never
        // reach a schema. `plan` gains exactly its re-admitted save surface.
        let s = write_server();
        for tool in s.tool_list() {
            let name = tool["name"].as_str().unwrap();
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            for banned in [
                "forge",
                "yes",
                "out",
                "interview",
                "no_edit",
                "force",
                "regenerate",
            ] {
                assert!(
                    !props.contains_key(banned),
                    "write-mode tool `{name}` leaks `{banned}`: {tool}"
                );
            }
            if name == "plan" {
                assert!(props.contains_key("save"), "plan re-admits --save: {tool}");
                assert!(
                    props.contains_key("dry_run"),
                    "plan re-admits --dry-run: {tool}"
                );
            }
            // `--quorum` parameterizes `accepted --forge` — forge-flavored on
            // the transition verb only (`review --quorum` is a doc parameter).
            if name == "set-status" {
                assert!(!props.contains_key("quorum"), "{tool}");
            }
        }
    }

    #[test]
    fn annotations_split_read_only_from_destructive() {
        // Read tools stay `readOnlyHint: true` in both modes; write-slice
        // tools (and `plan` once `--save` is re-admitted) announce
        // `destructiveHint: true` — the client confirmation signal the
        // ADR-0015 reopen criterion required.
        for tool in server().tool_list() {
            assert_eq!(
                tool["annotations"]["readOnlyHint"],
                json!(true),
                "read-only mode tool `{}` must be readOnlyHint",
                tool["name"]
            );
        }
        let s = write_server();
        for tool in s.tool_list() {
            let name = tool["name"].as_str().unwrap();
            let destructive = tool["annotations"]["destructiveHint"] == json!(true);
            let read_only = tool["annotations"]["readOnlyHint"] == json!(true);
            assert_ne!(destructive, read_only, "`{name}` hints must be exclusive");
            match name {
                "new" | "compose" | "set-status" | "plan" => {
                    assert!(destructive, "`{name}` must be destructive: {tool}")
                }
                _ => assert!(read_only, "`{name}` must stay read-only: {tool}"),
            }
        }
    }

    #[test]
    fn write_tools_force_no_edit_where_an_editor_could_open() {
        let s = write_server();
        for t in &s.tools {
            match t.name.as_str() {
                "new" | "compose" => assert_eq!(t.forced, &["--no-edit"], "`{}`", t.name),
                _ => assert!(t.forced.is_empty(), "`{}` forces {:?}", t.name, t.forced),
            }
        }
    }

    #[test]
    fn default_mode_is_byte_identical_to_pre_slice_projection() {
        // ADR-0015's property survives ADR-0021: without `--allow-write` the
        // projected surface carries no write verb, no re-admitted escalation,
        // and no destructive annotation — the one-sentence audit stands.
        let s = server();
        for tool in s.tool_list() {
            assert_eq!(tool["annotations"]["destructiveHint"], json!(false));
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            assert!(!props.contains_key("save"), "{tool}");
        }
        let names: Vec<&str> = s.tools.iter().map(|t| t.name.as_str()).collect();
        for w in ["new", "compose", "set-status"] {
            assert!(!names.contains(&w));
        }
    }

    #[test]
    fn projects_read_verbs_not_writers_or_servers() {
        let s = server();
        let names: Vec<&str> = s.tools.iter().map(|t| t.name.as_str()).collect();
        for read in ["list", "show", "search", "stats", "graph", "check"] {
            assert!(
                names.contains(&read),
                "missing read tool `{read}`: {names:?}"
            );
        }
        for excluded in [
            "new",
            "set-status",
            "supersede",
            "import",  // repo writes
            "publish", // produces an output tree
            "sync",
            "notify", // forge / webhook network side effects
            "serve",
            "mcp", // long-running servers
        ] {
            assert!(
                !names.contains(&excluded),
                "should not expose `{excluded}`: {names:?}"
            );
        }
    }

    #[test]
    fn projected_tools_carry_no_escalating_flags() {
        // The ADR-0005/0006 conformance: no projected tool's arg surface can
        // mutate the repo, the forge, or the filesystem. `forge` reaches the
        // forge, `yes` / `dry_run` apply / preview that side effect, `out`
        // writes an arbitrary local file, and `save` / `force` / `regenerate`
        // (ADR-0008) splice the corpus or force a fresh provider call — none
        // may appear in any inputSchema.
        const ESCALATING: &[&str] = &[
            "forge",
            "yes",
            "dry_run",
            "out",
            "save",
            "force",
            "regenerate",
        ];
        let s = server();
        let tools = s.tool_list();
        assert!(!tools.is_empty());
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            for flag in ESCALATING {
                assert!(
                    !props.contains_key(*flag),
                    "projected tool `{name}` leaks escalating flag `{flag}`"
                );
            }
        }
    }

    #[test]
    fn projection_strips_every_flag_the_manifest_marks_escalating() {
        // Mechanical drift-guard: whatever the manifest classifies as
        // escalating — today's flags or a future one — must be absent from the
        // projected schema of the same verb.
        let m: Value = serde_json::from_str(&manifest::json()).unwrap();
        let s = server();
        for tool in s.tool_list() {
            let name = tool["name"].as_str().unwrap();
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            let cmd = m["commands"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == name)
                .unwrap_or_else(|| panic!("tool `{name}` is in the manifest"));
            let args = cmd["args"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            for arg in args {
                if arg["escalates"].is_string() {
                    let flag = arg["name"].as_str().unwrap();
                    assert!(
                        !props.contains_key(flag),
                        "tool `{name}` projects `{flag}`, which the manifest marks escalates={}",
                        arg["escalates"]
                    );
                }
            }
        }
    }

    #[test]
    fn input_schema_marks_required_and_enum() {
        // A verb with a required positional + an enum option exercises both.
        let args = vec![
            opt("id", None, true, true, false, &[]),
            opt(
                "status",
                Some("--status"),
                false,
                false,
                false,
                &["Proposed", "Accepted"],
            ),
            opt("dry_run", Some("--dry-run"), false, false, true, &[]),
        ];
        let schema = input_schema(&args);
        assert_eq!(schema["required"], json!(["id"]));
        assert_eq!(
            schema["properties"]["status"]["enum"],
            json!(["Proposed", "Accepted"])
        );
        assert_eq!(schema["properties"]["dry_run"]["type"], json!("boolean"));
    }

    #[test]
    fn build_argv_orders_options_then_positional() {
        let tool = Tool {
            name: "x".into(),
            description: None,
            args: vec![
                opt("id", None, true, true, false, &[]),
                opt("status", Some("--status"), false, false, false, &[]),
                opt("dry_run", Some("--dry-run"), false, false, true, &[]),
            ],
            forced: &[],
            destructive: false,
        };
        let args = json!({ "id": "5", "status": "accepted", "dry_run": true });
        let argv = build_argv(&tool, Some(&args)).unwrap();
        assert_eq!(argv, vec!["--status", "accepted", "--dry-run", "5"]);
    }

    #[test]
    fn build_argv_rejects_non_object_arguments() {
        let tool = Tool {
            name: "x".into(),
            description: None,
            args: vec![],
            forced: &[],
            destructive: false,
        };
        assert!(build_argv(&tool, Some(&json!("oops"))).is_err());
    }

    fn opt(
        name: &str,
        long: Option<&str>,
        positional: bool,
        required: bool,
        flag: bool,
        values: &[&str],
    ) -> OptionInfo {
        OptionInfo {
            name: name.to_string(),
            long: long.map(str::to_string),
            short: None,
            positional,
            required,
            flag,
            value: None,
            values: values.iter().map(|s| s.to_string()).collect(),
            default: None,
            env: None,
            help: None,
            escalates: None,
        }
    }
}

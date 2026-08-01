//! End-to-end tests of the `tuesday-report` binary via assert_cmd, against
//! an in-process stub speaking just enough of Gitea's REST v1 to serve
//! `GET /api/v1/repos/{owner}/{repo}/pulls`. No network beyond loopback.

use assert_cmd::Command;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// One observed request: (path-and-query, Authorization header value).
type SeenRequests = Arc<Mutex<Vec<(String, String)>>>;

struct StubForge {
    base_url: String,
    seen: SeenRequests,
}

/// Serve `body` (a Gitea pulls array) for every GET, recording each request.
/// The acceptor thread dies with the test process.
fn stub_forge(body: String) -> StubForge {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let seen: SeenRequests = Arc::new(Mutex::new(Vec::new()));

    let seen_writer = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string();

            let mut authorization = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.trim_end().is_empty() {
                    break;
                }
                // hyper sends lowercase header names; match case-insensitively.
                if line.to_ascii_lowercase().starts_with("authorization:") {
                    authorization = line["authorization:".len()..].trim().to_string();
                }
            }
            seen_writer.lock().unwrap().push((path, authorization));

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    StubForge { base_url, seen }
}

/// A Gitea REST v1 pull payload merged at the given instant.
fn pull_merged_at(number: u64, title: &str, labels: &[&str], merged_at: &str) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "title": title,
        "body": "Body.\n\nAdr-Reference: ADR-0001",
        "html_url": format!("http://localhost:3000/como/conduit-dogfood/pulls/{number}"),
        "merged": true,
        "merged_at": merged_at,
        "labels": labels.iter().map(|name| serde_json::json!({"name": name})).collect::<Vec<_>>(),
    })
}

/// A Gitea REST v1 pull payload, merged 2026-03-15.
fn pull(number: u64, title: &str, labels: &[&str]) -> serde_json::Value {
    pull_merged_at(number, title, labels, "2026-03-15T12:00:00Z")
}

fn pulls_body(pulls: &[serde_json::Value]) -> String {
    serde_json::Value::Array(pulls.to_vec()).to_string()
}

/// The binary under test, with token env cleared for deterministic runs.
fn tuesday_report() -> Command {
    let mut cmd = Command::cargo_bin("tuesday-report").unwrap();
    cmd.env_remove("GITHUB_TOKEN")
        .env_remove("TUESDAY_GITEA_TOKEN")
        .env_remove("RUST_LOG");
    cmd
}

fn gitea_args(base_url: &str) -> Vec<String> {
    [
        "--source",
        "gitea",
        "--base-url",
        base_url,
        "--owner",
        "como",
        "--repo",
        "conduit-dogfood",
        "--year",
        "2026",
        "--month",
        "3",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn help_documents_the_full_surface() {
    let output = tuesday_report().arg("--help").output().unwrap();
    assert_eq!(output.status.code(), Some(0));

    let help = String::from_utf8(output.stdout).unwrap();
    for flag in [
        "--source",
        "--owner",
        "--repo",
        "--year",
        "--month",
        "--from",
        "--to",
        "--monthly-hours",
        "--base-url",
        "--token-file",
        "--output",
        "--scaling",
        "--strict",
        "--kb",
    ] {
        assert!(
            help.contains(flag),
            "--help should document {flag}:\n{help}"
        );
    }
    assert!(
        help.contains("tuesday-report"),
        "usage names the binary:\n{help}"
    );
}

#[test]
fn json_mode_prints_pure_json_with_adr_totals_and_exits_zero() {
    let forge = stub_forge(pulls_body(&[pull(
        1,
        "[ADR-0001] Good work",
        &["effort:3-average", "adr:ADR-0001"],
    )]));

    let output = tuesday_report()
        .args(gitea_args(&forge.base_url))
        .args(["-o", "json"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // stdout is PURE JSON: it parses as a single value.
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout parses as JSON");
    assert_eq!(report["month"], "March");
    assert_eq!(report["year"], 2026);
    assert_eq!(report["organization"], "como");
    assert_eq!(report["adr_totals"]["ADR-0001"], 360.0);
    assert_eq!(report["allocations"][0]["pr_number"], 1);
}

#[test]
fn table_is_the_default_output_mode() {
    let forge = stub_forge(pulls_body(&[pull(
        1,
        "[ADR-0001] Good work",
        &["effort:3-average", "adr:ADR-0001"],
    )]));

    let output = tuesday_report()
        .args(gitea_args(&forge.base_url))
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("March 2026"), "table header:\n{stdout}");
    assert!(stdout.contains("ADR totals"), "table rollup:\n{stdout}");
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "default mode is the human table, not JSON"
    );
}

#[test]
fn strict_exits_zero_when_every_pr_satisfies_the_contract() {
    // The referee ruling's positive side: effort + adr:* counts as
    // allocated even with zero category labels.
    let forge = stub_forge(pulls_body(&[
        pull(
            1,
            "[ADR-0001] Good work",
            &["effort:3-average", "adr:ADR-0001"],
        ),
        pull(2, "Categorized work", &["effort:1-super-quick", "feature"]),
    ]));

    let output = tuesday_report()
        .args(gitea_args(&forge.base_url))
        .args(["-o", "json", "--strict"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "strict must pass contract-shaped PRs; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the report is emitted");
    assert_eq!(report["allocations"].as_array().unwrap().len(), 2);
}

#[test]
fn strict_exits_nonzero_listing_the_offending_prs() {
    let forge = stub_forge(pulls_body(&[
        pull(
            1,
            "[ADR-0001] Good work",
            &["effort:3-average", "adr:ADR-0001"],
        ),
        pull(2, "Mystery work", &["experiment"]),
        pull(3, "Unattributed work", &["effort:2-not-long"]),
    ]));

    let output = tuesday_report()
        .args(gitea_args(&forge.base_url))
        .args(["-o", "json", "--strict"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "strict violations exit 1");

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("#2"), "lists PR 2:\n{stderr}");
    assert!(stderr.contains("Mystery work"), "names PR 2:\n{stderr}");
    assert!(stderr.contains("#3"), "lists PR 3:\n{stderr}");
    assert!(!stderr.contains("#1"), "PR 1 conforms:\n{stderr}");

    // The report artifact is still emitted for inspection.
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout still carries the report");
    assert_eq!(report["unallocated_prs"][0][1], 2);
}

#[test]
fn repeated_repo_flags_fetch_every_repository() {
    let forge = stub_forge(pulls_body(&[]));

    let output = tuesday_report()
        .args(gitea_args(&forge.base_url))
        .args(["--repo", "second-repo", "-o", "json"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let seen = forge.seen.lock().unwrap();
    let paths: Vec<&str> = seen.iter().map(|(path, _)| path.as_str()).collect();
    assert!(
        paths
            .iter()
            .any(|p| p.starts_with("/api/v1/repos/como/conduit-dogfood/pulls")),
        "first repo fetched: {paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|p| p.starts_with("/api/v1/repos/como/second-repo/pulls")),
        "second repo fetched: {paths:?}"
    );
}

#[test]
fn token_file_beats_the_environment_token() {
    let forge = stub_forge(pulls_body(&[]));
    let token_path =
        std::env::temp_dir().join(format!("tuesday-report-e2e-token-{}", std::process::id()));
    std::fs::write(&token_path, "file-token\n").unwrap();

    let output = tuesday_report()
        .args(gitea_args(&forge.base_url))
        .args(["--token-file", token_path.to_str().unwrap(), "-o", "json"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();
    std::fs::remove_file(&token_path).unwrap();

    assert_eq!(output.status.code(), Some(0));
    let seen = forge.seen.lock().unwrap();
    assert_eq!(seen[0].1, "token file-token", "Authorization from the file");
}

#[test]
fn env_token_reaches_the_forge_when_no_file_is_given() {
    let forge = stub_forge(pulls_body(&[]));

    let output = tuesday_report()
        .args(gitea_args(&forge.base_url))
        .args(["-o", "json"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let seen = forge.seen.lock().unwrap();
    assert_eq!(seen[0].1, "token env-token");
}

#[test]
fn github_without_a_token_fails_fast_with_the_fix() {
    let output = tuesday_report()
        .args([
            "--source", "github", "--owner", "como", "--repo", "tuesday", "--year", "2026",
            "--month", "3",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("GITHUB_TOKEN") && stderr.contains("--token-file"),
        "error lists both ways to supply a token:\n{stderr}"
    );
}

#[test]
fn base_url_is_rejected_for_github() {
    let output = tuesday_report()
        .args([
            "--source",
            "github",
            "--base-url",
            "http://localhost:3000",
            "--owner",
            "como",
            "--repo",
            "tuesday",
            "--year",
            "2026",
            "--month",
            "3",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--base-url") && stderr.contains("gitea"),
        "error explains the flag is gitea-only:\n{stderr}"
    );
}

#[test]
fn out_of_range_month_is_a_usage_error() {
    let output = tuesday_report()
        .args([
            "--source",
            "gitea",
            "--owner",
            "como",
            "--repo",
            "conduit-dogfood",
            "--year",
            "2026",
            "--month",
            "13",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "clap usage errors exit 2");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("month"), "names the bad flag:\n{stderr}");
}

/// `gitea_args` with the single-month window swapped for --from/--to.
fn range_gitea_args(base_url: &str, from: &str, to: &str) -> Vec<String> {
    let mut args: Vec<String> = gitea_args(base_url)
        .into_iter()
        .take_while(|arg| arg != "--year")
        .collect();
    args.extend(["--from", from, "--to", to].iter().map(|s| s.to_string()));
    args
}

/// One PR per month across the 2025→2026 year boundary, plus a violator in
/// January. The stub serves the full list for every window; the provider's
/// merge-window filter is what splits the months — exactly the production
/// path.
fn year_boundary_pulls() -> String {
    pulls_body(&[
        pull_merged_at(
            10,
            "[ADR-0001] December work",
            &["effort:3-average", "adr:ADR-0001"],
            "2025-12-10T12:00:00Z",
        ),
        pull_merged_at(
            11,
            "[ADR-0001] January work",
            &["effort:1-super-quick", "adr:ADR-0001"],
            "2026-01-05T12:00:00Z",
        ),
        pull_merged_at(
            12,
            "Mystery January work",
            &["experiment"],
            "2026-01-20T12:00:00Z",
        ),
    ])
}

#[test]
fn range_mode_emits_the_envelope_of_per_month_reports_across_a_year_boundary() {
    let forge = stub_forge(year_boundary_pulls());

    let output = tuesday_report()
        .args(range_gitea_args(&forge.base_url, "2025-12", "2026-01"))
        .args(["-o", "json"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is pure JSON");
    assert_eq!(envelope["from"], "2025-12");
    assert_eq!(envelope["to"], "2026-01");

    // One canonical per-month report per month, each PR in its merge month.
    let reports = envelope["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0]["month"], "December");
    assert_eq!(reports[0]["year"], 2025);
    assert_eq!(reports[0]["allocations"][0]["pr_number"], 10);
    assert_eq!(reports[1]["month"], "January");
    assert_eq!(reports[1]["year"], 2026);
    assert_eq!(reports[1]["allocations"][0]["pr_number"], 11);

    // The cross-month rollup sums ADR-0001 across the boundary: 360 + 360.
    assert_eq!(envelope["adr_totals"]["ADR-0001"], 720.0);
}

#[test]
fn range_strict_exits_nonzero_when_any_month_violates() {
    let forge = stub_forge(year_boundary_pulls());

    let output = tuesday_report()
        .args(range_gitea_args(&forge.base_url, "2025-12", "2026-01"))
        .args(["-o", "json", "--strict"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    // January's contract violation drives the exit code; December is clean.
    assert_eq!(output.status.code(), Some(1), "strict violations exit 1");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("#12"), "lists the violator:\n{stderr}");
    assert!(!stderr.contains("#10"), "December conforms:\n{stderr}");

    // The envelope is still emitted for inspection.
    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout still carries the envelope");
    assert_eq!(envelope["reports"].as_array().unwrap().len(), 2);
}

#[test]
fn range_table_mode_emits_the_sectioned_table_with_the_rollup() {
    let forge = stub_forge(year_boundary_pulls());

    let output = tuesday_report()
        .args(range_gitea_args(&forge.base_url, "2025-12", "2026-01"))
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("December 2025"), "first section:\n{stdout}");
    assert!(stdout.contains("January 2026"), "second section:\n{stdout}");
    assert!(
        stdout.contains("ADR totals across the range"),
        "rollup:\n{stdout}"
    );
}

#[test]
fn from_without_to_is_a_usage_error() {
    let output = tuesday_report()
        .args([
            "--source",
            "gitea",
            "--owner",
            "como",
            "--repo",
            "conduit-dogfood",
            "--from",
            "2025-12",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "clap usage errors exit 2");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--to"), "names the missing flag:\n{stderr}");
}

#[test]
fn inverted_range_exits_nonzero_naming_both_ends() {
    let forge = stub_forge(year_boundary_pulls());

    let output = tuesday_report()
        .args(range_gitea_args(&forge.base_url, "2026-01", "2025-12"))
        .args(["-o", "json"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("2026-01") && stderr.contains("2025-12"),
        "names both ends:\n{stderr}"
    );
}

#[test]
fn forge_errors_exit_nonzero_with_a_message_on_stderr() {
    // Nothing listens on this port (bound then dropped to find a free one).
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);

    let output = tuesday_report()
        .args(gitea_args(&base_url))
        .args(["-o", "json"])
        .env("TUESDAY_GITEA_TOKEN", "env-token")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "no partial report on stdout");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("error"), "diagnostic on stderr:\n{stderr}");
}

#[test]
fn kb_writes_a_measure_report_page_into_a_space() {
    // Wave 4 (portfolio#7): --kb emits the month as a measure-report typed
    // page. End to end over the real binary and a stub forge: page lands at
    // wiki/measures/<owner>-<YYYY-MM>.md, typed, deterministic, carrying the
    // adr_totals attribution the harness will query.
    let forge = stub_forge(pulls_body(&[
        pull(7, "Ship the widget", &["effort:3-average", "adr:ADR-0003"]),
        pull(
            8,
            "Fix the gadget",
            &["effort:1-super-quick", "category/bug"],
        ),
    ]));

    let space = tempfile::tempdir().unwrap();
    std::fs::write(space.path().join("wiki.toml"), "name = \"t\"\n").unwrap();

    let mut args = gitea_args(&forge.base_url);
    args.push("--kb".into());
    args.push(space.path().to_string_lossy().into_owned());

    let output = tuesday_report().args(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("kb: wrote"), "{stderr}");

    let page_path = space.path().join("wiki/measures/como-2026-03.md");
    let page = std::fs::read_to_string(&page_path).unwrap();
    assert!(page.contains("type: measure-report\n"), "{page}");
    assert!(page.contains("period: \"2026-03\"\n"), "{page}");
    assert!(page.contains("instrument: tuesday\n"), "{page}");
    assert!(page.contains("ADR-0003"), "{page}");

    // Converge: a second run over the same forge data is byte-identical.
    let rerun = tuesday_report().args(&args).output().unwrap();
    assert_eq!(rerun.status.code(), Some(0));
    assert_eq!(page, std::fs::read_to_string(&page_path).unwrap());
}

#[test]
fn kb_requires_a_space_and_names_the_bootstrap() {
    let forge = stub_forge(pulls_body(&[pull(
        7,
        "Ship the widget",
        &["effort:3-average", "adr:ADR-0003"],
    )]));
    let not_a_space = tempfile::tempdir().unwrap();

    let mut args = gitea_args(&forge.base_url);
    args.push("--kb".into());
    args.push(not_a_space.path().to_string_lossy().into_owned());

    let output = tuesday_report().args(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("not a KB space"), "{stderr}");
    assert!(stderr.contains("llm-wiki spaces create"), "{stderr}");
}

#[test]
fn kb_is_skipped_when_strict_violations_exist() {
    // A contract-violating month doesn't enter the record: --strict + --kb
    // exits nonzero, says so, and writes no page.
    let forge = stub_forge(pulls_body(&[pull(9, "No labels at all", &[])]));
    let space = tempfile::tempdir().unwrap();
    std::fs::write(space.path().join("wiki.toml"), "name = \"t\"\n").unwrap();

    let mut args = gitea_args(&forge.base_url);
    args.extend([
        "--strict".into(),
        "--kb".into(),
        space.path().to_string_lossy().into_owned(),
    ]);

    let output = tuesday_report().args(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("kb: not written"), "{stderr}");
    assert!(!space.path().join("wiki/measures").exists());
}

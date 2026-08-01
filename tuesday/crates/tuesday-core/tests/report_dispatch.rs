//! Integration proof that `generate_report` dispatches through the
//! `PrSource` seam from `ReportConfig.source`: a loopback stub speaking
//! just enough of Gitea's REST v1 receives the requests the config
//! describes. No network beyond loopback.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tuesday_core::{ReportConfig, SourceKind, generate_report};

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

/// A Gitea REST v1 pull payload, merged 2026-03-15.
fn pulls_body() -> String {
    serde_json::json!([{
        "number": 1,
        "title": "[ADR-0001] Good work",
        "body": "Body.\n\nAdr-Reference: ADR-0001",
        "html_url": "http://localhost:3000/como/conduit-dogfood/pulls/1",
        "merged": true,
        "merged_at": "2026-03-15T12:00:00Z",
        "labels": [{"name": "effort:3-average"}, {"name": "adr:ADR-0001"}],
    }])
    .to_string()
}

fn gitea_config(base_url: &str) -> ReportConfig {
    ReportConfig {
        source: SourceKind::Gitea,
        base_url: Some(base_url.to_string()),
        token: "stub-token".to_string(),
        organization: "como".to_string(),
        repositories: vec!["conduit-dogfood".to_string()],
        year: 2026,
        month: 3,
        monthly_hours: 160.0,
        ..ReportConfig::default()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn gitea_config_drives_the_gitea_source_and_yields_adr_totals() {
    let forge = stub_forge(pulls_body());

    let report = generate_report(gitea_config(&forge.base_url))
        .await
        .expect("report over the stub");

    // The canonical MonthlyReport, ADR rollup included (ADR-0004/0005).
    assert_eq!(report.month, "March");
    assert_eq!(report.year, 2026);
    assert_eq!(report.organization, "como");
    assert_eq!(report.total_hours, 160.0);
    assert_eq!(report.adr_totals.get("ADR-0001"), Some(&160.0));
    assert_eq!(report.allocations[0].pr_number, 1);

    // The dispatch reached the GITEA provider: REST v1 path, token scheme.
    let seen = forge.seen.lock().unwrap();
    assert!(
        seen.iter()
            .any(|(path, _)| path.starts_with("/api/v1/repos/como/conduit-dogfood/pulls")),
        "stub saw the Gitea pulls endpoint: {seen:?}"
    );
    assert_eq!(seen[0].1, "token stub-token", "Gitea token auth scheme");
}

#[tokio::test(flavor = "current_thread")]
async fn every_configured_repository_is_fetched() {
    let forge = stub_forge("[]".to_string());

    let mut cfg = gitea_config(&forge.base_url);
    cfg.repositories = vec!["alpha".to_string(), "beta".to_string()];
    let report = generate_report(cfg).await.unwrap();
    assert!(report.allocations.is_empty());

    let seen = forge.seen.lock().unwrap();
    for repo in ["alpha", "beta"] {
        assert!(
            seen.iter()
                .any(|(path, _)| path.starts_with(&format!("/api/v1/repos/como/{repo}/pulls"))),
            "{repo} fetched: {seen:?}"
        );
    }
}

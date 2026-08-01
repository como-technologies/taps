use dioxus::prelude::*;
use tuesday_core::{MonthlyReport, ReportConfig};

/// Machine-readable JSON export of the monthly report.
///
/// Mounted at the stable path `POST /api/export_report` so other tools can
/// invoke it headlessly (no UI session required):
///
/// ```sh
/// curl -sS -X POST http://127.0.0.1:8080/api/export_report \
///   -H 'Content-Type: application/json' \
///   -d '{"config":{"source":"gitea","base_url":"http://localhost:3000","token":"<token>","monthly_hours":160.0,"repositories":["my-repo"],"organization":"my-org","year":2026,"month":5,"scaling_series":"Linear"}}'
/// ```
///
/// The response body is the serialized `MonthlyReport`, including
/// `category_totals` and `adr_totals` (full allocated hours attributed to
/// each ADR reference).
#[server(endpoint = "export_report")]
pub async fn export_report(config: ReportConfig) -> Result<MonthlyReport, ServerFnError> {
    tuesday_core::generate_report(config)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use tuesday_cli::cli::OutputFormat;
    use tuesday_cli::run::{RunRequest, run_with_source};
    use tuesday_core::{GiteaSource, ReportConfig, ScalingSeries, SourceKind};

    /// Serve `body` (a Gitea pulls array) for every GET — just enough of
    /// Gitea's REST v1 for the pulls endpoint, on loopback only. The
    /// acceptor thread dies with the test process.
    fn stub_forge(body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.trim_end().is_empty() {
                        break;
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        base_url
    }

    /// Two contract-shaped merged PRs and one closed-unmerged PR, in
    /// Gitea's REST v1 list shape.
    fn pulls_body() -> String {
        serde_json::json!([
            {
                "number": 1,
                "title": "[ADR-0001] Good work",
                "body": "Body.\n\nAdr-Reference: ADR-0001",
                "html_url": "http://localhost:3000/como/conduit-dogfood/pulls/1",
                "merged": true,
                "merged_at": "2026-03-15T12:00:00Z",
                "labels": [{"name": "effort:3-average"}, {"name": "adr:ADR-0001"}],
            },
            {
                "number": 2,
                "title": "[ADR-0002] More work",
                "body": "Body.\n\nAdr-Reference: ADR-0002",
                "html_url": "http://localhost:3000/como/conduit-dogfood/pulls/2",
                "merged": true,
                "merged_at": "2026-03-20T12:00:00Z",
                "labels": [{"name": "effort:1-super-quick"}, {"name": "adr:ADR-0002"}],
            },
            {
                "number": 3,
                "title": "Rejected",
                "body": null,
                "html_url": "http://localhost:3000/como/conduit-dogfood/pulls/3",
                "merged": false,
                "merged_at": null,
                "labels": [],
            },
        ])
        .to_string()
    }

    /// The canonical serialization (ADR-0004): through `serde_json::Value`
    /// (BTreeMap-backed) so object keys are sorted — the same path the
    /// CLI's `-o json` renderer takes.
    fn canonical_json(report: &MonthlyReport) -> String {
        let value = serde_json::to_value(report).unwrap();
        serde_json::to_string_pretty(&value).unwrap()
    }

    /// The web JSON-export path and `tuesday-report -o json` must emit the
    /// SAME canonical MonthlyReport — adr_totals included — for the same
    /// window over the same (stub) Gitea forge.
    #[tokio::test(flavor = "current_thread")]
    async fn web_export_equals_cli_json_over_the_same_gitea_stub() {
        let base_url = stub_forge(pulls_body());

        // The web head's export path: the server fn over ReportConfig.
        let config = ReportConfig {
            source: SourceKind::Gitea,
            base_url: Some(base_url.clone()),
            token: "stub-token".to_string(),
            organization: "como".to_string(),
            repositories: vec!["conduit-dogfood".to_string()],
            year: 2026,
            month: 3,
            monthly_hours: 160.0,
            scaling_series: ScalingSeries::Linear,
        };
        let web_report = export_report(config).await.expect("web export path");
        let web_json = canonical_json(&web_report);

        // The CLI head over the same stub and window.
        let request = RunRequest {
            owner: "como".to_string(),
            repos: vec!["conduit-dogfood".to_string()],
            year: 2026,
            month: 3,
            monthly_hours: 160.0,
            scaling: ScalingSeries::Linear,
            output: OutputFormat::Json,
            strict: false,
        };
        let cli_outcome = run_with_source(
            &GiteaSource::new(base_url, Some("stub-token".to_string())),
            &request,
        )
        .await
        .expect("CLI pipeline over the stub");

        assert_eq!(
            web_json, cli_outcome.stdout,
            "web export and tuesday-report disagree on the canonical report"
        );

        // The shared report carries the ADR rollup (full credit per ADR).
        let value: serde_json::Value = serde_json::from_str(&web_json).unwrap();
        assert_eq!(value["adr_totals"]["ADR-0001"], 120.0);
        assert_eq!(value["adr_totals"]["ADR-0002"], 40.0);
        assert_eq!(value["month"], "March");
        assert_eq!(value["organization"], "como");
    }

    /// AdrBreakdown renders on the Gitea path: the report fetched over the
    /// Gitea stub, fed to the ReportView component, shows the ADR section.
    #[tokio::test(flavor = "current_thread")]
    async fn adr_breakdown_renders_on_the_gitea_path() {
        let base_url = stub_forge(pulls_body());

        let config = ReportConfig {
            source: SourceKind::Gitea,
            base_url: Some(base_url),
            token: "stub-token".to_string(),
            organization: "como".to_string(),
            repositories: vec!["conduit-dogfood".to_string()],
            year: 2026,
            month: 3,
            monthly_hours: 160.0,
            scaling_series: ScalingSeries::Linear,
        };
        let report = export_report(config).await.expect("web export path");

        let html = dioxus::ssr::render_element(rsx! {
            crate::components::report::ReportView { report }
        });

        assert!(
            html.contains("Hours by ADR") && html.contains("adr-breakdown"),
            "AdrBreakdown section missing from the rendered report:\n{html}"
        );
        assert!(html.contains("ADR-0001"), "ADR id rendered:\n{html}");
    }
}

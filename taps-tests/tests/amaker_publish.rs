//! Seam: amaker → a live llm-wiki appliance, over streamable HTTP, driven
//! through amaker's own door (`amaker publish`, the CLI binary).
//!
//! Setup goes through amaker's own doors where they exist: the project is
//! created with `amaker import` (the headless authoring door). Answers are
//! recorded with amaker-core's response service — deliberately: responding
//! is the one human-only act in the loop, so there is no headless door for
//! it. All *assertions* go through transports only: the publish CLI's
//! stdout report and the KB's wiki tools.

mod helpers;

use helpers::{Appliance, bin, gated};
use serde_json::json;

/// The golden sample: two questions, one answered yes (pass), one yes on a
/// negative-polarity question (a definite gap).
const ASSESSMENT_YAML: &str = "\
id: 11111111-1111-4111-8111-111111111111
name: Release Readiness
description: A minimal sample assessment of software release readiness
goal: Find the gaps that block predictable releases
created_at: 2026-01-01T00:00:00Z
updated_at: 2026-01-01T00:00:00Z
domains:
- id: 22222222-2222-4222-8222-222222222222
  name: Delivery
  context: How changes reach production
  value: Predictable releases
  risk: Slow error-prone releases
  practices:
  - id: 33333333-3333-4333-8333-333333333333
    name: Continuous Integration
    context: Every change is built and tested automatically
    value: Defects surface early
    risk: Broken builds at release time
    questions:
    - id: 44444444-4444-4444-8444-444444444444
      text: Does every change build and run tests in CI before merge?
      polarity: positive
    - id: 55555555-5555-4555-8555-555555555555
      text: Are releases routinely delayed by manual verification?
      polarity: negative
      remediation: Automate the manual checks that block releases most often
      roles:
      - engineer
";

async fn seed_amaker_data(data_dir: &std::path::Path) -> String {
    use amaker_core::models::{Answer, AnswerValue};
    use amaker_core::services::{ResponseService, StorageService};
    use amaker_core::storage_backend::{build_store, filesystem};

    // The headless authoring door: a drafted assessment file becomes a
    // project with a published version.
    let file = data_dir.join("release-readiness.yaml");
    std::fs::write(&file, ASSESSMENT_YAML).expect("write assessment file");
    let output = std::process::Command::new(bin("amaker"))
        .env("DATA_DIR", data_dir)
        .args(["import", "--name", "Release Readiness"])
        .arg(&file)
        .output()
        .expect("run amaker import");
    assert!(
        output.status.success(),
        "import failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let imported: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("import report is JSON");
    let project_id: amaker_core::models::ProjectId =
        imported["project_id"].as_str().unwrap().parse().unwrap();

    // Responding is human work; the test stands in for the respondent via
    // the same service the assess app uses.
    let storage = StorageService::new(build_store(&filesystem(data_dir)).expect("store"));
    let responses = ResponseService::new(storage);
    responses.ensure_primary(project_id).await.expect("primary");
    use amaker_core::models::ids::QuestionId;
    let q1: QuestionId = "44444444-4444-4444-8444-444444444444".parse().unwrap();
    let q2: QuestionId = "55555555-5555-4555-8555-555555555555".parse().unwrap();
    responses
        .upsert_answer(project_id, q1, Answer::new(AnswerValue::Yes))
        .await
        .expect("answer q1");
    responses
        .upsert_answer(project_id, q2, Answer::new(AnswerValue::Yes))
        .await
        .expect("answer q2");

    project_id.to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn amaker_publishes_assessment_and_report_to_the_kb() {
    if gated() {
        return;
    }
    let appliance = Appliance::launch("amspace");
    let data_dir = tempfile::tempdir().expect("data dir");
    let project_id = seed_amaker_data(data_dir.path()).await;

    // amaker's own door: the publish CLI verb, configured the way a user
    // configures it — KB_URL/KB_WIKI in the environment.
    let output = std::process::Command::new(bin("amaker"))
        .env("DATA_DIR", data_dir.path())
        .env("KB_URL", &appliance.url)
        .env("KB_WIKI", &appliance.wiki)
        .args(["publish", &project_id])
        .output()
        .expect("run amaker publish");
    assert!(
        output.status.success(),
        "publish failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("publish report is JSON");
    assert_eq!(report["ingest"]["pages_validated"], 2, "{report}");
    assert_eq!(report["ingest"]["indexed"], 2, "{report}");

    // Everything else is asserted through the KB's transport.
    let kb = appliance.client().await;

    // amaker's classes arrived on first contact, owned by amaker.
    let schema = kb
        .call(
            "wiki_schema",
            json!({"action": "show", "type": "assessment-report"}),
        )
        .await
        .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema).unwrap();
    assert_eq!(schema["x-owner"], "amaker");

    // The report page is admitted, typed, and searchable.
    let page = kb
        .call(
            "wiki_content_read",
            json!({"uri": report["report_page"].as_str().unwrap()}),
        )
        .await
        .unwrap();
    assert!(page.contains("type: assessment-report"), "{page}");
    assert!(
        page.contains("overall_percent: 50"),
        "one pass one fail: {page}"
    );
    assert!(page.contains("gaps: 1"), "{page}");
    assert!(page.contains("Automate the manual checks"), "{page}");

    let found = kb
        .call(
            "wiki_search",
            json!({"query": "release readiness", "format": "llms"}),
        )
        .await
        .unwrap();
    assert!(found.contains("assessments/release-readiness"), "{found}");

    // Re-publish: idempotent schemas, refreshed pages, still clean.
    let again = std::process::Command::new(bin("amaker"))
        .env("DATA_DIR", data_dir.path())
        .env("KB_URL", &appliance.url)
        .env("KB_WIKI", &appliance.wiki)
        .args(["publish", &project_id])
        .output()
        .expect("re-publish");
    assert!(again.status.success());
    let again: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert!(
        again["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s.as_str().unwrap().ends_with("unchanged")),
        "{again}"
    );

    kb.close().await.unwrap();
}

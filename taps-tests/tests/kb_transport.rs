//! Seam: any taps tool ↔ a live llm-wiki appliance, over streamable HTTP,
//! through the shared client crate. Assertions go through the transport
//! only — if it isn't reachable through a door, the test can't see it.

mod helpers;

use helpers::{Appliance, gated};
use serde_json::json;

const SAMPLE_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Integration sample type",
  "type": "object",
  "required": ["title", "type", "status"],
  "properties": {
    "title": { "type": "string" },
    "type": { "type": "string" },
    "status": { "type": "string" }
  },
  "x-wiki-types": { "sample": "A sample type registered over the transport" },
  "x-owner": "taps-tests"
}"#;

#[tokio::test(flavor = "multi_thread")]
async fn schema_registers_and_pages_admit_over_http() {
    if gated() {
        return;
    }
    let appliance = Appliance::launch("itspace");
    let kb = appliance.client().await;

    // Fresh space: no ghost artifact classes (taps#65).
    let types = kb
        .call_json("wiki_schema", json!({"action": "list"}))
        .await
        .unwrap();
    let names: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["name"].as_str())
        .collect();
    assert!(!names.contains(&"decision"), "ghost class in fresh space");
    assert!(!names.contains(&"sample"));

    // First contact: register a tool-owned type; idempotent on repeat.
    let report = kb
        .call_json(
            "wiki_admin_schema_register",
            json!({"type": "sample", "schema": SAMPLE_SCHEMA}),
        )
        .await
        .unwrap();
    assert_eq!(report["status"], "registered");
    assert_eq!(report["owner"], "taps-tests");
    let again = kb
        .call_json(
            "wiki_admin_schema_register",
            json!({"type": "sample", "schema": SAMPLE_SCHEMA}),
        )
        .await
        .unwrap();
    assert_eq!(again["status"], "unchanged");

    // Write a page of the new type, admit it through the gates, find it.
    kb.call(
        "wiki_content_write",
        json!({
            "uri": "samples/first",
            "content": "---\ntitle: \"First Sample\"\ntype: sample\nstatus: active\n---\n\nBorn over the transport.\n"
        }),
    )
    .await
    .unwrap();
    let ingest = kb
        .call_json("wiki_ingest", json!({"path": "samples"}))
        .await
        .unwrap();
    assert_eq!(ingest["pages_validated"], 1, "gate should admit the page");
    assert_eq!(ingest["indexed"], 1, "ingest indexes what it admits (#62)");

    let found = kb
        .call(
            "wiki_search",
            json!({"query": "transport", "format": "llms"}),
        )
        .await
        .unwrap();
    assert!(
        found.contains("samples/first"),
        "search should find the page: {found}"
    );

    kb.close().await.unwrap();
}

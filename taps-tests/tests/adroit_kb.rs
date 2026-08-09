//! Seam: adroit → a live llm-wiki appliance, over streamable HTTP, driven
//! through adroit's own door (the CLI binary, `-o json`). The KB is the
//! state store — every verb here round-trips through the real transport,
//! and every assertion goes through a door: adroit's stdout reports or the
//! engine's wiki tools. Ownership is a write boundary: adroit writes the
//! decision pages, and this test reads them back with no adroit dependency
//! (exactly how conduit reads accepted ADRs at Adopt).

mod helpers;

use helpers::{Appliance, bin, gated};
use serde_json::json;

/// Run the adroit binary against the appliance, expecting success and a
/// JSON report on stdout.
fn adroit(appliance: &Appliance, args: &[&str]) -> serde_json::Value {
    let output = std::process::Command::new(bin("adroit"))
        .env("KB_URL", &appliance.url)
        .env("KB_WIKI", &appliance.wiki)
        .args(["-o", "json"])
        .args(args)
        .output()
        .expect("run adroit");
    assert!(
        output.status.success(),
        "adroit {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "adroit {args:?} stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn decisions_live_and_die_over_the_transport() {
    if gated() {
        return;
    }
    let appliance = Appliance::launch("adroitspace");
    let kb = appliance.client().await;

    // Fresh space: no ghost `decision` class (taps#65) — adroit registers
    // its own schemas on first contact, below.
    let types = kb
        .call_json("wiki_schema", json!({"action": "list"}))
        .await
        .unwrap();
    assert!(
        !types
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "decision"),
        "ghost class in fresh space"
    );

    // First contact: `new` registers decision + plan and lands the page
    // through the admission gates.
    let first = adroit(
        &appliance,
        &[
            "new",
            "Use PostgreSQL",
            "--summary",
            "one datastore for the suite",
            "--body",
            "## Context and Problem Statement\n\nWe need a database.",
        ],
    );
    assert_eq!(first["reference"], "ADR-0001");
    assert_eq!(first["slug"], "decisions/0001-use-postgresql");
    assert_eq!(first["schemas"][0], "decision: registered");
    assert_eq!(first["schemas"][1], "plan: registered");
    assert_eq!(first["ingest"]["pages_validated"], 1, "{first}");

    // Second contact: registration is idempotent, allocation is max + 1,
    // and provenance edges record where the decision came from.
    let second = adroit(
        &appliance,
        &[
            "new",
            "Adopt feature flags",
            "--relates",
            "decisions/0001-use-postgresql",
            "--body",
            "## Context and Problem Statement\n\nDecouple deploy from release.",
        ],
    );
    assert_eq!(second["reference"], "ADR-0002");
    assert_eq!(second["schemas"][0], "decision: unchanged");

    // The corpus reads back through adroit…
    let list = adroit(&appliance, &["list"]);
    assert_eq!(list["decisions"].as_array().unwrap().len(), 2);
    let shown = adroit(&appliance, &["show", first["id"].as_str().unwrap()]);
    assert_eq!(shown["reference"], "ADR-0001", "resolution by page id");

    // …and through the engine's own doors, with no adroit dependency: the
    // provenance edge is real graph structure the lint can see.
    let listed = kb
        .call_json("wiki_list", json!({"type": "decision"}))
        .await
        .unwrap();
    assert_eq!(listed["total"], 2, "{listed}");
    let lint = kb
        .call("wiki_lint", json!({"rules": "broken-link"}))
        .await
        .unwrap();
    assert!(
        !lint.contains("decisions/0001"),
        "relates edge should resolve: {lint}"
    );

    // Lifecycle over the transport: accept, store a plan, supersede.
    let accepted = adroit(&appliance, &["set-status", "1", "accepted"]);
    assert_eq!(accepted["from"], "proposed");
    let plan = adroit(
        &appliance,
        &[
            "plan",
            "1",
            "--save",
            "--text",
            "1. Create the schema.\n2. Add tests.",
        ],
    );
    // The pinned envelope, exactly.
    assert_eq!(
        plan,
        json!({
            "reference": "ADR-0001",
            "title": "Use PostgreSQL",
            "plan": "1. Create the schema.\n2. Add tests.",
            "stored": true,
        })
    );
    adroit(&appliance, &["set-status", "2", "accepted"]);
    let superseded = adroit(&appliance, &["supersede", "2", "1"]);
    assert_eq!(superseded["old"]["status"], "superseded");

    // Foreign frontmatter written through the engine's door survives an
    // adroit rewrite — the round-trip is sacred across the transport too.
    let page = kb
        .call(
            "wiki_content_read",
            json!({"uri": "decisions/0002-adopt-feature-flags"}),
        )
        .await
        .unwrap();
    let close = page.rfind("\n---\n").unwrap();
    let with_citation = format!(
        "{}citations:\n- evidence/kb-chat.md@abc123\n{}",
        &page[..close + 1],
        &page[close + 1..]
    );
    kb.call(
        "wiki_content_write",
        json!({"uri": "decisions/0002-adopt-feature-flags", "content": with_citation}),
    )
    .await
    .unwrap();
    kb.call_json("wiki_ingest", json!({"path": "decisions"}))
        .await
        .unwrap();
    adroit(&appliance, &["set-review", "2", "2030-01-01"]);
    let rewritten = kb
        .call(
            "wiki_content_read",
            json!({"uri": "decisions/0002-adopt-feature-flags"}),
        )
        .await
        .unwrap();
    assert!(
        rewritten.contains("citations:\n- evidence/kb-chat.md@abc123"),
        "foreign key destroyed by rewrite: {rewritten}"
    );
    assert!(rewritten.contains("review_by: 2030-01-01"));

    // The final state honors every invariant: adroit's corpus check and the
    // engine's reading of the superseded page agree.
    let check = adroit(&appliance, &["check"]);
    assert_eq!(check["errors"], 0, "{check}");
    assert_eq!(check["warnings"], 0, "{check}");
    let old_page = kb
        .call(
            "wiki_content_read",
            json!({"uri": "decisions/0001-use-postgresql"}),
        )
        .await
        .unwrap();
    assert!(old_page.contains("status: superseded"));
    assert!(
        old_page.contains(&format!(
            "superseded_by: {}",
            second["id"].as_str().unwrap()
        )),
        "{old_page}"
    );

    kb.close().await.unwrap();
}

//! The lifecycle oracle: every verb driven end-to-end through the surface
//! core functions against an in-memory [`PageStore`] fake. The fake stores
//! raw page text exactly like the KB does, so these tests exercise the real
//! serialize → write → read → deserialize path — only the transport is
//! faked. Transport truth lives in taps-tests (`adroit_kb.rs`), against a
//! live appliance.

use std::collections::BTreeMap;
use std::sync::Mutex;

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde_json::{Value, json};

use adroit::naming::NamingScheme;
use adroit::store::PageStore;
use adroit::surface::{
    self, LintParams, ListParams, NewParams, PlanParams, SetReviewParams, SetStatusParams,
    ShowParams, SupersedeParams,
};

/// In-memory page store: slug → raw page text, plus a write counter so the
/// oracle can assert that no-ops don't rewrite pages.
#[derive(Default)]
struct FakeStore {
    pages: Mutex<BTreeMap<String, String>>,
    writes: Mutex<usize>,
}

impl FakeStore {
    fn page(&self, slug: &str) -> String {
        self.pages.lock().unwrap().get(slug).cloned().unwrap()
    }

    fn put(&self, slug: &str, content: &str) {
        self.pages
            .lock()
            .unwrap()
            .insert(slug.into(), content.into());
    }

    fn remove(&self, slug: &str) {
        self.pages.lock().unwrap().remove(slug);
    }

    fn write_count(&self) -> usize {
        *self.writes.lock().unwrap()
    }
}

#[async_trait]
impl PageStore for FakeStore {
    async fn ensure_schemas(&self) -> Result<Vec<String>> {
        Ok(vec![
            "decision: registered".into(),
            "plan: registered".into(),
        ])
    }

    async fn decision_slugs(&self) -> Result<Vec<String>> {
        Ok(self.pages.lock().unwrap().keys().cloned().collect())
    }

    async fn read(&self, slug: &str) -> Result<String> {
        match self.pages.lock().unwrap().get(slug) {
            Some(c) => Ok(c.clone()),
            None => bail!("no such page: {slug}"),
        }
    }

    async fn write(&self, slug: &str, content: &str) -> Result<()> {
        *self.writes.lock().unwrap() += 1;
        self.put(slug, content);
        Ok(())
    }

    async fn ingest(&self) -> Result<Value> {
        let n = self.pages.lock().unwrap().len();
        Ok(json!({"pages_validated": n, "indexed": n}))
    }
}

const SEQ: NamingScheme = NamingScheme::Sequential;

fn new_params(title: &str) -> NewParams {
    NewParams {
        title: title.into(),
        summary: None,
        body: None,
        body_file: None,
        relates: Vec::new(),
    }
}

async fn create(store: &FakeStore, title: &str) -> Value {
    surface::new_core(store, SEQ, &new_params(title))
        .await
        .unwrap()
}

async fn set_status(store: &FakeStore, id: &str, status: &str) -> Result<Value> {
    surface::set_status_core(
        store,
        SEQ,
        &SetStatusParams {
            id: id.into(),
            status: status.into(),
        },
    )
    .await
}

#[tokio::test]
async fn new_allocates_next_number_and_records_provenance() {
    let store = FakeStore::default();
    let first = surface::new_core(
        &store,
        SEQ,
        &NewParams {
            title: "Use PostgreSQL".into(),
            summary: Some("one datastore".into()),
            body: Some("## Context\n\nWe need a database.".into()),
            body_file: None,
            relates: vec!["assessments/taps-report".into()],
        },
    )
    .await
    .unwrap();
    assert_eq!(first["reference"], "ADR-0001");
    assert_eq!(first["slug"], "decisions/0001-use-postgresql");
    assert_eq!(first["status"], "proposed");
    assert_eq!(first["schemas"][0], "decision: registered");

    // The written page carries the provenance edge, the summary, and the
    // stamped type — visible to anyone reading through the engine.
    let text = store.page("decisions/0001-use-postgresql");
    assert!(text.contains("relates_to:\n- assessments/taps-report"));
    assert!(text.contains("summary: one datastore"));
    assert!(text.contains("type: decision"));

    // Allocation is max existing in the space + 1.
    let second = create(&store, "Adopt feature flags").await;
    assert_eq!(second["reference"], "ADR-0002");
}

#[tokio::test]
async fn resolution_accepts_reference_slug_and_id() {
    let store = FakeStore::default();
    let created = create(&store, "Use PostgreSQL").await;
    for id in [
        "1",
        "0001",
        "ADR-0001",
        "decisions/0001-use-postgresql",
        "0001-use-postgresql",
        created["id"].as_str().unwrap(),
    ] {
        let shown = surface::show_core(&store, SEQ, &ShowParams { id: id.into() })
            .await
            .unwrap();
        assert_eq!(shown["reference"], "ADR-0001", "resolving {id:?}");
    }
    let missing = surface::show_core(
        &store,
        SEQ,
        &ShowParams {
            id: "ADR-0099".into(),
        },
    )
    .await;
    assert!(
        missing
            .unwrap_err()
            .to_string()
            .contains("no decision matches")
    );
}

#[tokio::test]
async fn the_lifecycle_oracle() {
    let store = FakeStore::default();
    create(&store, "First").await;

    // A proposal is decided…
    let r = set_status(&store, "1", "accepted").await.unwrap();
    assert_eq!(r["from"], "proposed");
    assert_eq!(r["to"], "accepted");
    assert_eq!(r["changed"], true);

    // …idempotently (no rewrite on a no-op)…
    let writes = store.write_count();
    let again = set_status(&store, "1", "accepted").await.unwrap();
    assert_eq!(again["changed"], false);
    assert_eq!(
        store.write_count(),
        writes,
        "a no-op must not rewrite the page"
    );

    // …and terminal states don't come back.
    for (from_setup, to) in [("accepted", "proposed"), ("accepted", "rejected")] {
        let err = set_status(&store, "1", to).await.unwrap_err().to_string();
        assert!(err.contains("cannot move"), "{from_setup}->{to}: {err}");
    }

    // Superseded is never set directly.
    let err = set_status(&store, "1", "superseded").await.unwrap_err();
    assert!(err.to_string().contains("supersede"), "{err}");

    // Unknown words are refused with the vocabulary.
    let err = set_status(&store, "1", "banana").await.unwrap_err();
    assert!(err.to_string().contains("unknown status"), "{err}");
}

#[tokio::test]
async fn supersede_links_both_sides_and_check_stays_clean() {
    let store = FakeStore::default();
    create(&store, "Old way").await;
    set_status(&store, "1", "accepted").await.unwrap();
    create(&store, "New way").await;
    set_status(&store, "2", "accepted").await.unwrap();

    let r = surface::supersede_core(
        &store,
        SEQ,
        &SupersedeParams {
            new: "2".into(),
            old: "1".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(r["old"]["status"], "superseded");

    // Both sides' frontmatter, by page id, statuses consistent.
    let old = surface::show_core(&store, SEQ, &ShowParams { id: "1".into() })
        .await
        .unwrap();
    let new = surface::show_core(&store, SEQ, &ShowParams { id: "2".into() })
        .await
        .unwrap();
    assert_eq!(old["status"], "superseded");
    assert_eq!(old["superseded_by"], new["id"]);
    assert_eq!(new["supersedes"], old["id"]);

    let check = surface::check_core(&store, SEQ).await.unwrap();
    assert_eq!(check["errors"], 0, "{check}");
    assert_eq!(check["warnings"], 0, "{check}");

    // Self-supersession is refused.
    let err = surface::supersede_core(
        &store,
        SEQ,
        &SupersedeParams {
            new: "2".into(),
            old: "2".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("cannot supersede itself"), "{err}");
}

#[tokio::test]
async fn check_flags_broken_and_inconsistent_supersession() {
    let store = FakeStore::default();
    create(&store, "Old way").await;
    create(&store, "New way").await;
    surface::supersede_core(
        &store,
        SEQ,
        &SupersedeParams {
            new: "2".into(),
            old: "1".into(),
        },
    )
    .await
    .unwrap();

    // Hand-break the new side: drop its supersedes back-pointer → warning.
    let text = store.page("decisions/0002-new-way");
    store.put(
        "decisions/0002-new-way",
        &text
            .lines()
            .filter(|l| !l.starts_with("supersedes:"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let check = surface::check_core(&store, SEQ).await.unwrap();
    assert_eq!(check["errors"], 0, "{check}");
    assert_eq!(check["warnings"], 1, "{check}");

    // Delete the new side entirely → the old side's ref dangles (error).
    store.remove("decisions/0002-new-way");
    let check = surface::check_core(&store, SEQ).await.unwrap();
    assert_eq!(check["errors"], 1, "{check}");

    // Hand-flip the old side's status while superseded_by is set → error.
    let text = store.page("decisions/0001-old-way");
    store.put(
        "decisions/0001-old-way",
        &text.replace("status: superseded", "status: accepted"),
    );
    let check = surface::check_core(&store, SEQ).await.unwrap();
    assert_eq!(check["errors"], 2, "{check}");
}

#[tokio::test]
async fn check_flags_duplicate_identities() {
    let store = FakeStore::default();
    create(&store, "Same title").await;
    // A byte-for-byte copy under another slug: duplicate reference, id, and
    // title in one page.
    let text = store.page("decisions/0001-same-title");
    store.put("decisions/9999-copy", &text);
    let check = surface::check_core(&store, SEQ).await.unwrap();
    assert_eq!(check["errors"], 2, "duplicate reference + id: {check}");
    assert_eq!(check["warnings"], 1, "duplicate title advises: {check}");
}

#[tokio::test]
async fn the_stored_plan_contract() {
    let store = FakeStore::default();
    create(&store, "Use PostgreSQL").await;

    // No stored plan yet: the provider-free read refuses, deterministically.
    let err = surface::plan_core(
        &store,
        SEQ,
        &PlanParams {
            id: "1".into(),
            save: false,
            text: None,
            file: None,
            force: false,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("no stored plan"), "{err}");

    // Save splices the marker-bracketed section; the envelope is the pinned
    // {reference, title, plan, stored} shape.
    let saved = surface::plan_core(
        &store,
        SEQ,
        &PlanParams {
            id: "1".into(),
            save: true,
            text: Some("1. Create the schema.\n2. Add tests.".into()),
            file: None,
            force: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        saved,
        json!({
            "reference": "ADR-0001",
            "title": "Use PostgreSQL",
            "plan": "1. Create the schema.\n2. Add tests.",
            "stored": true,
        })
    );
    let text = store.page("decisions/0001-use-postgresql");
    assert!(text.contains("<!-- adroit:plan -->"));
    assert!(text.contains("<!-- /adroit:plan -->"));

    // The read is deterministic and identical to what was saved.
    let read = surface::plan_core(
        &store,
        SEQ,
        &PlanParams {
            id: "ADR-0001".into(),
            save: false,
            text: None,
            file: None,
            force: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(read, saved);

    // Overwrite needs --force.
    let err = surface::plan_core(
        &store,
        SEQ,
        &PlanParams {
            id: "1".into(),
            save: true,
            text: Some("different".into()),
            file: None,
            force: false,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("--force"), "{err}");
    let forced = surface::plan_core(
        &store,
        SEQ,
        &PlanParams {
            id: "1".into(),
            save: true,
            text: Some("different".into()),
            file: None,
            force: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(forced["plan"], "different");
}

#[tokio::test]
async fn plan_never_overwrites_a_hand_written_section() {
    let store = FakeStore::default();
    surface::new_core(
        &store,
        SEQ,
        &NewParams {
            title: "Careful".into(),
            summary: None,
            body: Some("## Implementation\n\nBy hand, with love.".into()),
            body_file: None,
            relates: Vec::new(),
        },
    )
    .await
    .unwrap();
    let err = surface::plan_core(
        &store,
        SEQ,
        &PlanParams {
            id: "1".into(),
            save: true,
            text: Some("machine plan".into()),
            file: None,
            force: false,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("hand-written"), "{err}");
}

#[tokio::test]
async fn review_dates_set_clear_and_flag_due() {
    let store = FakeStore::default();
    create(&store, "First").await;

    let r = surface::set_review_core(
        &store,
        SEQ,
        &SetReviewParams {
            id: "1".into(),
            date: Some("2020-01-01".into()),
            clear: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(r["review_by"], "2020-01-01");

    // A still-proposed decision past its deadline is review-due in list.
    let list = surface::list_core(&store, SEQ, &ListParams { status: None })
        .await
        .unwrap();
    assert_eq!(list["decisions"][0]["review_due"], true, "{list}");

    let r = surface::set_review_core(
        &store,
        SEQ,
        &SetReviewParams {
            id: "1".into(),
            date: None,
            clear: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(r["review_by"], Value::Null);
    let text = store.page("decisions/0001-first");
    assert!(!text.contains("review_by:"));
}

#[tokio::test]
async fn list_filters_by_status() {
    let store = FakeStore::default();
    create(&store, "First").await;
    create(&store, "Second").await;
    set_status(&store, "2", "accepted").await.unwrap();

    let accepted = surface::list_core(
        &store,
        SEQ,
        &ListParams {
            status: Some("accepted".into()),
        },
    )
    .await
    .unwrap();
    let rows = accepted["decisions"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["reference"], "ADR-0002");
}

#[tokio::test]
async fn foreign_frontmatter_survives_every_rewrite() {
    // The sacred round-trip, exercised through the verbs (not just the
    // serializer): keys adroit doesn't own survive set-status, set-review,
    // supersede, and plan --save untouched.
    let store = FakeStore::default();
    create(&store, "First").await;
    create(&store, "Second").await;

    let slug = "decisions/0001-first";
    let foreign = "citations:\n- evidence/kb-chat.md@abc123\nkb_meta:\n  confidence: 0.9\n";
    let text = store.page(slug);
    let close = text.rfind("\n---\n").unwrap();
    store.put(
        slug,
        &format!("{}{}{}", &text[..close + 1], foreign, &text[close + 1..]),
    );

    set_status(&store, "1", "accepted").await.unwrap();
    surface::set_review_core(
        &store,
        SEQ,
        &SetReviewParams {
            id: "1".into(),
            date: Some("2030-01-01".into()),
            clear: false,
        },
    )
    .await
    .unwrap();
    surface::plan_core(
        &store,
        SEQ,
        &PlanParams {
            id: "1".into(),
            save: true,
            text: Some("1. Step.".into()),
            file: None,
            force: false,
        },
    )
    .await
    .unwrap();
    surface::supersede_core(
        &store,
        SEQ,
        &SupersedeParams {
            new: "2".into(),
            old: "1".into(),
        },
    )
    .await
    .unwrap();

    let after = store.page(slug);
    assert!(
        after.contains("citations:\n- evidence/kb-chat.md@abc123"),
        "{after}"
    );
    assert!(after.contains("kb_meta:\n  confidence: 0.9"), "{after}");
    assert!(after.contains("status: superseded"));
    assert!(after.contains("review_by: 2030-01-01"));
}

#[tokio::test]
async fn lint_gates_an_unfinished_draft() {
    let store = FakeStore::default();
    surface::new_core(
        &store,
        SEQ,
        &NewParams {
            title: "Draft".into(),
            summary: None,
            body: Some(
                "## Context and Problem Statement\n\n_Describe the problem._\n\n\
                 ## Considered Options\n\n1. Only one\n"
                    .into(),
            ),
            body_file: None,
            relates: Vec::new(),
        },
    )
    .await
    .unwrap();
    let r = surface::lint_core(&store, SEQ, &LintParams { id: "1".into() })
        .await
        .unwrap();
    assert!(r["errors"].as_u64().unwrap() > 0, "{r}");
}

#[tokio::test]
async fn edit_refines_proposed_decisions_only() {
    let store = FakeStore::default();
    create(&store, "First").await;

    // The refine seat: replace the body wholesale; frontmatter (and foreign
    // keys) survive by construction.
    let slug = "decisions/0001-first";
    let foreign = "citations:\n- evidence/kb-chat.md@abc123\n";
    let text = store.page(slug);
    let close = text.rfind("\n---\n").unwrap();
    store.put(
        slug,
        &format!("{}{}{}", &text[..close + 1], foreign, &text[close + 1..]),
    );

    let r = surface::edit_core(
        &store,
        SEQ,
        &adroit::surface::EditParams {
            id: "1".into(),
            body: Some("## Context and Problem Statement\n\nRefined.".into()),
            body_file: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(r["reference"], "ADR-0001");
    assert_eq!(r["status"], "proposed");
    let after = store.page(slug);
    assert!(after.contains("Refined."));
    assert!(
        after.contains("citations:\n- evidence/kb-chat.md@abc123"),
        "{after}"
    );

    // A body is required…
    let err = surface::edit_core(
        &store,
        SEQ,
        &adroit::surface::EditParams {
            id: "1".into(),
            body: None,
            body_file: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("--body"), "{err}");

    // …and decided records don't get edited — supersede them.
    set_status(&store, "1", "accepted").await.unwrap();
    let err = surface::edit_core(
        &store,
        SEQ,
        &adroit::surface::EditParams {
            id: "1".into(),
            body: Some("rewrite history".into()),
            body_file: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("supersede"), "{err}");
}

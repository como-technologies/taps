//! Producer-side export-contract gate.
//!
//! Builds one small, fully pinned assessment (fixed UUIDs and timestamps —
//! nothing random), exports it through the real `ExportService::to_data`
//! pipeline (the same code path the CLI `export` command and the HTTP
//! export route serialize through), and pins the result two ways:
//!
//! 1. **Schema:** the exported document round-trips through
//!    `ExportService::validate_and_parse`, i.e. what we export always
//!    satisfies the published JSON Schema (`amaker schema`).
//! 2. **Bytes:** the exported YAML is identical to the vendored golden at
//!    `../../../contract/fixtures/golden-assessment.yaml`, so any change to the export
//!    shape surfaces as a reviewed diff to that file — never silent drift
//!    for downstream consumers (adroit's `import --from-assessment` seam).
//!
//! If a contract change is intentional, regenerate the fixture (the failure
//! message writes the current bytes next to the golden) and commit the diff.

use std::path::PathBuf;

use amaker_core::models::assessment::{Assessment, Domain, Practice, Question};
use amaker_core::models::{EffortRange, Polarity};
use amaker_core::services::{DataFormat, ExportService};

fn pinned_time() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .expect("pinned timestamp parses")
        .with_timezone(&chrono::Utc)
}

fn id<T: From<uuid::Uuid>>(s: &str) -> T {
    T::from(uuid::Uuid::parse_str(s).expect("pinned uuid parses"))
}

/// One minimal assessment exercising every level of the metamodel —
/// domain, practice, positive and negative questions, optional question
/// metadata (guidance / evidence / remediation / roles / effort), and the
/// default controlled vocabularies (via the real `Assessment::new`).
/// Every ID and timestamp is pinned, so the export is deterministic.
fn pinned_assessment() -> Assessment {
    let mut q1 =
        Question::new("Does every change build and run the test suite in CI before merge?".into());
    q1.id = id("a55e55ed-0000-4000-8000-0000000000e1");

    let mut q2 = Question::new("Are releases routinely delayed by manual verification?".into());
    q2.id = id("a55e55ed-0000-4000-8000-0000000000e2");
    q2.polarity = Polarity::Negative;
    q2.guidance = Some("Ask the team when the last release slipped and why".into());
    q2.evidence = Some("Release calendar versus actual ship dates".into());
    q2.remediation = Some("Automate the manual checks that block releases most often".into());
    q2.roles = vec!["engineer".into(), "tech-lead".into()];
    q2.effort = Some(EffortRange::new(8, 40));

    let mut practice = Practice::new(
        "Continuous Integration".into(),
        "Every change is built and tested automatically".into(),
        "Defects surface minutes after they are introduced".into(),
        "Broken builds discovered at release time".into(),
    );
    practice.id = id("a55e55ed-0000-4000-8000-0000000000c0");
    practice.guidance = Some("Start with a single pipeline that gates merges".into());
    practice.questions = vec![q1, q2];

    let mut domain = Domain::new(
        "Delivery".into(),
        "How changes get from a developer's machine to production".into(),
        "Predictable, low-friction releases".into(),
        "Slow, error-prone releases that block the business".into(),
    );
    domain.id = id("a55e55ed-0000-4000-8000-0000000000d0");
    domain.practices = vec![practice];

    let mut assessment = Assessment::new(
        "Release Readiness Sample".into(),
        "A minimal sample assessment of software release readiness".into(),
        "Exercise every level of the export contract with one example".into(),
    );
    assessment.id = id("a55e55ed-0000-4000-8000-0000000000a0");
    assessment.domains = vec![domain];
    assessment.created_at = pinned_time();
    assessment.updated_at = pinned_time();
    assessment
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../contract/fixtures/golden-assessment.yaml")
}

/// The export validates against the published JSON Schema (round-trips
/// through the same `validate_and_parse` the CLI `validate` command uses)
/// in every format.
#[test]
fn export_validates_against_the_published_schema() {
    let assessment = pinned_assessment();
    for format in [DataFormat::Yaml, DataFormat::Json, DataFormat::Toml] {
        let exported = ExportService::to_data(&assessment, format)
            .unwrap_or_else(|e| panic!("export to {format:?} failed: {e}"));
        let parsed = ExportService::validate_and_parse(&exported, format)
            .unwrap_or_else(|e| panic!("{format:?} export failed schema validation: {e}"));
        assert_eq!(parsed.name, assessment.name);
        assert_eq!(parsed.question_count(), assessment.question_count());
    }
}

/// The YAML export (the format the Assess→Prescribe seam ships) is
/// byte-identical to the vendored golden fixture.
#[test]
fn yaml_export_matches_the_vendored_golden() {
    let exported = ExportService::to_data(&pinned_assessment(), DataFormat::Yaml)
        .expect("YAML export succeeds");
    let golden = std::fs::read_to_string(golden_path()).unwrap_or_else(|e| {
        panic!(
            "cannot read {} ({e}) — the vendored golden must be committed",
            golden_path().display()
        )
    });
    if exported != golden {
        let actual = golden_path().with_extension("actual.yaml");
        std::fs::write(&actual, &exported).ok();
        panic!(
            "export drifted from the vendored golden.\n\
             If the contract change is intentional, review and replace\n\
             {} with {} (written just now), and note the change for\n\
             downstream consumers of the export.",
            golden_path().display(),
            actual.display()
        );
    }
}

//! Output renderers: canonical JSON (`-o json`) and a compact human table,
//! each in single-month (ADR-0004) and multi-month range (ADR-0007) form.

use crate::window::YearMonth;
use std::collections::BTreeMap;
use tuesday_core::MonthlyReport;

/// The canonical machine-readable form: the serde `MonthlyReport`,
/// pretty-printed with sorted object keys. Identical bytes to the schema
/// pinned by tuesday-core's `monthly_report.json` fixture (ADR-0004):
/// converting through `serde_json::Value` (BTreeMap-backed in this build)
/// sorts keys, so HashMap iteration order never leaks into the output.
pub fn render_json(report: &MonthlyReport) -> String {
    let value = serde_json::to_value(report).expect("MonthlyReport serializes");
    serde_json::to_string_pretty(&value).expect("Value renders")
}

/// The cross-month ADR rollup: full credit per decision (ADR-0005), summed
/// across every month of the range. BTreeMap so iteration is sorted.
fn range_adr_totals(reports: &[MonthlyReport]) -> BTreeMap<String, f64> {
    let mut totals = BTreeMap::new();
    for report in reports {
        for (adr, hours) in &report.adr_totals {
            *totals.entry(adr.clone()).or_insert(0.0) += hours;
        }
    }
    totals
}

/// The canonical multi-month form (ADR-0007): an **additive envelope**
/// around per-month reports — `from`/`to` echo the inclusive window,
/// `reports` carries one **unchanged** canonical `MonthlyReport` per month
/// (the ADR-0004 schema, bit-for-bit what single-month `-o json` emits for
/// that month), and `adr_totals` is the derived cross-month rollup. Keys
/// are sorted by the same `serde_json::Value` path as [`render_json`].
pub fn render_range_json(from: YearMonth, to: YearMonth, reports: &[MonthlyReport]) -> String {
    let value = serde_json::json!({
        "from": from.to_string(),
        "to": to.to_string(),
        "reports": reports,
        "adr_totals": range_adr_totals(reports),
    });
    serde_json::to_string_pretty(&value).expect("range envelope renders")
}

/// The multi-month human form: the range header, one sectioned per-month
/// table (the single-month renderer, unchanged), and the cross-month ADR
/// rollup the JSON envelope carries.
pub fn render_range_table(from: YearMonth, to: YearMonth, reports: &[MonthlyReport]) -> String {
    let mut out = format!("Range {from} — {to}: {} month(s)\n", reports.len());

    for report in reports {
        out.push('\n');
        out.push_str("────────────────────────────────────────\n");
        out.push_str(&render_table(report));
    }

    let totals = range_adr_totals(reports);
    if !totals.is_empty() {
        out.push('\n');
        out.push_str("────────────────────────────────────────\n");
        out.push_str("ADR totals across the range (full credit per decision):\n");
        for (adr, hours) in &totals {
            out.push_str(&format!("  {adr:<12} {hours:>7.1} h\n"));
        }
    }

    out
}

/// The compact human table (the default output mode).
pub fn render_table(report: &MonthlyReport) -> String {
    let mut out = String::new();
    let push = |out: &mut String, line: String| {
        out.push_str(&line);
        out.push('\n');
    };

    push(
        &mut out,
        format!(
            "{} {} — {}: {:.1} monthly hours over {} effort points ({:.2} h/point)",
            report.month,
            report.year,
            report.organization,
            report.total_hours,
            report.total_effort_points,
            report.hours_per_point,
        ),
    );

    if !report.allocations.is_empty() {
        push(&mut out, String::new());
        push(
            &mut out,
            format!(
                "{:<20} {:>6}  {:>3}  {:>7}  {:>6}  {:<10}  TITLE",
                "REPO", "PR", "EFF", "HOURS", "%", "ADR"
            ),
        );
        for allocation in &report.allocations {
            push(
                &mut out,
                format!(
                    "{:<20} {:>6}  {:>3}  {:>7.1}  {:>5.1}%  {:<10}  {}",
                    allocation.repository,
                    format!("#{}", allocation.pr_number),
                    allocation.effort_score.value(),
                    allocation.allocated_hours,
                    allocation.percentage_of_total,
                    allocation.adr_id.as_deref().unwrap_or("-"),
                    allocation.pr_title,
                ),
            );
        }
    }

    let sorted = |totals: &std::collections::HashMap<String, f64>| {
        let mut entries: Vec<(String, f64)> = totals.iter().map(|(k, v)| (k.clone(), *v)).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    };

    if !report.adr_totals.is_empty() {
        push(&mut out, String::new());
        push(
            &mut out,
            "ADR totals (full credit per decision):".to_string(),
        );
        for (adr, hours) in sorted(&report.adr_totals) {
            push(&mut out, format!("  {adr:<12} {hours:>7.1} h"));
        }
    }

    if !report.category_totals.is_empty() {
        push(&mut out, String::new());
        push(&mut out, "Category totals:".to_string());
        for (category, hours) in sorted(&report.category_totals) {
            push(&mut out, format!("  {category:<16} {hours:>7.1} h"));
        }
    }

    if !report.unallocated_prs.is_empty() {
        push(&mut out, String::new());
        push(&mut out, "Unallocated PRs (no effort label):".to_string());
        for (repo, number, title) in &report.unallocated_prs {
            push(&mut out, format!("  {repo} #{number} {title}"));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::make_pr;
    use crate::report::build_report;
    use tuesday_core::ScalingSeries;

    /// The exact PR set behind tuesday-core's pinned fixture
    /// (`crates/tuesday-core/tests/fixtures/monthly_report.json`).
    fn fixture_report() -> MonthlyReport {
        let alpha = vec![
            make_pr(
                101,
                "Implement the parser",
                None,
                &[
                    "effort:5-felt-like-forever",
                    "adr:ADR-0007",
                    "feature",
                    "parser",
                ],
            ),
            make_pr(
                102,
                "Fix parser edge case",
                Some("Closes the loop.\n\nAdr-Reference: ADR-0007"),
                &["effort:3-average", "bug-fix"],
            ),
            make_pr(
                103,
                "Tidy module docs",
                None,
                &["effort:1-super-quick", "adr:ADR-0009"],
            ),
        ];
        let beta = vec![
            make_pr(
                201,
                "Refactor config loading",
                None,
                &["effort:2-not-long", "refactor"],
            ),
            make_pr(204, "Spike: label experiment", None, &["experiment"]),
        ];
        build_report(
            vec![("alpha".to_string(), alpha), ("beta".to_string(), beta)],
            "como",
            2026,
            1,
            360.0,
            ScalingSeries::Linear,
        )
        .unwrap()
    }

    #[test]
    fn json_output_is_byte_identical_to_the_pinned_core_schema() {
        // THE canonical MonthlyReport: the CLI must emit the exact bytes of
        // the byte-compat-pinned schema shared with the web export head.
        let json = render_json(&fixture_report());
        let pinned = include_str!("../../tuesday-core/tests/fixtures/monthly_report.json");
        assert_eq!(json, pinned.trim_end());
    }

    #[test]
    fn json_output_parses_and_exposes_adr_totals() {
        let value: serde_json::Value =
            serde_json::from_str(&render_json(&fixture_report())).unwrap();
        assert_eq!(value["adr_totals"]["ADR-0007"], 261.8181818181818);
        assert_eq!(value["adr_totals"]["ADR-0009"], 32.72727272727273);
        assert_eq!(value["allocations"][0]["adr_id"], "ADR-0007");
    }

    #[test]
    fn table_shows_header_allocations_rollups_and_qc_list() {
        let table = render_table(&fixture_report());

        // Header: period, org, the hour budget.
        assert!(table.contains("January 2026"), "header period:\n{table}");
        assert!(table.contains("como"), "header org:\n{table}");
        assert!(table.contains("360"), "header monthly hours:\n{table}");

        // One row per allocation: PR number, hours, ADR attribution.
        assert!(table.contains("#101"), "allocation row:\n{table}");
        assert!(table.contains("163.6"), "allocated hours:\n{table}");

        // The ADR rollup is a first-class section (ADR-0005).
        assert!(table.contains("ADR totals"), "adr section:\n{table}");
        assert!(table.contains("261.8"), "adr full credit:\n{table}");

        // Category rollup and the QC list of unallocated PRs.
        assert!(
            table.contains("Category totals"),
            "category section:\n{table}"
        );
        assert!(table.contains("bug-fix"), "category name:\n{table}");
        assert!(table.contains("Unallocated"), "qc section:\n{table}");
        assert!(table.contains("#204"), "unallocated PR:\n{table}");
    }

    #[test]
    fn table_rollups_are_sorted_for_deterministic_output() {
        // HashMap iteration order must not leak into the table.
        let table = render_table(&fixture_report());
        let adr_0007 = table.find("ADR-0007").unwrap();
        let adr_0009 = table.find("ADR-0009").unwrap();
        assert!(adr_0007 < adr_0009, "ADR rollup sorted by id:\n{table}");

        let bug_fix = table.find("bug-fix").unwrap();
        let feature = table.rfind("feature").unwrap();
        assert!(bug_fix < feature, "categories sorted by name:\n{table}");
    }

    // --- the multi-month range forms (ADR-0007) ---

    fn ym(year: u32, month: u32) -> YearMonth {
        YearMonth { year, month }
    }

    /// Two months sharing ADR-0007 (so the rollup must sum) across a year
    /// boundary; January is the pinned fixture month.
    fn range_reports() -> Vec<MonthlyReport> {
        let december = build_report(
            vec![(
                "alpha".to_string(),
                vec![make_pr(
                    90,
                    "December decision work",
                    None,
                    &["effort:2-not-long", "adr:ADR-0007"],
                )],
            )],
            "como",
            2025,
            12,
            360.0,
            ScalingSeries::Linear,
        )
        .unwrap();
        vec![december, fixture_report()]
    }

    #[test]
    fn range_json_is_the_additive_envelope_around_unchanged_reports() {
        let reports = range_reports();
        let json = render_range_json(ym(2025, 12), ym(2026, 1), &reports);
        let envelope: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(envelope["from"], "2025-12");
        assert_eq!(envelope["to"], "2026-01");

        // Each element is the UNCHANGED canonical report: semantically
        // identical to what render_json emits for that month.
        for (index, report) in reports.iter().enumerate() {
            let single: serde_json::Value = serde_json::from_str(&render_json(report)).unwrap();
            assert_eq!(envelope["reports"][index], single);
        }
    }

    #[test]
    fn range_json_rolls_adr_totals_up_across_months() {
        let json = render_range_json(ym(2025, 12), ym(2026, 1), &range_reports());
        let envelope: serde_json::Value = serde_json::from_str(&json).unwrap();

        // ADR-0007: 360.0 (December, sole PR) + 261.81…(January fixture).
        assert_eq!(
            envelope["adr_totals"]["ADR-0007"],
            360.0 + 261.8181818181818
        );
        // ADR-0009 appears in January only — carried through unchanged.
        assert_eq!(envelope["adr_totals"]["ADR-0009"], 32.72727272727273);
    }

    #[test]
    fn range_json_keys_are_sorted_for_deterministic_output() {
        let json = render_range_json(ym(2025, 12), ym(2026, 1), &range_reports());
        let adr_totals = json.find("\"adr_totals\"").unwrap();
        let from = json.find("\"from\"").unwrap();
        let reports = json.find("\"reports\"").unwrap();
        let to = json.find("\"to\"").unwrap();
        assert!(
            adr_totals < from && from < reports && reports < to,
            "envelope keys sorted:\n{json}"
        );
    }

    #[test]
    fn empty_range_months_keep_their_place_in_the_envelope() {
        let empty = build_report(
            vec![("alpha".to_string(), Vec::new())],
            "como",
            2026,
            2,
            360.0,
            ScalingSeries::Linear,
        )
        .unwrap();
        let json = render_range_json(ym(2026, 2), ym(2026, 2), &[empty]);
        let envelope: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope["reports"].as_array().unwrap().len(), 1);
        assert_eq!(envelope["reports"][0]["month"], "February");
        assert_eq!(envelope["adr_totals"], serde_json::json!({}));
    }

    #[test]
    fn range_table_sections_every_month_and_appends_the_rollup() {
        let table = render_range_table(ym(2025, 12), ym(2026, 1), &range_reports());

        assert!(
            table.contains("Range 2025-12 — 2026-01: 2 month(s)"),
            "{table}"
        );
        assert!(table.contains("December 2025"), "first section:\n{table}");
        assert!(table.contains("January 2026"), "second section:\n{table}");
        assert!(
            table.contains("ADR totals across the range"),
            "rollup section:\n{table}"
        );
        // The cross-range rollup shows the summed figure.
        assert!(table.contains("621.8"), "summed ADR-0007 hours:\n{table}");

        // Rollup is sorted by ADR id.
        let rollup = &table[table.find("across the range").unwrap()..];
        let adr_0007 = rollup.find("ADR-0007").unwrap();
        let adr_0009 = rollup.find("ADR-0009").unwrap();
        assert!(adr_0007 < adr_0009, "rollup sorted:\n{rollup}");
    }

    #[test]
    fn table_marks_allocations_without_an_adr() {
        let report = build_report(
            vec![(
                "alpha".to_string(),
                vec![make_pr(
                    1,
                    "No decision",
                    None,
                    &["effort:1-super-quick", "chore"],
                )],
            )],
            "como",
            2026,
            1,
            360.0,
            ScalingSeries::Linear,
        )
        .unwrap();
        let table = render_table(&report);
        assert!(table.contains('-'), "missing ADR shown as a dash:\n{table}");
        assert!(
            !table.contains("ADR totals"),
            "no empty ADR section:\n{table}"
        );
    }
}

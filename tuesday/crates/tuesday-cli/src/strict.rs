//! The `--strict` contract check (ADR-0005, portfolio referee ruling).
//!
//! Strict mode passes a merged PR iff it carries **exactly one** valid
//! `effort:N-*` label AND (at least one category label OR an `adr:*` label).
//! ADR-labeled PRs count as allocated even with no category label — ADR
//! attribution is itself an allocation of capacity to a decision. Category
//! labels are everything outside the structural prefixes (`effort:`,
//! `adr:`, `conduit:`). Note the `Adr-Reference:` body trailer does NOT
//! satisfy strict mode: the conduit contract requires the label to be final
//! at merge; the trailer is only a tolerant attribution fallback in the
//! report itself.

use tuesday_core::MergedPr;

/// A merged PR that fails the strict contract, with every reason it fails.
#[derive(Debug, Clone, PartialEq)]
pub struct StrictViolation {
    pub repository: String,
    pub pr_number: u64,
    pub pr_title: String,
    pub problems: Vec<String>,
}

/// The closed five-value effort enum from the conduit contract table.
const VALID_EFFORT_LABELS: [&str; 5] = [
    "effort:1-super-quick",
    "effort:2-not-long",
    "effort:3-average",
    "effort:4-a-while",
    "effort:5-felt-like-forever",
];

/// Check every merged PR against the strict rule, in input order.
pub fn check_strict(repo_prs: &[(String, Vec<MergedPr>)]) -> Vec<StrictViolation> {
    let mut violations = Vec::new();

    for (repository, prs) in repo_prs {
        for pr in prs {
            let problems = check_pr(pr);
            if !problems.is_empty() {
                violations.push(StrictViolation {
                    repository: repository.clone(),
                    pr_number: pr.number,
                    pr_title: pr.title.clone(),
                    problems,
                });
            }
        }
    }

    violations
}

fn check_pr(pr: &MergedPr) -> Vec<String> {
    let mut problems = Vec::new();

    // Exactly one effort label, and it must be one of the five valid scores.
    let effort_labels: Vec<&String> = pr
        .labels
        .iter()
        .filter(|label| label.starts_with("effort:"))
        .collect();
    match effort_labels.as_slice() {
        [] => problems.push("no effort:N-* label".to_string()),
        [label] if !VALID_EFFORT_LABELS.contains(&label.as_str()) => {
            problems.push(format!("unknown effort label `{label}`"));
        }
        [_] => {}
        many => problems.push(format!(
            "{} effort labels (expected exactly one): {}",
            many.len(),
            many.iter()
                .map(|label| label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }

    // Allocated: at least one category label OR an adr:* label.
    let has_adr = pr.labels.iter().any(|label| label.starts_with("adr:"));
    let has_category = pr.labels.iter().any(|label| {
        !label.starts_with("effort:")
            && !label.starts_with("adr:")
            && !label.starts_with("conduit:")
    });
    if !has_adr && !has_category {
        problems.push("no category label and no adr:* label".to_string());
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::make_pr;

    fn check_one(labels: &[&str]) -> Vec<StrictViolation> {
        let pr = make_pr(7, "Add widget", None, labels);
        check_strict(&[("alpha".to_string(), vec![pr])])
    }

    #[test]
    fn effort_plus_category_label_passes() {
        assert_eq!(check_one(&["effort:3-average", "feature"]), Vec::new());
    }

    #[test]
    fn effort_plus_adr_label_passes_with_no_category() {
        // The referee ruling: adr-labeled PRs COUNT as allocated.
        assert_eq!(
            check_one(&["effort:1-super-quick", "adr:ADR-0001"]),
            Vec::new()
        );
        // ... including when conduit machinery labels ride along.
        assert_eq!(
            check_one(&["effort:1-super-quick", "adr:ADR-0001", "conduit:task"]),
            Vec::new()
        );
    }

    #[test]
    fn missing_effort_label_is_a_violation() {
        let violations = check_one(&["feature"]);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].pr_number, 7);
        assert!(
            violations[0].problems.iter().any(|p| p.contains("effort")),
            "problems should name the missing effort label: {violations:?}"
        );
    }

    #[test]
    fn multiple_effort_labels_are_a_violation() {
        let violations = check_one(&["effort:2-not-long", "effort:5-felt-like-forever", "feature"]);
        assert_eq!(violations.len(), 1);
        assert!(
            violations[0]
                .problems
                .iter()
                .any(|p| p.contains("2 effort labels")),
            "problems should count the effort labels: {violations:?}"
        );
    }

    #[test]
    fn unknown_effort_label_is_a_violation() {
        let violations = check_one(&["effort:9-instant", "feature"]);
        assert_eq!(violations.len(), 1);
        assert!(
            violations[0]
                .problems
                .iter()
                .any(|p| p.contains("effort:9-instant")),
            "problems should name the unknown label: {violations:?}"
        );
    }

    #[test]
    fn structural_only_labels_are_a_violation() {
        // effort + conduit:* machinery, no category, no adr:* — unallocated.
        let violations = check_one(&["effort:3-average", "conduit:task"]);
        assert_eq!(violations.len(), 1);
        assert!(
            violations[0]
                .problems
                .iter()
                .any(|p| p.contains("category") && p.contains("adr:")),
            "problems should state the allocation rule: {violations:?}"
        );
    }

    #[test]
    fn body_trailer_does_not_satisfy_strict() {
        // The contract requires the adr:* LABEL final at merge; the trailer
        // is only the report's tolerant attribution fallback.
        let pr = make_pr(
            8,
            "Follow-up",
            Some("Cleanup.\n\nAdr-Reference: ADR-0001"),
            &["effort:1-super-quick"],
        );
        let violations = check_strict(&[("alpha".to_string(), vec![pr])]);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_pr_failing_both_halves_reports_both_problems() {
        let violations = check_one(&["conduit:task"]);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].problems.len(), 2);
    }

    #[test]
    fn violations_carry_repo_number_and_title_in_input_order() {
        let alpha = vec![
            make_pr(1, "Good", None, &["effort:1-super-quick", "adr:ADR-0001"]),
            make_pr(2, "No effort", None, &["feature"]),
        ];
        let beta = vec![make_pr(3, "No allocation", None, &["effort:2-not-long"])];
        let violations = check_strict(&[("alpha".to_string(), alpha), ("beta".to_string(), beta)]);

        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].repository, "alpha");
        assert_eq!(violations[0].pr_number, 2);
        assert_eq!(violations[0].pr_title, "No effort");
        assert_eq!(violations[1].repository, "beta");
        assert_eq!(violations[1].pr_number, 3);
    }

    #[test]
    fn empty_month_has_no_violations() {
        assert_eq!(
            check_strict(&[("alpha".to_string(), Vec::new())]),
            Vec::new()
        );
    }
}

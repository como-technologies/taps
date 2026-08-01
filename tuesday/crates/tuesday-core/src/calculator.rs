use crate::domain::MergedPr;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Time tracking and effort calculation types

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ScalingSeries {
    #[default]
    Linear, // 1, 2, 3, 4, 5
    Doubling,    // 1, 2, 4, 8, 16
    Fibonacci,   // 1, 2, 3, 5, 8
    Exponential, // 1, 2, 4, 8, 16 (same as doubling but could be 1, 3, 9, 27, 81)
    TShirtSizes, // 1, 3, 5, 8, 13 (XS, S, M, L, XL mapping)
    Square,      // 1, 4, 9, 16, 25
}

impl ScalingSeries {
    pub fn get_values(&self) -> [f64; 5] {
        match self {
            ScalingSeries::Linear => [1.0, 2.0, 3.0, 4.0, 5.0],
            ScalingSeries::Doubling => [1.0, 2.0, 4.0, 8.0, 16.0],
            ScalingSeries::Fibonacci => [1.0, 2.0, 3.0, 5.0, 8.0],
            ScalingSeries::Exponential => [1.0, 3.0, 9.0, 27.0, 81.0],
            ScalingSeries::TShirtSizes => [1.0, 3.0, 5.0, 8.0, 13.0],
            ScalingSeries::Square => [1.0, 4.0, 9.0, 16.0, 25.0],
        }
    }

    pub fn description(&self) -> &str {
        match self {
            ScalingSeries::Linear => "Linear (1, 2, 3, 4, 5)",
            ScalingSeries::Doubling => "Doubling (1, 2, 4, 8, 16)",
            ScalingSeries::Fibonacci => "Fibonacci (1, 2, 3, 5, 8)",
            ScalingSeries::Exponential => "Exponential (1, 3, 9, 27, 81)",
            ScalingSeries::TShirtSizes => "T-Shirt Sizes (1, 3, 5, 8, 13)",
            ScalingSeries::Square => "Square (1, 4, 9, 16, 25)",
        }
    }
}

impl std::fmt::Display for ScalingSeries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EffortScore {
    #[default]
    SuperQuick = 1,
    NotLong = 2,
    Average = 3,
    AWhile = 4,
    FeltLikeForever = 5,
}

impl EffortScore {
    pub fn value(&self) -> u32 {
        *self as u32
    }

    pub fn scaled_value(&self, series: ScalingSeries) -> f64 {
        let values = series.get_values();
        let index = (*self as u32 - 1) as usize;
        values[index]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeAllocation {
    pub pr_number: u32,
    pub pr_title: String,
    pub repository: String, // Repository name for multi-repo support
    pub effort_score: EffortScore,
    pub allocated_hours: f64,
    pub percentage_of_total: f64,
    pub categories: HashMap<String, f64>,
    pub adr_id: Option<String>, // ADR reference this PR's work is attributed to
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonthlyReport {
    pub month: String,
    pub year: u32,
    pub total_hours: f64,
    pub total_effort_points: u32,
    pub hours_per_point: f64,
    pub allocations: Vec<TimeAllocation>,
    pub category_totals: HashMap<String, f64>,
    pub adr_totals: HashMap<String, f64>, // Full allocated hours per ADR reference
    pub unallocated_hours: f64,           // Hours from PRs without effort scores
    pub unallocated_prs: Vec<(String, u32, String)>, // List of (repo_name, pr_number, pr_title) with no allocation
    pub organization: String,                        // Organization name for building PR links
}

impl MonthlyReport {
    pub fn new(month: String, year: u32, total_hours: f64, organization: String) -> Self {
        Self {
            month,
            year,
            total_hours,
            total_effort_points: 0,
            hours_per_point: 0.0,
            allocations: Vec::new(),
            category_totals: HashMap::new(),
            adr_totals: HashMap::new(),
            unallocated_hours: 0.0,
            unallocated_prs: Vec::new(),
            organization,
        }
    }
}

pub struct EffortCalculator {
    monthly_hours: f64,
    scaling_series: ScalingSeries,
}

impl EffortCalculator {
    pub fn new(monthly_hours: f64, scaling_series: ScalingSeries) -> Self {
        tracing::info!(
            "EffortCalculator initialized with {} monthly hours",
            monthly_hours
        );

        Self {
            monthly_hours,
            scaling_series,
        }
    }

    // Extract effort score from PR labels
    fn extract_effort_from_pr(&self, pr: &MergedPr) -> Option<EffortScore> {
        let mut effort_labels = Vec::new();

        for label in &pr.labels {
            if label.starts_with("effort:") {
                effort_labels.push(label);
            }
        }

        if effort_labels.is_empty() {
            return None;
        }

        if effort_labels.len() > 1 {
            tracing::warn!(
                "PR #{}: Multiple effort labels found: {:?}, using first one",
                pr.number,
                effort_labels
            );
        }

        // Parse the effort score from the label name
        match effort_labels[0].as_str() {
            "effort:1-super-quick" => Some(EffortScore::SuperQuick),
            "effort:2-not-long" => Some(EffortScore::NotLong),
            "effort:3-average" => Some(EffortScore::Average),
            "effort:4-a-while" => Some(EffortScore::AWhile),
            "effort:5-felt-like-forever" => Some(EffortScore::FeltLikeForever),
            label => {
                tracing::warn!("PR #{}: Unknown effort label format: {}", pr.number, label);
                None
            }
        }
    }

    // Extract ADR reference from PR labels, falling back to the body trailer.
    // Primary source: first `adr:<reference>` label (e.g. `adr:ADR-0012`).
    // Fallback: a body trailer line `Adr-Reference: <reference>`.
    fn extract_adr_from_pr(&self, pr: &MergedPr) -> Option<String> {
        let mut adr_labels = Vec::new();

        for label in &pr.labels {
            if let Some(reference) = label.strip_prefix("adr:") {
                adr_labels.push(reference);
            }
        }

        if adr_labels.len() > 1 {
            tracing::warn!(
                "PR #{}: Multiple ADR labels found: {:?}, using first one",
                pr.number,
                adr_labels
            );
        }

        if let Some(reference) = adr_labels.first() {
            return Some(reference.to_string());
        }

        // Fallback: scan the PR body for an `Adr-Reference: <reference>` trailer line
        let body = pr.body.as_deref()?;
        for line in body.lines() {
            if let Some(rest) = line.strip_prefix("Adr-Reference:")
                && let Some(reference) = rest.split_whitespace().next()
            {
                return Some(reference.to_string());
            }
        }

        None
    }

    // Extract categories from the PR's labels. Structural labels are
    // attribution/machinery, not categories (ADR-0005): effort:* carries the
    // score, adr:* carries decision attribution, conduit:* is task plumbing.
    fn extract_categories_from_pr(&self, pr: &MergedPr) -> Vec<String> {
        let categories: Vec<String> = pr
            .labels
            .iter()
            .filter(|label| {
                !label.starts_with("effort:")
                    && !label.starts_with("adr:")
                    && !label.starts_with("conduit:")
            })
            .cloned()
            .collect();

        if categories.is_empty() {
            vec!["Uncategorized".to_string()]
        } else {
            categories
        }
    }

    pub fn calculate_report(
        &self,
        repo_prs: Vec<(String, Vec<MergedPr>)>,
        month: String,
        year: u32,
        organization: String,
    ) -> MonthlyReport {
        let mut report = MonthlyReport::new(month, year, self.monthly_hours, organization);

        // Flatten repo_prs into (repo_name, pr) tuples
        let all_prs_with_repo: Vec<(String, MergedPr)> = repo_prs
            .into_iter()
            .flat_map(|(repo_name, prs)| prs.into_iter().map(move |pr| (repo_name.clone(), pr)))
            .collect();

        // Separate PRs with and without scores by computing effort on the fly
        let (prs_with_scores, prs_without_scores): (Vec<_>, Vec<_>) = all_prs_with_repo
            .into_iter()
            .partition(|(_, pr)| self.extract_effort_from_pr(pr).is_some());

        if prs_with_scores.is_empty() && prs_without_scores.is_empty() {
            return report;
        }

        let total_scaled_points: f64 = prs_with_scores
            .iter()
            .map(|(_, pr)| {
                self.extract_effort_from_pr(pr)
                    .unwrap()
                    .scaled_value(self.scaling_series)
            })
            .sum();

        report.total_effort_points = total_scaled_points as u32;

        // Only calculate hours_per_point if we have effort points
        if total_scaled_points > 0.0 {
            report.hours_per_point = self.monthly_hours / total_scaled_points;
        } else {
            report.hours_per_point = 0.0;
        }

        for (repo_name, pr) in prs_with_scores {
            let effort_score = self.extract_effort_from_pr(&pr).unwrap();
            let scaled_value = effort_score.scaled_value(self.scaling_series);
            let total_allocated_hours = scaled_value * report.hours_per_point;
            let percentage = (scaled_value / total_scaled_points) * 100.0;

            // Get categories for this PR
            let categories = self.extract_categories_from_pr(&pr);

            // Build category allocations for this PR
            let mut category_allocations = HashMap::new();
            let hours_per_category = total_allocated_hours / categories.len() as f64;
            for category in &categories {
                category_allocations.insert(category.clone(), hours_per_category);

                // Update category totals
                *report
                    .category_totals
                    .entry(category.clone())
                    .or_insert(0.0) += hours_per_category;
            }

            // Attribute the FULL allocated hours to the PR's ADR (a PR has at
            // most one ADR, so no splitting - unlike categories)
            let adr_id = self.extract_adr_from_pr(&pr);
            if let Some(adr) = &adr_id {
                *report.adr_totals.entry(adr.clone()).or_insert(0.0) += total_allocated_hours;
            }

            // Create one allocation per PR (team-level, no individual attribution)
            let allocation = TimeAllocation {
                pr_number: pr.number as u32,
                pr_title: pr.title.clone(),
                repository: repo_name.clone(),
                effort_score,
                allocated_hours: total_allocated_hours,
                percentage_of_total: percentage,
                categories: category_allocations,
                adr_id,
            };

            report.allocations.push(allocation);
        }

        // Track PRs without effort scores as unallocated (0 hours)
        for (repo_name, pr) in prs_without_scores {
            tracing::warn!(
                "PR #{}: No effort score label found - cannot allocate hours",
                pr.number
            );
            report
                .unallocated_prs
                .push((repo_name, pr.number as u32, pr.title.clone()));
        }

        report
            .allocations
            .sort_by(|a, b| b.allocated_hours.partial_cmp(&a.allocated_hours).unwrap());

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pr(number: u64, title: &str, body: Option<&str>, labels: &[&str]) -> MergedPr {
        MergedPr {
            number,
            title: title.to_string(),
            body: body.map(|b| b.to_string()),
            url: format!("https://github.com/test-org/test-repo/pull/{number}"),
            merged_at: chrono::Utc::now(),
            labels: labels.iter().map(|name| name.to_string()).collect(),
        }
    }

    fn calculator() -> EffortCalculator {
        EffortCalculator::new(360.0, ScalingSeries::Linear)
    }

    // --- extract_effort_from_pr ---

    #[test]
    fn each_valid_effort_label_parses_to_its_score() {
        let cases = [
            ("effort:1-super-quick", EffortScore::SuperQuick),
            ("effort:2-not-long", EffortScore::NotLong),
            ("effort:3-average", EffortScore::Average),
            ("effort:4-a-while", EffortScore::AWhile),
            ("effort:5-felt-like-forever", EffortScore::FeltLikeForever),
        ];
        for (label, expected) in cases {
            let pr = make_pr(20, "Add feature", None, &[label, "feature"]);
            assert_eq!(
                calculator().extract_effort_from_pr(&pr),
                Some(expected),
                "label {label}"
            );
        }
    }

    #[test]
    fn first_effort_label_wins_when_multiple() {
        let pr = make_pr(
            21,
            "Add feature",
            None,
            &["effort:2-not-long", "effort:5-felt-like-forever"],
        );
        assert_eq!(
            calculator().extract_effort_from_pr(&pr),
            Some(EffortScore::NotLong)
        );
    }

    #[test]
    fn unknown_effort_label_yields_none() {
        let pr = make_pr(22, "Add feature", None, &["effort:9-instant", "feature"]);
        assert_eq!(calculator().extract_effort_from_pr(&pr), None);
    }

    #[test]
    fn missing_effort_label_yields_none() {
        let pr = make_pr(23, "Add feature", None, &["feature"]);
        assert_eq!(calculator().extract_effort_from_pr(&pr), None);
    }

    // --- ScalingSeries ---

    #[test]
    fn every_scaling_series_maps_scores_to_its_values() {
        let cases = [
            (ScalingSeries::Linear, [1.0, 2.0, 3.0, 4.0, 5.0]),
            (ScalingSeries::Doubling, [1.0, 2.0, 4.0, 8.0, 16.0]),
            (ScalingSeries::Fibonacci, [1.0, 2.0, 3.0, 5.0, 8.0]),
            (ScalingSeries::Exponential, [1.0, 3.0, 9.0, 27.0, 81.0]),
            (ScalingSeries::TShirtSizes, [1.0, 3.0, 5.0, 8.0, 13.0]),
            (ScalingSeries::Square, [1.0, 4.0, 9.0, 16.0, 25.0]),
        ];
        let scores = [
            EffortScore::SuperQuick,
            EffortScore::NotLong,
            EffortScore::Average,
            EffortScore::AWhile,
            EffortScore::FeltLikeForever,
        ];
        for (series, values) in cases {
            assert_eq!(series.get_values(), values, "series {series}");
            for (i, score) in scores.iter().enumerate() {
                assert_eq!(score.scaled_value(series), values[i], "series {series}");
            }
        }
    }

    #[test]
    fn scaling_series_changes_allocation_weights() {
        // Doubling: effort 2 -> 2 points, effort 5 -> 16 points. Total 18
        // points over 360 hours = 20 hours/point; allocations sort desc.
        let pr_small = make_pr(24, "Small change", None, &["effort:2-not-long", "a"]);
        let pr_big = make_pr(25, "Big change", None, &["effort:5-felt-like-forever", "b"]);
        let report = EffortCalculator::new(360.0, ScalingSeries::Doubling).calculate_report(
            vec![("test-repo".to_string(), vec![pr_small, pr_big])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        assert_eq!(report.hours_per_point, 20.0);
        assert_eq!(report.total_effort_points, 18);
        assert_eq!(report.allocations[0].pr_number, 25);
        assert_eq!(report.allocations[0].allocated_hours, 320.0);
        assert_eq!(report.allocations[1].pr_number, 24);
        assert_eq!(report.allocations[1].allocated_hours, 40.0);
    }

    // --- category split math ---

    #[test]
    fn hours_split_equally_across_categories() {
        // Single PR absorbs all 360 hours; three categories split it 120/120/120.
        let pr = make_pr(
            26,
            "Add feature",
            None,
            &["effort:3-average", "feature", "testing", "docs"],
        );
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![pr])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        let allocation = &report.allocations[0];
        assert_eq!(allocation.allocated_hours, 360.0);
        for category in ["feature", "testing", "docs"] {
            assert_eq!(allocation.categories.get(category), Some(&120.0));
            assert_eq!(report.category_totals.get(category), Some(&120.0));
        }
    }

    // --- unallocated handling ---

    #[test]
    fn prs_without_effort_are_unallocated_and_do_not_skew_hours() {
        let scored = make_pr(27, "Add feature", None, &["effort:3-average", "feature"]);
        let unscored = make_pr(28, "Mystery work", None, &["feature"]);
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![scored, unscored])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        // The unscored PR earns no allocation and is reported for QC...
        assert_eq!(report.allocations.len(), 1);
        assert_eq!(
            report.unallocated_prs,
            vec![("test-repo".to_string(), 28, "Mystery work".to_string())]
        );
        assert_eq!(report.unallocated_hours, 0.0);
        // ...while the scored PR still absorbs the full monthly hours.
        assert_eq!(report.allocations[0].allocated_hours, 360.0);
    }

    #[test]
    fn empty_month_produces_empty_report() {
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), Vec::new())],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        assert_eq!(report.total_effort_points, 0);
        assert_eq!(report.hours_per_point, 0.0);
        assert!(report.allocations.is_empty());
        assert!(report.unallocated_prs.is_empty());
        assert!(report.category_totals.is_empty());
        assert!(report.adr_totals.is_empty());
    }

    // --- extract_adr_from_pr ---

    #[test]
    fn adr_label_is_extracted() {
        let pr = make_pr(
            1,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012"],
        );
        assert_eq!(
            calculator().extract_adr_from_pr(&pr),
            Some("ADR-0012".to_string())
        );
    }

    #[test]
    fn first_adr_label_wins_when_multiple() {
        let pr = make_pr(
            2,
            "Add feature",
            None,
            &["adr:ADR-0001", "adr:ADR-0002", "effort:2-not-long"],
        );
        assert_eq!(
            calculator().extract_adr_from_pr(&pr),
            Some("ADR-0001".to_string())
        );
    }

    #[test]
    fn body_trailer_is_fallback_when_no_adr_label() {
        let pr = make_pr(
            3,
            "Add feature",
            Some("Implements the new module.\n\nAdr-Reference: ADR-0007"),
            &["effort:1-super-quick"],
        );
        assert_eq!(
            calculator().extract_adr_from_pr(&pr),
            Some("ADR-0007".to_string())
        );
    }

    #[test]
    fn adr_label_beats_body_trailer() {
        let pr = make_pr(
            4,
            "Add feature",
            Some("Adr-Reference: ADR-0002"),
            &["adr:ADR-0001"],
        );
        assert_eq!(
            calculator().extract_adr_from_pr(&pr),
            Some("ADR-0001".to_string())
        );
    }

    #[test]
    fn no_adr_returns_none() {
        let no_body = make_pr(5, "Fix bug", None, &["effort:1-super-quick", "bug-fix"]);
        assert_eq!(calculator().extract_adr_from_pr(&no_body), None);

        let plain_body = make_pr(
            6,
            "Fix bug",
            Some("A body without any trailer.\nMentions ADR-0001 but not as a trailer."),
            &["effort:1-super-quick"],
        );
        assert_eq!(calculator().extract_adr_from_pr(&plain_body), None);

        let empty_trailer = make_pr(7, "Fix bug", Some("Adr-Reference:"), &[]);
        assert_eq!(calculator().extract_adr_from_pr(&empty_trailer), None);
    }

    // --- extract_categories_from_pr ---

    #[test]
    fn adr_labels_are_excluded_from_categories() {
        let pr = make_pr(
            8,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012", "feature"],
        );
        assert_eq!(
            calculator().extract_categories_from_pr(&pr),
            vec!["feature".to_string()]
        );
    }

    #[test]
    fn adr_only_pr_falls_back_to_uncategorized() {
        let pr = make_pr(
            9,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012"],
        );
        assert_eq!(
            calculator().extract_categories_from_pr(&pr),
            vec!["Uncategorized".to_string()]
        );
    }

    #[test]
    fn category_hours_not_diluted_by_adr_label() {
        // Single PR, effort 3 (linear) => all 360 monthly hours allocated to it.
        // The adr: label must NOT count as a category, so "feature" gets the
        // full 360 hours instead of an equal split with "adr:ADR-0012".
        let pr = make_pr(
            10,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012", "feature"],
        );
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![pr])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        assert_eq!(report.category_totals.get("feature"), Some(&360.0));
        assert!(!report.category_totals.contains_key("adr:ADR-0012"));
    }

    #[test]
    fn conduit_labels_are_excluded_from_categories() {
        // ADR-0005: conduit:* prefixes are machinery, not categories.
        let pr = make_pr(
            30,
            "Add feature",
            None,
            &["effort:3-average", "conduit:task", "feature"],
        );
        assert_eq!(
            calculator().extract_categories_from_pr(&pr),
            vec!["feature".to_string()]
        );
    }

    #[test]
    fn structural_only_pr_falls_back_to_uncategorized() {
        // The dogfood shape: a conduit PR carries exactly one effort label,
        // an adr:<reference> label, and conduit:* machinery - no category
        // labels. It must land in Uncategorized, not grow fake categories.
        let pr = make_pr(
            31,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012", "conduit:task"],
        );
        assert_eq!(
            calculator().extract_categories_from_pr(&pr),
            vec!["Uncategorized".to_string()]
        );
    }

    #[test]
    fn category_totals_stay_free_of_conduit_labels() {
        // Single PR, effort 3 (linear) => all 360 monthly hours allocated.
        // conduit:* must not appear in category_totals, and the hours fall
        // to Uncategorized undiluted; the ADR still gets full credit.
        let pr = make_pr(
            32,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012", "conduit:task"],
        );
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![pr])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        assert!(!report.category_totals.contains_key("conduit:task"));
        assert_eq!(report.category_totals.get("Uncategorized"), Some(&360.0));
        assert_eq!(report.adr_totals.get("ADR-0012"), Some(&360.0));
    }

    // --- ADR attribution in the report ---

    #[test]
    fn allocations_carry_adr_id() {
        let with_adr = make_pr(
            11,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012"],
        );
        let without_adr = make_pr(12, "Fix bug", None, &["effort:2-not-long", "bug-fix"]);
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![with_adr, without_adr])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        let find = |number: u32| {
            report
                .allocations
                .iter()
                .find(|a| a.pr_number == number)
                .unwrap()
        };
        assert_eq!(find(11).adr_id, Some("ADR-0012".to_string()));
        assert_eq!(find(12).adr_id, None);
    }

    #[test]
    fn adr_totals_accumulate_full_credit_across_prs() {
        // Effort points (linear): 3 + 1 + 1 = 5 => 72 hours per point.
        // PR 13 (label) and PR 14 (body trailer) share ADR-0012: 216 + 72 = 288.
        // PR 15 has no ADR and must not appear anywhere in adr_totals.
        let pr_label = make_pr(
            13,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012"],
        );
        let pr_trailer = make_pr(
            14,
            "Follow-up",
            Some("Cleanup.\n\nAdr-Reference: ADR-0012"),
            &["effort:1-super-quick"],
        );
        let pr_no_adr = make_pr(15, "Fix bug", None, &["effort:1-super-quick", "bug-fix"]);
        let report = calculator().calculate_report(
            vec![(
                "test-repo".to_string(),
                vec![pr_label, pr_trailer, pr_no_adr],
            )],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        assert_eq!(report.adr_totals.len(), 1);
        let total = report.adr_totals.get("ADR-0012").copied().unwrap();
        assert!((total - 288.0).abs() < 1e-9, "expected 288.0, got {total}");
    }

    #[test]
    fn adr_gets_full_credit_while_categories_split() {
        // One PR, two categories: categories split the 360 hours equally,
        // but the ADR is credited with the FULL 360 hours.
        let pr = make_pr(
            16,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0042", "feature", "testing"],
        );
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![pr])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        assert_eq!(report.category_totals.get("feature"), Some(&180.0));
        assert_eq!(report.category_totals.get("testing"), Some(&180.0));
        assert_eq!(report.adr_totals.get("ADR-0042"), Some(&360.0));
    }

    #[test]
    fn report_without_adrs_has_empty_adr_totals() {
        let pr = make_pr(17, "Fix bug", None, &["effort:2-not-long", "bug-fix"]);
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![pr])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        assert!(report.adr_totals.is_empty());
    }

    // --- canonical JSON schema (export endpoint contract) ---

    /// Canonical serialization: serde_json's Map is BTreeMap-backed (no
    /// `preserve_order` feature in this build), so converting to `Value`
    /// sorts every object's keys and HashMap iteration order cannot leak
    /// into the output. Byte-stable across runs for fixed inputs.
    fn canonical_json(report: &MonthlyReport) -> String {
        let value = serde_json::to_value(report).unwrap();
        serde_json::to_string_pretty(&value).unwrap()
    }

    /// Deterministic multi-repo report exercising the full schema surface:
    /// multi-category split, ADR via label and via body trailer, an
    /// Uncategorized ADR-only PR, a PR with neither, and an unallocated PR.
    /// Effort points (linear) 5+3+1+2 = 11 force non-terminating decimals,
    /// so any float-math change breaks the byte comparison.
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
        calculator().calculate_report(
            vec![("alpha".to_string(), alpha), ("beta".to_string(), beta)],
            "January".to_string(),
            2026,
            "como".to_string(),
        )
    }

    #[test]
    fn monthly_report_json_matches_pinned_fixture() {
        // Fixture captured from the pre-workspace-split calculator; the
        // export endpoint's response schema must stay byte-compatible.
        let json = canonical_json(&fixture_report());
        let pinned = include_str!("../tests/fixtures/monthly_report.json");
        assert_eq!(json, pinned.trim_end());
    }

    #[test]
    fn serialized_report_exposes_adr_totals() {
        // Contract for the machine-readable export: adr_totals and per-allocation
        // adr_id must be present in the serialized MonthlyReport.
        let pr = make_pr(
            18,
            "Add feature",
            None,
            &["effort:3-average", "adr:ADR-0012"],
        );
        let report = calculator().calculate_report(
            vec![("test-repo".to_string(), vec![pr])],
            "January".to_string(),
            2026,
            "test-org".to_string(),
        );

        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["adr_totals"]["ADR-0012"], serde_json::json!(360.0));
        assert_eq!(
            json["allocations"][0]["adr_id"],
            serde_json::json!("ADR-0012")
        );
    }
}

//! Polarity-aware scorecard computation.
//!
//! Takes an `Assessment` + `AssessmentResponse` and produces a rollup of
//! pass/fail/unknown/unanswered counts at practice, domain, and
//! assessment levels, with percent weighted by question count (*not*
//! the unweighted mean of child percents — see design notes in the
//! vision book).

use serde::Serialize;

use crate::models::ids::{DomainId, PracticeId, QuestionId};
use crate::models::{Answer, AnswerValue, Assessment, AssessmentResponse, Polarity};

/// Whether a single (polarity, answer) combination represents a pass.
///
/// - Positive polarity + Yes  → pass
/// - Negative polarity + No   → pass
/// - Everything else (No, Unknown, Unanswered on Positive; Yes, Unknown,
///   Unanswered on Negative) → not a pass.
pub fn resolves_to_pass(polarity: Polarity, answer: Option<AnswerValue>) -> bool {
    matches!(
        (polarity, answer),
        (Polarity::Positive, Some(AnswerValue::Yes)) | (Polarity::Negative, Some(AnswerValue::No))
    )
}

/// Outcome bucket for a single question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Passed,
    Failed,
    Unknown,
    Unanswered,
}

fn classify(polarity: Polarity, answer: Option<AnswerValue>) -> Outcome {
    match (polarity, answer) {
        (Polarity::Positive, Some(AnswerValue::Yes)) => Outcome::Passed,
        (Polarity::Negative, Some(AnswerValue::No)) => Outcome::Passed,
        (Polarity::Positive, Some(AnswerValue::No)) => Outcome::Failed,
        (Polarity::Negative, Some(AnswerValue::Yes)) => Outcome::Failed,
        (_, Some(AnswerValue::Unknown)) => Outcome::Unknown,
        (_, None) => Outcome::Unanswered,
    }
}

/// Counts for one level of the hierarchy (practice, domain, or overall).
#[derive(Debug, Clone, Default, Serialize)]
pub struct LevelSummary {
    pub passed: usize,
    pub failed: usize,
    pub unknown: usize,
    pub unanswered: usize,
    pub total: usize,
    /// Weighted percent: `passed / (passed + failed)` rounded to an integer.
    /// Zero when no definite answers yet (no-op denominator).
    pub percent: u8,
}

impl LevelSummary {
    fn from_counts(passed: usize, failed: usize, unknown: usize, unanswered: usize) -> Self {
        let total = passed + failed + unknown + unanswered;
        let definite = passed + failed;
        let percent = if definite == 0 {
            0
        } else {
            ((passed as f64 / definite as f64) * 100.0).round() as u8
        };
        Self {
            passed,
            failed,
            unknown,
            unanswered,
            total,
            percent,
        }
    }

    /// True once at least one question has a definite Yes/No answer —
    /// otherwise the percent is `0` but not meaningful.
    pub fn has_definite_answers(&self) -> bool {
        self.passed + self.failed > 0
    }
}

/// One practice's rollup.
#[derive(Debug, Clone, Serialize)]
pub struct PracticeSummary {
    pub id: PracticeId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminology: Option<String>,
    pub summary: LevelSummary,
}

/// One domain's rollup (weighted by question count across practices).
#[derive(Debug, Clone, Serialize)]
pub struct DomainSummary {
    pub id: DomainId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminology: Option<String>,
    pub summary: LevelSummary,
    pub practices: Vec<PracticeSummary>,
}

/// Top-level scorecard.
#[derive(Debug, Clone, Serialize)]
pub struct Scorecard {
    pub overall: LevelSummary,
    pub domains: Vec<DomainSummary>,
}

/// Build a polarity-aware scorecard from an assessment + response.
///
/// Aggregation rolls counts upward: a practice's summary is the sum of
/// its questions' outcomes; a domain's summary is the sum across its
/// practices; the overall summary is the sum across all domains. The
/// `percent` at every level is always computed from `passed` and
/// `failed` at *that* level — this is what makes the weighting
/// question-count-proportional and not a naive mean-of-means.
pub fn compute_scorecard(assessment: &Assessment, response: &AssessmentResponse) -> Scorecard {
    let mut overall_passed = 0;
    let mut overall_failed = 0;
    let mut overall_unknown = 0;
    let mut overall_unanswered = 0;
    let mut domains = Vec::with_capacity(assessment.domains.len());

    for domain in &assessment.domains {
        let mut d_passed = 0;
        let mut d_failed = 0;
        let mut d_unknown = 0;
        let mut d_unanswered = 0;
        let mut practices = Vec::with_capacity(domain.practices.len());

        for practice in &domain.practices {
            let mut p_passed = 0;
            let mut p_failed = 0;
            let mut p_unknown = 0;
            let mut p_unanswered = 0;

            for question in &practice.questions {
                let answer = answer_for(response, question.id).map(|a| a.value);
                match classify(question.polarity, answer) {
                    Outcome::Passed => p_passed += 1,
                    Outcome::Failed => p_failed += 1,
                    Outcome::Unknown => p_unknown += 1,
                    Outcome::Unanswered => p_unanswered += 1,
                }
            }

            practices.push(PracticeSummary {
                id: practice.id,
                name: practice.name.clone(),
                terminology: practice.terminology.clone(),
                summary: LevelSummary::from_counts(p_passed, p_failed, p_unknown, p_unanswered),
            });

            d_passed += p_passed;
            d_failed += p_failed;
            d_unknown += p_unknown;
            d_unanswered += p_unanswered;
        }

        domains.push(DomainSummary {
            id: domain.id,
            name: domain.name.clone(),
            terminology: domain.terminology.clone(),
            summary: LevelSummary::from_counts(d_passed, d_failed, d_unknown, d_unanswered),
            practices,
        });

        overall_passed += d_passed;
        overall_failed += d_failed;
        overall_unknown += d_unknown;
        overall_unanswered += d_unanswered;
    }

    Scorecard {
        overall: LevelSummary::from_counts(
            overall_passed,
            overall_failed,
            overall_unknown,
            overall_unanswered,
        ),
        domains,
    }
}

fn answer_for(response: &AssessmentResponse, question_id: QuestionId) -> Option<&Answer> {
    response.answers.get(&question_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::assessment::{Domain, Practice, Question};
    use crate::models::ids::RespondentId;
    use crate::models::{Answer, AnswerValue};

    fn practice_with(n: usize, polarity: Polarity) -> Practice {
        let mut p = Practice::new("P".into(), "c".into(), "v".into(), "r".into());
        for i in 0..n {
            let mut q = Question::new(format!("Q{}", i));
            q.polarity = polarity;
            p.questions.push(q);
        }
        p
    }

    fn fresh_response(a: &Assessment) -> AssessmentResponse {
        AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string())
    }

    fn assessment_of(practices: Vec<Practice>) -> Assessment {
        let mut a = Assessment::new("A".into(), "d".into(), "g".into());
        let mut d = Domain::new("D".into(), "c".into(), "v".into(), "r".into());
        d.practices = practices;
        a.domains.push(d);
        a
    }

    #[test]
    fn all_yes_positive_is_100_percent() {
        let a = assessment_of(vec![practice_with(3, Polarity::Positive)]);
        let mut r = fresh_response(&a);
        for q in &a.domains[0].practices[0].questions {
            r.upsert_answer(q.id, Answer::new(AnswerValue::Yes));
        }
        let sc = compute_scorecard(&a, &r);
        assert_eq!(sc.overall.passed, 3);
        assert_eq!(sc.overall.failed, 0);
        assert_eq!(sc.overall.percent, 100);
    }

    #[test]
    fn all_yes_negative_is_0_percent() {
        // Negative polarity: Yes = problem exists = fail.
        let a = assessment_of(vec![practice_with(3, Polarity::Negative)]);
        let mut r = fresh_response(&a);
        for q in &a.domains[0].practices[0].questions {
            r.upsert_answer(q.id, Answer::new(AnswerValue::Yes));
        }
        let sc = compute_scorecard(&a, &r);
        assert_eq!(sc.overall.passed, 0);
        assert_eq!(sc.overall.failed, 3);
        assert_eq!(sc.overall.percent, 0);
    }

    #[test]
    fn all_no_negative_is_100_percent() {
        let a = assessment_of(vec![practice_with(3, Polarity::Negative)]);
        let mut r = fresh_response(&a);
        for q in &a.domains[0].practices[0].questions {
            r.upsert_answer(q.id, Answer::new(AnswerValue::No));
        }
        let sc = compute_scorecard(&a, &r);
        assert_eq!(sc.overall.passed, 3);
        assert_eq!(sc.overall.percent, 100);
    }

    #[test]
    fn unknown_and_unanswered_tracked_distinctly() {
        let a = assessment_of(vec![practice_with(4, Polarity::Positive)]);
        let mut r = fresh_response(&a);
        let qs = &a.domains[0].practices[0].questions;
        r.upsert_answer(qs[0].id, Answer::new(AnswerValue::Yes));
        r.upsert_answer(qs[1].id, Answer::new(AnswerValue::No));
        r.upsert_answer(qs[2].id, Answer::new(AnswerValue::Unknown));
        // qs[3] left unanswered.
        let sc = compute_scorecard(&a, &r);
        assert_eq!(sc.overall.passed, 1);
        assert_eq!(sc.overall.failed, 1);
        assert_eq!(sc.overall.unknown, 1);
        assert_eq!(sc.overall.unanswered, 1);
        assert_eq!(sc.overall.total, 4);
        // Percent uses only definite answers (1 passed / 2 definite).
        assert_eq!(sc.overall.percent, 50);
    }

    #[test]
    fn weighted_aggregation_across_practices() {
        // Practice A: 10 questions, all Yes → 10 passed.
        // Practice B: 2 questions, all No → 2 failed.
        // Unweighted mean would be (100% + 0%) / 2 = 50%.
        // Weighted is 10 / 12 = 83%.
        let a = assessment_of(vec![
            practice_with(10, Polarity::Positive),
            practice_with(2, Polarity::Positive),
        ]);
        let mut r = fresh_response(&a);
        for q in &a.domains[0].practices[0].questions {
            r.upsert_answer(q.id, Answer::new(AnswerValue::Yes));
        }
        for q in &a.domains[0].practices[1].questions {
            r.upsert_answer(q.id, Answer::new(AnswerValue::No));
        }
        let sc = compute_scorecard(&a, &r);
        assert_eq!(sc.overall.passed, 10);
        assert_eq!(sc.overall.failed, 2);
        assert_eq!(sc.overall.percent, 83); // 10/12 = 0.833... → 83
        // And per-practice percents are still discoverable.
        assert_eq!(sc.domains[0].practices[0].summary.percent, 100);
        assert_eq!(sc.domains[0].practices[1].summary.percent, 0);
    }

    /// Diagnostic against the real user-reported lemonade-stand project.
    /// Prints the per-question classification + per-practice rollup so we
    /// can see what the code actually produces for that data.
    #[test]
    #[ignore = "diagnostic: run with `--ignored -- --nocapture` against live data"]
    fn lemonade_site_selection_diagnostic() {
        let project_id = "38c255be-0516-43a5-87e3-cce1dd936f6c";
        let a_yaml =
            std::fs::read_to_string(format!("data/projects/{}/assessment.yaml", project_id))
                .expect("assessment.yaml");
        let r_yaml = std::fs::read_to_string(format!(
            "data/projects/{}/responses/00000000-0000-0000-0000-000000000001.yaml",
            project_id
        ))
        .expect("response yaml");

        let assessment: Assessment = serde_yaml::from_str(&a_yaml).expect("parse assessment");
        let response: AssessmentResponse = serde_yaml::from_str(&r_yaml).expect("parse response");

        for domain in &assessment.domains {
            for practice in &domain.practices {
                if practice.name != "Site Selection" {
                    continue;
                }
                eprintln!("\n=== Practice: {} ({}) ===", practice.name, practice.id);
                for q in &practice.questions {
                    let a = response.answers.get(&q.id);
                    eprintln!(
                        "  Q {} polarity={:?} answer={:?} outcome={:?}\n      text={}",
                        q.id,
                        q.polarity,
                        a.map(|x| x.value),
                        classify(q.polarity, a.map(|x| x.value)),
                        q.text,
                    );
                }
            }
        }

        let sc = compute_scorecard(&assessment, &response);
        for domain in &sc.domains {
            for p in &domain.practices {
                if p.name != "Site Selection" {
                    continue;
                }
                eprintln!(
                    "\nSite Selection: passed={}, failed={}, unknown={}, unanswered={}, percent={}",
                    p.summary.passed,
                    p.summary.failed,
                    p.summary.unknown,
                    p.summary.unanswered,
                    p.summary.percent
                );
            }
        }
    }

    #[test]
    fn empty_response_has_definite_false() {
        let a = assessment_of(vec![practice_with(3, Polarity::Positive)]);
        let r = fresh_response(&a);
        let sc = compute_scorecard(&a, &r);
        assert_eq!(sc.overall.unanswered, 3);
        assert_eq!(sc.overall.passed + sc.overall.failed, 0);
        assert!(!sc.overall.has_definite_answers());
    }
}

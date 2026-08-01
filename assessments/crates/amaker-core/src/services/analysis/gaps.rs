//! Gap inventory: every non-passing question, enriched with the
//! narrative and operational context needed to act on it.
//!
//! A "gap" here includes every outcome that isn't a pass under the
//! question's polarity. Unknown and Unanswered questions also qualify
//! (they're epistemic gaps) and are tagged so the UI can surface them
//! distinctly.

use serde::Serialize;

use super::scorecard::resolves_to_pass;
use crate::models::assessment::{Domain, Practice, Question};
use crate::models::ids::{BlockerTypeId, DomainId, PracticeId, QuestionId};
use crate::models::{
    AnswerValue, Assessment, AssessmentResponse, BlockerType, EffortRange, Polarity,
};

/// A single gap row — flattened for rendering and export.
#[derive(Debug, Clone, Serialize)]
pub struct Gap {
    // Identity & context
    pub question_id: QuestionId,
    pub question_text: String,
    pub polarity: Polarity,
    pub answer_value: Option<AnswerValue>,

    pub domain_id: DomainId,
    pub domain_name: String,
    pub practice_id: PracticeId,
    pub practice_name: String,

    // Narrative (inherited from the practice; domain CVR is available on
    // the parent domain summary if the UI wants it).
    pub practice_context: String,
    pub practice_value: String,
    pub practice_risk: String,

    // Operational metadata carried on the question itself.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,

    // Respondent metadata (only populated for actual No answers).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<BlockerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Gap {
    /// True for answers that represent a definite problem (Positive+No or
    /// Negative+Yes). False for Unknown / Unanswered gaps. Used by the
    /// gap-inventory and narrative filtering layers (M4).
    #[allow(dead_code)]
    pub fn is_definite(&self) -> bool {
        matches!(
            (self.polarity, self.answer_value),
            (Polarity::Positive, Some(AnswerValue::No))
                | (Polarity::Negative, Some(AnswerValue::Yes))
        )
    }
}

/// Collection of gaps in insertion order (matches assessment traversal).
#[derive(Debug, Clone, Default, Serialize)]
pub struct GapInventory {
    pub gaps: Vec<Gap>,
}

/// Walk the assessment, collecting a `Gap` for every question whose
/// polarity-adjusted outcome isn't a pass.
pub fn compute_gaps(assessment: &Assessment, response: &AssessmentResponse) -> GapInventory {
    let mut gaps = Vec::new();

    for domain in &assessment.domains {
        for practice in &domain.practices {
            for question in &practice.questions {
                let answer_value = response.answers.get(&question.id).map(|a| a.value);
                if resolves_to_pass(question.polarity, answer_value) {
                    continue;
                }
                gaps.push(build_gap(
                    assessment,
                    domain,
                    practice,
                    question,
                    answer_value,
                    response,
                ));
            }
        }
    }

    GapInventory { gaps }
}

fn build_gap(
    assessment: &Assessment,
    domain: &Domain,
    practice: &Practice,
    question: &Question,
    answer_value: Option<AnswerValue>,
    response: &AssessmentResponse,
) -> Gap {
    let answer = response.answers.get(&question.id);

    // Only resolve blockers / planned / notes when the respondent took a
    // definite "No" position — for Unknown / Unanswered these fields are
    // absent and should stay absent on the gap row.
    let (blockers, planned, notes) = match answer {
        Some(a) if a.value == AnswerValue::No => (
            resolve_blockers(&assessment.blocker_types, &a.blocker_ids),
            a.planned,
            a.notes.clone(),
        ),
        Some(a) => (Vec::new(), None, a.notes.clone()),
        None => (Vec::new(), None, None),
    };

    Gap {
        question_id: question.id,
        question_text: question.text.clone(),
        polarity: question.polarity,
        answer_value,

        domain_id: domain.id,
        domain_name: domain.name.clone(),
        practice_id: practice.id,
        practice_name: practice.name.clone(),

        practice_context: practice.context.clone(),
        practice_value: practice.value.clone(),
        practice_risk: practice.risk.clone(),

        roles: question.roles.clone(),
        effort: question.effort.clone(),
        remediation: question.remediation.clone(),

        blockers,
        planned,
        notes,
    }
}

fn resolve_blockers(vocab: &[BlockerType], ids: &[BlockerTypeId]) -> Vec<BlockerType> {
    ids.iter()
        .filter_map(|id| vocab.iter().find(|bt| &bt.id == id).cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::assessment::{Domain, Practice, Question};
    use crate::models::ids::RespondentId;
    use crate::models::{Answer, AnswerValue};

    fn mk_assessment() -> Assessment {
        let mut a = Assessment::new("A".into(), "d".into(), "g".into());
        let mut d = Domain::new("D".into(), "dctx".into(), "dval".into(), "drisk".into());
        let mut p = Practice::new("P".into(), "pctx".into(), "pval".into(), "prisk".into());
        p.questions.push(Question::new("Q1".into()));
        p.questions.push({
            let mut q = Question::new("Q2 (negative)".into());
            q.polarity = Polarity::Negative;
            q
        });
        p.questions.push(Question::new("Q3".into()));
        d.practices.push(p);
        a.domains.push(d);
        a
    }

    #[test]
    fn passing_questions_excluded() {
        let a = mk_assessment();
        let mut r = AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string());
        // Q1 (positive) Yes → pass.
        // Q2 (negative) Yes → fail (problem exists).
        // Q3 (positive) unanswered → epistemic gap.
        let qs = &a.domains[0].practices[0].questions;
        r.upsert_answer(qs[0].id, Answer::new(AnswerValue::Yes));
        r.upsert_answer(qs[1].id, Answer::new(AnswerValue::Yes));

        let gi = compute_gaps(&a, &r);
        assert_eq!(gi.gaps.len(), 2);
        assert_eq!(gi.gaps[0].question_text, "Q2 (negative)");
        assert!(gi.gaps[0].is_definite());
        assert_eq!(gi.gaps[1].question_text, "Q3");
        assert!(!gi.gaps[1].is_definite());
        assert_eq!(gi.gaps[1].answer_value, None);
    }

    #[test]
    fn blockers_resolved_from_vocabulary() {
        let a = mk_assessment();
        let mut r = AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string());
        let qs = &a.domains[0].practices[0].questions;

        let mut ans = Answer::new(AnswerValue::No);
        ans.blocker_ids.push(a.blocker_types[0].id.clone()); // "people"
        ans.blocker_ids.push(a.blocker_types[2].id.clone()); // "technology"
        ans.planned = Some(true);
        r.upsert_answer(qs[0].id, ans);

        let gi = compute_gaps(&a, &r);
        let gap = gi
            .gaps
            .iter()
            .find(|g| g.question_id == qs[0].id)
            .expect("gap for Q1");
        assert_eq!(gap.blockers.len(), 2);
        assert_eq!(gap.blockers[0].label, "People");
        assert_eq!(gap.blockers[1].label, "Technology");
        assert_eq!(gap.planned, Some(true));
    }

    #[test]
    fn unknown_is_a_gap_but_not_definite() {
        let a = mk_assessment();
        let mut r = AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string());
        let qs = &a.domains[0].practices[0].questions;
        r.upsert_answer(qs[0].id, Answer::new(AnswerValue::Unknown));

        let gi = compute_gaps(&a, &r);
        let gap = gi.gaps.iter().find(|g| g.question_id == qs[0].id).unwrap();
        assert_eq!(gap.answer_value, Some(AnswerValue::Unknown));
        assert!(!gap.is_definite());
        assert!(gap.blockers.is_empty());
    }
}

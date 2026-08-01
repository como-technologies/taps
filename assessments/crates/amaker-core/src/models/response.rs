//! Respondent answers to an assessment.
//!
//! An `AssessmentResponse` is the per-respondent collection of answers
//! to a specific assessment. V1 uses a single default respondent; the
//! `respondent_id` field is carried from day one so multi-SME
//! aggregation is an additive change later.
//!
//! Staging infrastructure for M2 (Collection phase). Items here are
//! consumed by the `ResponseService`, answer handlers, and analysis layer.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Assessment;
use super::assessment::Polarity;
use super::ids::{AssessmentId, BlockerTypeId, EvidenceTypeId, QuestionId, RespondentId};
use crate::error::AppError;

/// Trinary answer value. Absence of an `Answer` record means unanswered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AnswerValue {
    Yes,
    No,
    Unknown,
}

/// A single question's answer, plus the supporting metadata that the
/// analysis layer turns into narrative.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Answer {
    pub value: AnswerValue,

    /// Which evidence types support a "yes". Keyed into the assessment's
    /// `evidence_types` vocabulary. Empty for non-yes answers.
    #[serde(default)]
    pub evidence_ids: Vec<EvidenceTypeId>,

    /// Which blocker types explain a "no". Keyed into the assessment's
    /// `blocker_types` vocabulary. Empty for non-no answers.
    #[serde(default)]
    pub blocker_ids: Vec<BlockerTypeId>,

    /// For "no" answers, is remediation planned?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned: Option<bool>,

    /// Free-text notes — always available as an escape hatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    pub answered_at: DateTime<Utc>,
}

impl Answer {
    /// Create a fresh answer with the given value and `answered_at = now`.
    pub fn new(value: AnswerValue) -> Self {
        Self {
            value,
            evidence_ids: Vec::new(),
            blocker_ids: Vec::new(),
            planned: None,
            notes: None,
            answered_at: Utc::now(),
        }
    }

    /// True if this answer cites the given evidence type. Used from templates
    /// (askama expressions can't hold closures).
    pub fn has_evidence(&self, id: &EvidenceTypeId) -> bool {
        self.evidence_ids.iter().any(|e| e == id)
    }

    /// True if this answer cites the given blocker type.
    pub fn has_blocker(&self, id: &BlockerTypeId) -> bool {
        self.blocker_ids.iter().any(|b| b == id)
    }

    /// True when this answer represents a "pass" under the question's
    /// polarity: Positive+Yes or Negative+No. Drives which supporting
    /// UI (evidence vs. blockers) renders and what the server persists.
    ///
    /// Takes `&Polarity` so askama expressions can pass `question.polarity`
    /// directly (templates reference struct fields through references).
    pub fn resolves_to_pass(&self, polarity: &Polarity) -> bool {
        matches!(
            (polarity, self.value),
            (Polarity::Positive, AnswerValue::Yes) | (Polarity::Negative, AnswerValue::No)
        )
    }

    /// True when this answer represents a definite gap under the
    /// question's polarity: Positive+No or Negative+Yes. Unknown is
    /// neither a pass nor a definite fail.
    pub fn resolves_to_fail(&self, polarity: &Polarity) -> bool {
        matches!(
            (polarity, self.value),
            (Polarity::Positive, AnswerValue::No) | (Polarity::Negative, AnswerValue::Yes)
        )
    }
}

/// A respondent's collected answers to an assessment.
///
/// Bound to a specific *published version* (git tag) at construction.
/// Answers always resolve against the assessment shape at that tag, so
/// subsequent draft edits — including a different SME publishing v2 —
/// don't invalidate the response. See
/// `docs/src/architecture/draft-publish.md` "Responses & answers".
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssessmentResponse {
    /// The published version (git tag) this response is bound to. The
    /// loader reads `git show <version>:assessment.yaml` to recover the
    /// shape, so changes to the live draft don't affect a recorded
    /// response. Required field — a missing `version` is a hard
    /// deserialization error.
    pub version: String,

    pub assessment_id: AssessmentId,
    pub respondent_id: RespondentId,

    #[serde(default)]
    pub answers: HashMap<QuestionId, Answer>,

    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<DateTime<Utc>>,
}

impl AssessmentResponse {
    /// Create a new empty response bound to a specific published version.
    pub fn new(assessment_id: AssessmentId, respondent_id: RespondentId, version: String) -> Self {
        let now = Utc::now();
        Self {
            version,
            assessment_id,
            respondent_id,
            answers: HashMap::new(),
            started_at: now,
            updated_at: now,
            submitted_at: None,
        }
    }

    /// Verify every answered question UUID exists in `assessment`. The
    /// response loader runs this after reading the assessment at the
    /// response's bound version — a mismatch means the response references
    /// a question that doesn't exist at that tag, which (per spec) is a
    /// hard error not a silent skip.
    pub fn validate_against(&self, assessment: &Assessment) -> Result<(), AppError> {
        let valid: HashSet<QuestionId> = assessment
            .domains
            .iter()
            .flat_map(|d| &d.practices)
            .flat_map(|p| &p.questions)
            .map(|q| q.id)
            .collect();
        for qid in self.answers.keys() {
            if !valid.contains(qid) {
                return Err(AppError::Internal(format!(
                    "Response references question {qid} which doesn't exist in \
                     assessment {} at version '{}'",
                    assessment.id, self.version
                )));
            }
        }
        Ok(())
    }

    /// Upsert an answer, bumping `updated_at`.
    pub fn upsert_answer(&mut self, question_id: QuestionId, answer: Answer) {
        self.answers.insert(question_id, answer);
        self.updated_at = Utc::now();
    }

    /// Remove an answer (reverting to unanswered), bumping `updated_at`.
    pub fn clear_answer(&mut self, question_id: &QuestionId) {
        if self.answers.remove(question_id).is_some() {
            self.updated_at = Utc::now();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.answers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_to_pass_is_polarity_aware() {
        let yes = Answer::new(AnswerValue::Yes);
        let no = Answer::new(AnswerValue::No);
        let unknown = Answer::new(AnswerValue::Unknown);

        // Positive polarity: Yes = pass.
        assert!(yes.resolves_to_pass(&Polarity::Positive));
        assert!(!yes.resolves_to_fail(&Polarity::Positive));
        assert!(!no.resolves_to_pass(&Polarity::Positive));
        assert!(no.resolves_to_fail(&Polarity::Positive));

        // Negative polarity: No = pass (the problem is absent).
        assert!(no.resolves_to_pass(&Polarity::Negative));
        assert!(!no.resolves_to_fail(&Polarity::Negative));
        assert!(!yes.resolves_to_pass(&Polarity::Negative));
        assert!(yes.resolves_to_fail(&Polarity::Negative));

        // Unknown never passes and never definitely fails.
        for p in [Polarity::Positive, Polarity::Negative] {
            assert!(!unknown.resolves_to_pass(&p));
            assert!(!unknown.resolves_to_fail(&p));
        }
    }

    #[test]
    fn upsert_and_clear_roundtrip() {
        let mut r =
            AssessmentResponse::new(AssessmentId::new(), RespondentId::new(), "v1".to_string());
        let q = QuestionId::new();

        assert!(r.is_empty());

        r.upsert_answer(q, Answer::new(AnswerValue::Yes));
        assert_eq!(r.answers.len(), 1);
        assert_eq!(r.answers.get(&q).unwrap().value, AnswerValue::Yes);

        r.upsert_answer(q, Answer::new(AnswerValue::No));
        assert_eq!(r.answers.get(&q).unwrap().value, AnswerValue::No);

        r.clear_answer(&q);
        assert!(r.is_empty());
    }
}

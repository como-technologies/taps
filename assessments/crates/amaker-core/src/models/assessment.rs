//! Assessment data model following the Amaker metamodel.
//!
//! The metamodel defines a 4-level hierarchy:
//! - Assessment: The root container
//! - Domain: Major focus areas (3-7 per assessment)
//! - Practice: Specific capabilities within a domain (2-5 per domain)
//! - Question: Binary checks (3-12 per practice)

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ids::{AssessmentId, DomainId, PracticeId, QuestionId};
use super::vocab::{
    BlockerType, EffortRange, EvidenceType, default_blocker_types, default_evidence_types,
};

/// Polarity of a question - whether "yes" is good or bad.
///
/// Most questions should be Positive (yes = practice is in place).
/// Use Negative for risk-focused questions (yes = problem exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(
    description = "Whether 'yes' indicates the practice is implemented (positive) or a problem exists (negative)"
)]
#[serde(rename_all = "lowercase")]
pub enum Polarity {
    /// "Yes" means the practice is in place (desirable)
    Positive,
    /// "Yes" means a problem exists (undesirable)
    Negative,
}

impl std::fmt::Display for Polarity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Polarity::Positive => write!(f, "positive"),
            Polarity::Negative => write!(f, "negative"),
        }
    }
}

/// An assessment following the Amaker metamodel.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(
    description = "An assessment following the Amaker metamodel - the root container defining what is being evaluated and why"
)]
pub struct Assessment {
    #[serde(default)]
    pub id: AssessmentId,
    pub name: String,
    pub description: String,
    pub goal: String,
    pub domains: Vec<Domain>,

    /// Controlled vocabulary describing what evidence can support a "yes"
    /// answer. Customizable per assessment; seeded with a sensible default.
    #[serde(default = "default_evidence_types")]
    pub evidence_types: Vec<EvidenceType>,

    /// Controlled vocabulary describing what can block a "no" from
    /// becoming a "yes". Customizable per assessment; seeded with a
    /// sensible default.
    #[serde(default = "default_blocker_types")]
    pub blocker_types: Vec<BlockerType>,

    #[serde(default)]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Assessment {
    /// Count total questions across all domains and practices.
    pub fn question_count(&self) -> usize {
        self.domains
            .iter()
            .flat_map(|d| &d.practices)
            .map(|p| p.questions.len())
            .sum()
    }

    /// Count total domains.
    pub fn domain_count(&self) -> usize {
        self.domains.len()
    }

    /// Count total practices.
    pub fn practice_count(&self) -> usize {
        self.domains.iter().map(|d| d.practices.len()).sum()
    }

    /// Serialize the assessment to YAML.
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Find a domain by id.
    pub fn find_domain_mut(&mut self, id: DomainId) -> Option<&mut Domain> {
        self.domains.iter_mut().find(|d| d.id == id)
    }

    /// Find a practice by id, anywhere in the tree.
    pub fn find_practice_mut(&mut self, id: PracticeId) -> Option<&mut Practice> {
        self.domains
            .iter_mut()
            .flat_map(|d| d.practices.iter_mut())
            .find(|p| p.id == id)
    }

    /// Find a question by id, anywhere in the tree.
    pub fn find_question_mut(&mut self, id: QuestionId) -> Option<&mut Question> {
        self.domains
            .iter_mut()
            .flat_map(|d| d.practices.iter_mut())
            .flat_map(|p| p.questions.iter_mut())
            .find(|q| q.id == id)
    }

    /// Remove a question by id, returning it on success.
    pub fn remove_question(&mut self, id: QuestionId) -> Option<Question> {
        for domain in &mut self.domains {
            for practice in &mut domain.practices {
                if let Some(idx) = practice.questions.iter().position(|q| q.id == id) {
                    return Some(practice.questions.remove(idx));
                }
            }
        }
        None
    }

    /// Remove a practice by id, returning it on success.
    pub fn remove_practice(&mut self, id: PracticeId) -> Option<Practice> {
        for domain in &mut self.domains {
            if let Some(idx) = domain.practices.iter().position(|p| p.id == id) {
                return Some(domain.practices.remove(idx));
            }
        }
        None
    }

    /// Remove a domain by id, returning it on success.
    pub fn remove_domain(&mut self, id: DomainId) -> Option<Domain> {
        let idx = self.domains.iter().position(|d| d.id == id)?;
        Some(self.domains.remove(idx))
    }

    /// Create a new empty assessment with defaulted vocabularies and fresh IDs.
    pub fn new(name: String, description: String, goal: String) -> Self {
        Self {
            id: AssessmentId::new(),
            name,
            description,
            goal,
            domains: Vec::new(),
            evidence_types: default_evidence_types(),
            blocker_types: default_blocker_types(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

/// A domain (major focus area) within an assessment.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(
    description = "A domain represents a major focus area or category within the assessment (3-7 per assessment)"
)]
pub struct Domain {
    #[serde(default)]
    pub id: DomainId,
    pub name: String,
    pub context: String,
    pub value: String,
    pub risk: String,
    pub practices: Vec<Practice>,

    /// Alternative terminology (e.g., "Stage", "Category", "Pillar")
    #[serde(default)]
    pub terminology: Option<String>,
}

impl Domain {
    /// Create a new domain with a fresh ID and no practices.
    pub fn new(name: String, context: String, value: String, risk: String) -> Self {
        Self {
            id: DomainId::new(),
            name,
            context,
            value,
            risk,
            practices: Vec::new(),
            terminology: None,
        }
    }
}

/// A practice (specific capability) within a domain.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(
    description = "A practice represents a specific capability, control, or activity within a domain (2-5 per domain)"
)]
pub struct Practice {
    #[serde(default)]
    pub id: PracticeId,
    pub name: String,
    pub context: String,
    pub value: String,
    pub risk: String,
    pub questions: Vec<Question>,

    /// Optional guidance for implementing this practice
    #[serde(default)]
    pub guidance: Option<String>,

    /// Alternative terminology (e.g., "Capability", "Control", "Activity")
    #[serde(default)]
    pub terminology: Option<String>,
}

impl Practice {
    /// Create a new practice with a fresh ID and no questions.
    pub fn new(name: String, context: String, value: String, risk: String) -> Self {
        Self {
            id: PracticeId::new(),
            name,
            context,
            value,
            risk,
            questions: Vec::new(),
            guidance: None,
            terminology: None,
        }
    }
}

/// A question (binary check) within a practice.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(
    description = "A question is a binary check (yes/no/unknown) that determines if a practice is implemented correctly (3-12 per practice)"
)]
pub struct Question {
    #[serde(default)]
    pub id: QuestionId,
    pub text: String,

    /// Whether "yes" is positive (practice in place) or negative (problem exists)
    pub polarity: Polarity,

    /// Optional guidance on how to verify this question
    #[serde(default)]
    pub guidance: Option<String>,

    /// What evidence would prove a "yes" answer
    #[serde(default)]
    pub evidence: Option<String>,

    /// Guidance if the answer is "no"
    #[serde(default)]
    pub remediation: Option<String>,

    /// Which disciplines own this question. Short role tags like
    /// "security-engineer", "product-manager". Multiple allowed.
    #[serde(default)]
    pub roles: Vec<String>,

    /// Estimated effort to remediate if the answer is "no".
    #[serde(default)]
    pub effort: Option<EffortRange>,
}

impl Question {
    /// Create a new question (test helper).
    /// Create a new question with a fresh ID, positive polarity, and no
    /// metadata. Useful when constructing test fixtures or seeding a draft
    /// practice with a placeholder.
    pub fn new(text: String) -> Self {
        Self {
            id: QuestionId::new(),
            text,
            polarity: Polarity::Positive,
            guidance: None,
            evidence: None,
            remediation: None,
            roles: Vec::new(),
            effort: None,
        }
    }
}

/// Rig extractor target for `generate_questions`. The extractor's synthetic
/// `submit` tool needs an object-typed schema, so questions are wrapped.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(description = "A set of questions generated for a single practice")]
pub struct QuestionList {
    pub questions: Vec<Question>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assessment_counts() {
        let mut assessment = Assessment::new(
            "Test".to_string(),
            "Description".to_string(),
            "Goal".to_string(),
        );

        let mut domain = Domain::new(
            "Domain 1".to_string(),
            "Context".to_string(),
            "Value".to_string(),
            "Risk".to_string(),
        );

        let mut practice = Practice::new(
            "Practice 1".to_string(),
            "Context".to_string(),
            "Value".to_string(),
            "Risk".to_string(),
        );

        practice
            .questions
            .push(Question::new("Question 1?".to_string()));
        practice
            .questions
            .push(Question::new("Question 2?".to_string()));
        domain.practices.push(practice);
        assessment.domains.push(domain);

        assert_eq!(assessment.domain_count(), 1);
        assert_eq!(assessment.practice_count(), 1);
        assert_eq!(assessment.question_count(), 2);
    }

    #[test]
    fn find_and_remove_helpers_work_across_levels() {
        let mut assessment = Assessment::new("A".into(), "d".into(), "g".into());
        let mut domain = Domain::new("D".into(), "c".into(), "v".into(), "r".into());
        let mut practice = Practice::new("P".into(), "c".into(), "v".into(), "r".into());
        let q1 = Question::new("Q1?".into());
        let q1_id = q1.id;
        let p_id = practice.id;
        let d_id = domain.id;
        practice.questions.push(q1);
        domain.practices.push(practice);
        assessment.domains.push(domain);

        // find_*_mut returns the expected entity
        assert!(assessment.find_question_mut(q1_id).is_some());
        assert!(assessment.find_practice_mut(p_id).is_some());
        assert!(assessment.find_domain_mut(d_id).is_some());

        // Missing ids return None
        assert!(assessment.find_question_mut(QuestionId::new()).is_none());

        // Remove drains from the tree
        assert!(assessment.remove_question(q1_id).is_some());
        assert_eq!(assessment.question_count(), 0);
        assert!(assessment.remove_practice(p_id).is_some());
        assert_eq!(assessment.practice_count(), 0);
        assert!(assessment.remove_domain(d_id).is_some());
        assert_eq!(assessment.domain_count(), 0);

        // Second remove is None
        assert!(assessment.remove_domain(d_id).is_none());
    }

    #[test]
    fn question_list_schema_is_object_typed() {
        let schema = schemars::schema_for!(QuestionList);
        let value = serde_json::to_value(&schema).expect("schema serializable");
        assert_eq!(value.get("type").and_then(|t| t.as_str()), Some("object"));
        assert!(
            value
                .get("properties")
                .and_then(|p| p.get("questions"))
                .is_some(),
            "QuestionList schema missing `questions` property"
        );
    }
}

//! Project model.
//!
//! A `Project` is the unit of authoring — one git-backed repo per project.
//! Its `focus_substate` records where the SME is in the top-down
//! decomposition (Scoping → Structuring → Questions → Refining). The
//! substate is an indicator, not a gate; data integrity belongs to the
//! draft/publish layer (see `docs/src/architecture/draft-publish.md`).
//!
//! Responding and Analyzing are not project states — they're audience
//! views routed to separate URLs (and, eventually, separate binaries per
//! #39). They never appear on `Project`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Assessment;
use super::chat::ClarifyingQuestion;
use super::ids::{DocumentId, ProjectId};

/// A project carries the live draft of an assessment plus the SME's
/// current authoring focus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Where the SME is in the authoring decomposition. Starts at Scoping
    /// and moves forward as the conversation progresses, but the SME can
    /// also jump backward (e.g. revisit Scoping mid-Questions) via
    /// `switch_focus`. Surgical CRUD works from any substate.
    #[serde(default = "default_substate", alias = "focus_substate")]
    pub focus_substate: AuthoringSubstate,

    /// IDs of uploaded documents for context.
    #[serde(default)]
    pub document_ids: Vec<DocumentId>,

    /// The evolving assessment (None until AI generates first draft).
    #[serde(default)]
    pub assessment: Option<Assessment>,

    /// Aggregate question-count commitment recorded via `set_question_budget`.
    /// `None` on both fields means no aggregate budget is enforced; question
    /// generation only checks per-practice bounds.
    #[serde(default)]
    pub question_budget_min: Option<u16>,
    #[serde(default)]
    pub question_budget_max: Option<u16>,

    /// Clarifying questions the orchestrator emitted in one turn that haven't
    /// been shown yet. Drained one-per-interaction on answer/skip; flushed by
    /// any free-text `send_message`.
    #[serde(default)]
    pub pending_clarifying_questions: Vec<ClarifyingQuestion>,
}

fn default_substate() -> AuthoringSubstate {
    AuthoringSubstate::Scoping
}

impl Project {
    /// Create a new project starting at Scoping.
    pub fn new(name: String, description: Option<String>) -> Self {
        Self {
            id: ProjectId::new(),
            name,
            description,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            focus_substate: AuthoringSubstate::Scoping,
            document_ids: Vec::new(),
            assessment: None,
            question_budget_min: None,
            question_budget_max: None,
            pending_clarifying_questions: Vec::new(),
        }
    }

    /// Update the timestamp.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Move the authoring focus to a different substate. Always succeeds
    /// (substates are advisory).
    pub fn set_substate(&mut self, substate: AuthoringSubstate) {
        self.focus_substate = substate;
    }
}

/// The four authoring substates. Substates are advisory: surgical CRUD
/// tools work from any of them, the substate just steers the conversation's
/// framing (which prompt fragment, which tool gating, which UI accent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringSubstate {
    /// SME explains domain/problem to assess.
    Scoping,
    /// AI proposes domains + practices (no questions yet).
    Structuring,
    /// AI drafts questions per practice.
    Questions,
    /// Collaborative polish + surgical edits before publish.
    Refining,
}

impl AuthoringSubstate {
    pub fn display_name(&self) -> &'static str {
        match self {
            AuthoringSubstate::Scoping => "Scoping",
            AuthoringSubstate::Structuring => "Structuring",
            AuthoringSubstate::Questions => "Questions",
            AuthoringSubstate::Refining => "Refining",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AuthoringSubstate::Scoping => "Define the domain, scope, and goals",
            AuthoringSubstate::Structuring => {
                "Propose domains + practices with context / value / risk"
            }
            AuthoringSubstate::Questions => "Draft questions per practice",
            AuthoringSubstate::Refining => "Polish, fix wording, fill gaps before publish",
        }
    }

    pub fn all() -> &'static [AuthoringSubstate] {
        &[
            AuthoringSubstate::Scoping,
            AuthoringSubstate::Structuring,
            AuthoringSubstate::Questions,
            AuthoringSubstate::Refining,
        ]
    }

    /// Parse a snake_case wire value. Returns `None` for unknown strings.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "scoping" => Some(AuthoringSubstate::Scoping),
            "structuring" => Some(AuthoringSubstate::Structuring),
            "questions" => Some(AuthoringSubstate::Questions),
            "refining" => Some(AuthoringSubstate::Refining),
            _ => None,
        }
    }

    /// The snake_case value used on the wire (form posts, JSON tool args).
    pub fn wire_value(&self) -> &'static str {
        match self {
            AuthoringSubstate::Scoping => "scoping",
            AuthoringSubstate::Structuring => "structuring",
            AuthoringSubstate::Questions => "questions",
            AuthoringSubstate::Refining => "refining",
        }
    }
}

impl std::fmt::Display for AuthoringSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Phase in the assessment authoring workflow.
///
/// The loop line's linear generation pipeline (`services::generation`) keys
/// its prompts off this; the web apps track decomposition progress via
/// `AuthoringSubstate` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPhase {
    Scoping,
    Structuring,
    Questions,
    Refining,
    Complete,
}

/// Metadata for an uploaded document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadedDocument {
    pub id: DocumentId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: usize,
    pub uploaded_at: DateTime<Utc>,

    /// Extracted text content (for AI context).
    #[serde(default)]
    pub extracted_text: Option<String>,
}

impl UploadedDocument {
    /// Create metadata for a new uploaded document.
    pub fn new(filename: String, content_type: String, size_bytes: usize) -> Self {
        Self {
            id: DocumentId::new(),
            filename,
            content_type,
            size_bytes,
            uploaded_at: Utc::now(),
            extracted_text: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_starts_in_scoping() {
        let project = Project::new("Test".to_string(), None);
        assert_eq!(project.focus_substate, AuthoringSubstate::Scoping);
    }

    #[test]
    fn set_substate_updates_focus() {
        let mut project = Project::new("Test".to_string(), None);
        project.set_substate(AuthoringSubstate::Questions);
        assert_eq!(project.focus_substate, AuthoringSubstate::Questions);
    }

    #[test]
    fn project_serde_round_trip_with_focus() {
        let mut project = Project::new("Test".to_string(), None);
        project.set_substate(AuthoringSubstate::Questions);
        let json = serde_json::to_string(&project).unwrap();
        let round: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(round.focus_substate, AuthoringSubstate::Questions);
    }

    #[test]
    fn project_serde_round_trip_with_queue() {
        use crate::models::ids::ClarifyingQuestionId;
        let mut project = Project::new("Test".to_string(), None);
        project
            .pending_clarifying_questions
            .push(ClarifyingQuestion {
                id: ClarifyingQuestionId::new(),
                question: "Q?".to_string(),
                options: vec![],
                allow_custom: true,
                multi_select: false,
            });
        let json = serde_json::to_string(&project).unwrap();
        let round: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(round.pending_clarifying_questions.len(), 1);
        assert_eq!(round.pending_clarifying_questions[0].question, "Q?");
    }
}

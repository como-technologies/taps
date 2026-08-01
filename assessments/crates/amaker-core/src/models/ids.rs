//! Strongly-typed ID wrappers using the newtype pattern.
//!
//! These types provide compile-time safety to prevent mixing up different
//! kinds of identifiers (e.g., passing a `QuestionId` where a `PracticeId`
//! is expected).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Macro to define a strongly-typed UUID wrapper.
macro_rules! define_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(#[schemars(with = "String")] pub Uuid);

        impl $name {
            /// Create a new random ID.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }
    };
}

define_id!(AssessmentId, "Unique identifier for an Assessment.");
define_id!(
    DomainId,
    "Unique identifier for a Domain within an assessment."
);
define_id!(
    PracticeId,
    "Unique identifier for a Practice within a domain."
);
define_id!(
    QuestionId,
    "Unique identifier for a Question within a practice."
);
define_id!(ProjectId, "Unique identifier for a Project.");
define_id!(DocumentId, "Unique identifier for an uploaded Document.");
define_id!(ChatMessageId, "Unique identifier for a ChatMessage.");
define_id!(
    ClarifyingQuestionId,
    "Unique identifier for a ClarifyingQuestion."
);
define_id!(RespondentId, "Unique identifier for a Respondent.");

/// Macro to define a strongly-typed short-string identifier.
///
/// Unlike [`define_id`], slug IDs are human-readable strings (e.g.
/// `"process_doc"`, `"people"`) rather than UUIDs. Used for per-assessment
/// vocabularies where stability across YAML edits matters more than
/// global uniqueness.
macro_rules! define_slug_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub String);

        // Helpers consumed by M2 (Collection handlers + vocab tailoring).
        #[allow(dead_code)]
        impl $name {
            /// Create a new slug ID from a string.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Borrow the underlying string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

define_slug_id!(
    EvidenceTypeId,
    "Short slug identifying an EvidenceType in an assessment's vocabulary."
);
define_slug_id!(
    BlockerTypeId,
    "Short slug identifying a BlockerType in an assessment's vocabulary."
);

/// String-based ID for tool use (external API compatibility).
///
/// Tool use IDs from Claude's API are strings, not UUIDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolUseId(pub String);

impl std::fmt::Display for ToolUseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_creation() {
        let id1 = ProjectId::new();
        let id2 = ProjectId::new();
        // Each call creates a unique ID
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_display() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let id = ProjectId(uuid);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_id_from_str() {
        let id: ProjectId = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
        assert_eq!(
            id.0,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
    }

    #[test]
    fn test_id_serialization() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let id = ProjectId(uuid);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"550e8400-e29b-41d4-a716-446655440000\"");
    }

    #[test]
    fn test_id_deserialization() {
        let json = "\"550e8400-e29b-41d4-a716-446655440000\"";
        let id: ProjectId = serde_json::from_str(json).unwrap();
        assert_eq!(
            id.0,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
    }

    #[test]
    fn test_tool_use_id() {
        let id = ToolUseId("toolu_123".to_string());
        assert_eq!(id.to_string(), "toolu_123");
    }

    #[test]
    fn test_different_id_types_not_equal() {
        // This test verifies that different ID types are distinct at compile time.
        // The following would not compile if we tried to compare them directly:
        // let project_id = ProjectId::new();
        // let document_id = DocumentId::new();
        // assert_ne!(project_id, document_id); // Compile error!

        // Instead, we can only compare IDs of the same type
        let id1 = ProjectId::new();
        let id2 = ProjectId::new();
        assert_ne!(id1, id2);
    }
}

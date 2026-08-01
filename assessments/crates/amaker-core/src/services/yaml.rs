//! YAML parsing and validation for assessments.
//!
//! This module provides backward-compatible YAML parsing by delegating
//! to the unified ExportService.

use crate::error::AppError;
use crate::models::Assessment;
use crate::services::export::{DataFormat, ExportService};

/// YAML service for parsing and validating assessments.
///
/// This is a thin wrapper around ExportService that maintains backward
/// compatibility with existing code.
pub struct YamlService;

impl YamlService {
    /// Parse YAML into an Assessment struct with schema-based validation.
    pub fn parse_assessment(yaml: &str) -> Result<Assessment, AppError> {
        ExportService::validate_and_parse(yaml, DataFormat::Yaml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::assessment::{Domain, Practice, Question};

    #[test]
    fn test_parse_valid_yaml() {
        let yaml = r#"
id: 550e8400-e29b-41d4-a716-446655440000
name: Test Assessment
description: A test assessment
goal: Test the system
created_at: 2024-01-01T00:00:00Z
updated_at: 2024-01-01T00:00:00Z
domains:
  - id: 550e8400-e29b-41d4-a716-446655440001
    name: Test Domain
    context: Domain context
    value: Domain value
    risk: Domain risk
    practices:
      - id: 550e8400-e29b-41d4-a716-446655440002
        name: Test Practice
        context: Practice context
        value: Practice value
        risk: Practice risk
        questions:
          - id: 550e8400-e29b-41d4-a716-446655440003
            text: Is this a test?
            polarity: positive
"#;

        let result = YamlService::parse_assessment(yaml);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let assessment = result.unwrap();
        assert_eq!(assessment.name, "Test Assessment");
        assert_eq!(assessment.domains.len(), 1);
        assert_eq!(assessment.domains[0].practices.len(), 1);
        assert_eq!(assessment.domains[0].practices[0].questions.len(), 1);
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "not: valid: yaml: syntax";
        let result = YamlService::parse_assessment(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_yaml_round_trip() {
        let mut assessment = Assessment::new(
            "Test Assessment".to_string(),
            "Test description".to_string(),
            "Test goal".to_string(),
        );

        let mut domain = Domain::new(
            "Test Domain".to_string(),
            "Context".to_string(),
            "Value".to_string(),
            "Risk".to_string(),
        );

        let mut practice = Practice::new(
            "Test Practice".to_string(),
            "Context".to_string(),
            "Value".to_string(),
            "Risk".to_string(),
        );

        practice
            .questions
            .push(Question::new("Is this working?".to_string()));
        domain.practices.push(practice);
        assessment.domains.push(domain);

        // Round-trip through YAML
        let yaml = assessment.to_yaml().expect("Should serialize");
        let parsed = YamlService::parse_assessment(&yaml).expect("Should parse");

        assert_eq!(parsed.name, assessment.name);
        assert_eq!(parsed.description, assessment.description);
        assert_eq!(parsed.domains.len(), 1);
        assert_eq!(parsed.domains[0].name, "Test Domain");
    }

    #[test]
    fn test_parse_yaml_without_ids() {
        // This mimics AI output - no ids, no timestamps
        let yaml = r#"
name: "Dino Boss: Mike's Lemonade Stand"
description: "A fun readiness checklist for Mike's lemonade stand"
goal: "Make sure Mike is ready to run his stand"

domains:
  - name: "Dino Chef"
    context: "Making great lemonade"
    value: "Customers love homemade!"
    risk: "Without tasty lemonade, customers might not come back."
    practices:
      - name: "Lemonade Making"
        context: "Mike helps mix up the lemonade"
        value: "Builds confidence"
        risk: "Spills could slow things down"
        questions:
          - text: "Do you know what ingredients go in lemonade?"
            polarity: positive
            guidance: "Lemons, water, sugar. Bonus for ice!"
          - text: "Can you measure a cup of water?"
            polarity: positive
"#;

        let result = YamlService::parse_assessment(yaml);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
        let assessment = result.unwrap();
        assert_eq!(assessment.name, "Dino Boss: Mike's Lemonade Stand");
        assert_eq!(assessment.domains.len(), 1);
        assert_eq!(assessment.domains[0].practices[0].questions.len(), 2);
        // IDs should be auto-generated
        assert!(!assessment.id.0.is_nil());
        assert!(!assessment.domains[0].id.0.is_nil());
    }
}

//! Unified export service for assessments.
//!
//! Provides:
//! - Data format exports (JSON, YAML, TOML) via serde
//! - JSON Schema generation from Rust structs
//! - Schema-based validation

use schemars::schema_for;
use serde_json::Value;

use crate::error::AppError;
use crate::models::Assessment;

/// Supported data formats for import/export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Json,
    Yaml,
    Toml,
}

impl DataFormat {
    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            DataFormat::Json => "json",
            DataFormat::Yaml => "yaml",
            DataFormat::Toml => "toml",
        }
    }

    /// Get the MIME type for this format.
    pub fn content_type(&self) -> &'static str {
        match self {
            DataFormat::Json => "application/json",
            DataFormat::Yaml => "text/yaml",
            DataFormat::Toml => "application/toml",
        }
    }

    /// Parse format from string (case-insensitive). Returns `None` for an
    /// unknown format; we deliberately don't implement `FromStr` because
    /// callers never need a typed error here.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(DataFormat::Json),
            "yaml" | "yml" => Some(DataFormat::Yaml),
            "toml" => Some(DataFormat::Toml),
            _ => None,
        }
    }
}

/// Unified export service for assessment data.
pub struct ExportService;

impl ExportService {
    /// Export an assessment to the specified data format.
    pub fn to_data(assessment: &Assessment, format: DataFormat) -> Result<String, AppError> {
        match format {
            DataFormat::Json => serde_json::to_string_pretty(assessment).map_err(AppError::from),
            DataFormat::Yaml => {
                serde_yaml::to_string(assessment).map_err(|e| AppError::ParseError(e.to_string()))
            }
            DataFormat::Toml => {
                toml::to_string_pretty(assessment).map_err(|e| AppError::ParseError(e.to_string()))
            }
        }
    }

    /// Generate the JSON Schema for Assessment.
    ///
    /// The `id` fields are `#[serde(default)]` and their `Default` impls
    /// mint *fresh* UUIDs, so schemars would embed a different random
    /// `default` value on every generation — making the published schema
    /// (`amaker schema`, `GET /api/schema`) non-deterministic. Those
    /// defaults carry no contract information ("a fresh UUID is minted
    /// when omitted"), so they are stripped.
    pub fn json_schema() -> Value {
        let mut schema = serde_json::to_value(schema_for!(Assessment)).unwrap();
        strip_random_uuid_defaults(&mut schema);
        schema
    }

    /// Validate data against schema and parse into an Assessment.
    pub fn validate_and_parse(data: &str, format: DataFormat) -> Result<Assessment, AppError> {
        // Parse to JSON Value first (for schema validation)
        let value: Value = match format {
            DataFormat::Json => serde_json::from_str(data).map_err(AppError::from)?,
            DataFormat::Yaml => {
                serde_yaml::from_str(data).map_err(|e| AppError::ParseError(e.to_string()))?
            }
            DataFormat::Toml => {
                let toml_val: toml::Value =
                    toml::from_str(data).map_err(|e| AppError::ParseError(e.to_string()))?;
                serde_json::to_value(toml_val).map_err(AppError::from)?
            }
        };

        // Validate against schema
        let schema = Self::json_schema();
        let validator = jsonschema::validator_for(&schema)
            .map_err(|e| AppError::Internal(format!("Schema error: {}", e)))?;

        if !validator.is_valid(&value) {
            let errors: Vec<String> = validator
                .iter_errors(&value)
                .map(|e| format!("{} at {}", e, e.instance_path()))
                .collect();
            return Err(AppError::ParseError(errors.join("; ")));
        }

        // Deserialize to Assessment
        serde_json::from_value(value).map_err(AppError::from)
    }
}

/// Recursively drop `"default"` entries whose value is a UUID string —
/// the run-random defaults schemars derives from the id newtypes' `Default`
/// impls (fresh `Uuid::new_v4()` per generation). Deterministic defaults
/// (the seeded vocabularies, epoch timestamps, `null`s) are kept.
fn strip_random_uuid_defaults(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let is_random_uuid = map
                .get("default")
                .and_then(Value::as_str)
                .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok());
            if is_random_uuid {
                map.remove("default");
            }
            for v in map.values_mut() {
                strip_random_uuid_defaults(v);
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                strip_random_uuid_defaults(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::assessment::{Domain, Practice, Question};

    fn sample_assessment() -> Assessment {
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

        assessment
    }

    #[test]
    fn test_export_json() {
        let assessment = sample_assessment();
        let json = ExportService::to_data(&assessment, DataFormat::Json).unwrap();
        assert!(json.contains("\"name\": \"Test Assessment\""));
    }

    #[test]
    fn test_export_yaml() {
        let assessment = sample_assessment();
        let yaml = ExportService::to_data(&assessment, DataFormat::Yaml).unwrap();
        assert!(yaml.contains("name: Test Assessment"));
    }

    #[test]
    fn test_export_toml() {
        let assessment = sample_assessment();
        let toml = ExportService::to_data(&assessment, DataFormat::Toml).unwrap();
        assert!(toml.contains("name = \"Test Assessment\""));
    }

    #[test]
    fn test_json_schema() {
        let schema = ExportService::json_schema();
        assert!(schema.get("$schema").is_some() || schema.get("type").is_some());
        assert!(schema.get("properties").is_some());
    }

    /// The published schema must be deterministic: without the stripper,
    /// every generation embeds fresh random UUID `default`s (from the id
    /// newtypes' `Default` impls) and two runs of `amaker schema` differ.
    #[test]
    fn json_schema_is_deterministic_and_free_of_random_uuid_defaults() {
        let a = ExportService::json_schema();
        let b = ExportService::json_schema();
        assert_eq!(a, b, "two schema generations must be identical");

        fn no_uuid_defaults(v: &Value) {
            match v {
                Value::Object(map) => {
                    if let Some(d) = map.get("default").and_then(Value::as_str) {
                        assert!(
                            uuid::Uuid::parse_str(d).is_err(),
                            "schema embeds a random UUID default: {d}"
                        );
                    }
                    map.values().for_each(no_uuid_defaults);
                }
                Value::Array(items) => items.iter().for_each(no_uuid_defaults),
                _ => {}
            }
        }
        no_uuid_defaults(&a);
    }

    #[test]
    fn test_validate_and_parse_yaml() {
        let yaml = r#"
name: Test Assessment
description: A test
goal: Test goal
domains:
  - name: Domain
    context: Context
    value: Value
    risk: Risk
    practices:
      - name: Practice
        context: Context
        value: Value
        risk: Risk
        questions:
          - text: Is this a test?
            polarity: positive
"#;
        let result = ExportService::validate_and_parse(yaml, DataFormat::Yaml);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        assert_eq!(result.unwrap().name, "Test Assessment");
    }

    #[test]
    fn test_validate_and_parse_json() {
        let assessment = sample_assessment();
        let json = ExportService::to_data(&assessment, DataFormat::Json).unwrap();
        let parsed = ExportService::validate_and_parse(&json, DataFormat::Json).unwrap();
        assert_eq!(parsed.name, assessment.name);
    }

    #[test]
    fn test_format_helpers() {
        assert_eq!(DataFormat::Json.extension(), "json");
        assert_eq!(DataFormat::Yaml.content_type(), "text/yaml");
        assert_eq!(DataFormat::from_str("YAML"), Some(DataFormat::Yaml));
        assert_eq!(DataFormat::from_str("yml"), Some(DataFormat::Yaml));
        assert_eq!(DataFormat::from_str("unknown"), None);
    }
}

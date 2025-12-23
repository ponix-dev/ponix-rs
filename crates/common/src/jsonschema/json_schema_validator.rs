//! JSON Schema validator implementation.

use crate::jsonschema::{SchemaValidationError, SchemaValidationResult, SchemaValidator};
use jsonschema::Validator;

/// JSON Schema validator implementation.
///
/// Uses the `jsonschema` crate for validation against JSON Schema Draft 2020-12.
pub struct JsonSchemaValidator;

// TODO: Add LRU cache keyed by schema hash for performance optimization
// Consider using `lru` crate with schema string hash as key

impl JsonSchemaValidator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonSchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaValidator for JsonSchemaValidator {
    fn validate(&self, schema_str: &str, data: &serde_json::Value) -> SchemaValidationResult<()> {
        // Parse the schema string as JSON
        let schema_value: serde_json::Value =
            serde_json::from_str(schema_str).map_err(|e| SchemaValidationError {
                message: format!("Invalid schema JSON: {}", e),
            })?;

        // Compile the schema
        let validator = Validator::new(&schema_value).map_err(|e| SchemaValidationError {
            message: format!("Invalid JSON Schema: {}", e),
        })?;

        // Validate the data - collect all errors
        let errors: Vec<String> = validator
            .iter_errors(data)
            .map(|e| format!("{} at {}", e, e.instance_path))
            .collect();

        if !errors.is_empty() {
            return Err(SchemaValidationError {
                message: errors.join("; "),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_schema_allows_any_object() {
        let validator = JsonSchemaValidator::new();
        let schema = "{}";
        let data = serde_json::json!({"temperature": 25.5, "humidity": 60});

        assert!(validator.validate(schema, &data).is_ok());
    }

    #[test]
    fn test_valid_data_passes_schema() {
        let validator = JsonSchemaValidator::new();
        let schema = r#"{
            "type": "object",
            "properties": {
                "temperature": {"type": "number"},
                "humidity": {"type": "number"}
            },
            "required": ["temperature"]
        }"#;
        let data = serde_json::json!({"temperature": 25.5, "humidity": 60});

        assert!(validator.validate(schema, &data).is_ok());
    }

    #[test]
    fn test_missing_required_field_fails() {
        let validator = JsonSchemaValidator::new();
        let schema = r#"{
            "type": "object",
            "properties": {
                "temperature": {"type": "number"}
            },
            "required": ["temperature"]
        }"#;
        let data = serde_json::json!({"humidity": 60});

        let result = validator.validate(schema, &data);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("required"));
    }

    #[test]
    fn test_wrong_type_fails() {
        let validator = JsonSchemaValidator::new();
        let schema = r#"{
            "type": "object",
            "properties": {
                "temperature": {"type": "number"}
            }
        }"#;
        let data = serde_json::json!({"temperature": "not a number"});

        let result = validator.validate(schema, &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_schema_json_fails() {
        let validator = JsonSchemaValidator::new();
        let schema = "not valid json";
        let data = serde_json::json!({"temperature": 25.5});

        let result = validator.validate(schema, &data);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Invalid schema JSON"));
    }

    #[test]
    fn test_invalid_schema_type_fails() {
        let validator = JsonSchemaValidator::new();
        let schema = r#"{"type": "not_a_valid_type"}"#;
        let data = serde_json::json!({"temperature": 25.5});

        let result = validator.validate(schema, &data);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Invalid JSON Schema"));
    }
}

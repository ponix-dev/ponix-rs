//! JSON Schema validation utilities.

use crate::domain::DomainError;
use jsonschema::Validator;

/// Validate that a string is a valid JSON Schema (Draft 2020-12)
/// Returns Ok(()) if valid, or InvalidJsonSchema error with details
pub fn validate_json_schema(schema_str: &str) -> Result<(), DomainError> {
    // Parse the schema string as JSON
    let schema_value: serde_json::Value = serde_json::from_str(schema_str)
        .map_err(|e| DomainError::InvalidJsonSchema(format!("Invalid JSON: {}", e)))?;

    // Attempt to compile the schema - this validates it's a valid JSON Schema
    Validator::new(&schema_value)
        .map_err(|e| DomainError::InvalidJsonSchema(format!("Invalid JSON Schema: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_empty_schema() {
        assert!(validate_json_schema("{}").is_ok());
    }

    #[test]
    fn test_valid_object_schema() {
        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}}"#;
        assert!(validate_json_schema(schema).is_ok());
    }

    #[test]
    fn test_invalid_json() {
        let result = validate_json_schema("not valid json");
        assert!(matches!(result, Err(DomainError::InvalidJsonSchema(_))));
        if let Err(DomainError::InvalidJsonSchema(msg)) = result {
            assert!(msg.contains("Invalid JSON"));
        }
    }

    #[test]
    fn test_invalid_schema_type() {
        // A schema with an invalid type value
        let schema = r#"{"type": "not_a_valid_type"}"#;
        let result = validate_json_schema(schema);
        assert!(matches!(result, Err(DomainError::InvalidJsonSchema(_))));
    }
}

//! Garde validation utilities.

use crate::domain::DomainError;
use garde::{Report, Validate};

/// Convert garde validation report to DomainError
pub fn validate_struct<T>(value: &T) -> Result<(), DomainError>
where
    T: Validate,
    T::Context: Default,
{
    value
        .validate()
        .map_err(|report| DomainError::ValidationError(format_validation_errors(&report)))
}

/// Format validation errors from garde Report into a human-readable string
fn format_validation_errors(report: &Report) -> String {
    report
        .iter()
        .map(|(path, error)| {
            if path.to_string().is_empty() {
                error.message().to_string()
            } else {
                format!("{}: {}", path, error.message())
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;

    #[derive(Validate)]
    struct TestRequest {
        #[garde(length(min = 1))]
        field: String,
    }

    #[test]
    fn test_validate_success() {
        let request = TestRequest {
            field: "value".to_string(),
        };
        assert!(validate_struct(&request).is_ok());
    }

    #[test]
    fn test_validate_failure() {
        let request = TestRequest {
            field: "".to_string(),
        };
        let result = validate_struct(&request);
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[test]
    fn test_validate_error_message_contains_field_info() {
        let request = TestRequest {
            field: "".to_string(),
        };
        let result = validate_struct(&request);
        if let Err(DomainError::ValidationError(msg)) = result {
            // Garde generates a message like "field: length is lower than 1"
            assert!(msg.contains("field"));
        } else {
            panic!("Expected ValidationError");
        }
    }
}

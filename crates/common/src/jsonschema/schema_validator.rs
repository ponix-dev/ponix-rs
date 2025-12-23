//! Schema validator trait for JSON Schema validation.

/// Result type for schema validation operations.
pub type SchemaValidationResult<T> = Result<T, SchemaValidationError>;

/// Error type for schema validation failures.
#[derive(Debug, Clone)]
pub struct SchemaValidationError {
    /// The validation error message(s)
    pub message: String,
}

impl std::fmt::Display for SchemaValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SchemaValidationError {}

/// Validates JSON data against a JSON Schema.
///
/// The schema_str should be a valid JSON Schema (Draft 2020-12).
/// Empty schema `{}` is permissive and allows any valid JSON.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait SchemaValidator: Send + Sync {
    /// Validate data against a JSON Schema.
    ///
    /// # Arguments
    /// * `schema_str` - The JSON Schema as a string
    /// * `data` - The JSON value to validate
    ///
    /// # Returns
    /// * `Ok(())` if validation passes
    /// * `Err(SchemaValidationError)` if validation fails
    fn validate(
        &self,
        schema_str: &str,
        data: &serde_json::Value,
    ) -> SchemaValidationResult<()>;
}

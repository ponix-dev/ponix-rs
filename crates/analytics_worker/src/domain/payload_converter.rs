use common::domain::DomainResult;

/// Trait for converting binary payloads to JSON using pre-compiled CEL expressions
///
/// Implementations should:
/// - Deserialize the compiled bytes back into a CEL AST
/// - Execute it against the binary payload
/// - Return JSON value on success
/// - Return PayloadConversionError on failure
#[cfg_attr(test, mockall::automock)]
pub trait PayloadConverter: Send + Sync {
    /// Transform binary payload to JSON using a pre-compiled CEL expression
    ///
    /// # Arguments
    /// * `compiled` - Serialized CheckedExpr bytes
    /// * `source` - Original expression source (for error messages)
    /// * `payload` - Binary payload to convert
    ///
    /// # Returns
    /// JSON value as serde_json::Value on success
    fn transform(
        &self,
        compiled: &[u8],
        source: &str,
        payload: &[u8],
    ) -> DomainResult<serde_json::Value>;

    /// Evaluate a pre-compiled CEL match expression against a binary payload
    ///
    /// # Arguments
    /// * `compiled` - Serialized CheckedExpr bytes
    /// * `source` - Original expression source (for error messages)
    /// * `payload` - Binary payload to evaluate against
    ///
    /// # Returns
    /// true if the expression matches, false otherwise
    fn evaluate_match(&self, compiled: &[u8], source: &str, payload: &[u8]) -> DomainResult<bool>;
}

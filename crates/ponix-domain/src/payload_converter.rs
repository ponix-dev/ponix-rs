use crate::error::DomainResult;

/// Trait for converting binary payloads to JSON using CEL expressions
///
/// Implementations should:
/// - Execute the provided CEL expression against the binary payload
/// - Return JSON value on success
/// - Return PayloadConversionError on failure
#[cfg_attr(test, mockall::automock)]
pub trait PayloadConverter: Send + Sync {
    /// Convert binary payload to JSON using a CEL expression
    ///
    /// # Arguments
    /// * `expression` - CEL expression string (e.g., "cayenne_lpp_decode(input)")
    /// * `payload` - Binary payload to convert
    ///
    /// # Returns
    /// JSON value as serde_json::Value on success
    fn convert(
        &self,
        expression: &str,
        payload: &[u8],
    ) -> DomainResult<serde_json::Value>;
}

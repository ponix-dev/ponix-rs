use crate::domain::PayloadConverter;
use common::cel::{CelEnvironment, CelValue};
use common::domain::{DomainError, DomainResult};
use tracing::{debug, error, instrument};

/// Implementation of PayloadConverter using CelEnvironment
///
/// Converts binary payloads to JSON using CEL expressions
pub struct CelPayloadConverter {
    cel_env: CelEnvironment,
}

impl CelPayloadConverter {
    /// Create a new CelPayloadConverter
    ///
    /// Initializes a CelEnvironment with cayenne_lpp_decode function registered
    pub fn new() -> Self {
        Self {
            cel_env: CelEnvironment::new(),
        }
    }
}

impl Default for CelPayloadConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl PayloadConverter for CelPayloadConverter {
    #[instrument(
        name = "payload_transform",
        skip(self, compiled, payload),
        fields(
            source = %source,
            payload_size = payload.len(),
        )
    )]
    fn transform(
        &self,
        compiled: &[u8],
        source: &str,
        payload: &[u8],
    ) -> DomainResult<serde_json::Value> {
        debug!("transforming payload with pre-compiled CEL expression");

        self.cel_env
            .execute(compiled, source, payload)
            .map_err(|e| {
                error!(error = %e, "CEL execution failed");
                DomainError::PayloadConversionError(e.to_string())
            })
    }

    #[instrument(
        name = "payload_evaluate_match",
        skip(self, compiled, payload),
        fields(
            source = %source,
            payload_size = payload.len(),
        )
    )]
    fn evaluate_match(&self, compiled: &[u8], source: &str, payload: &[u8]) -> DomainResult<bool> {
        debug!("evaluating match expression");

        self.cel_env
            .evaluate_bool(compiled, source, CelValue::from(payload.to_vec()))
            .map_err(|e| {
                error!(error = %e, "CEL match evaluation failed");
                DomainError::PayloadConversionError(e.to_string())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::cel::{CelCompiler, CelExpressionCompiler};

    fn compile(expr: &str) -> Vec<u8> {
        CelCompiler::new().compile(expr).unwrap()
    }

    #[test]
    fn test_cel_converter_simple_expression() {
        let converter = CelPayloadConverter::new();
        let expr = "{'result': 42}";
        let result = converter.transform(&compile(expr), expr, &[]);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["result"], 42);
    }

    #[test]
    fn test_cel_converter_cayenne_lpp_decode() {
        let converter = CelPayloadConverter::new();

        // Cayenne LPP payload: channel 1, temperature sensor (0x67), value 27.2°C (0x0110)
        let payload = vec![0x01, 0x67, 0x01, 0x10];

        let expr = "cayenne_lpp_decode(input)";
        let result = converter.transform(&compile(expr), expr, &payload);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.is_object());
        assert!(json.get("temperature_1").is_some());
    }

    #[test]
    fn test_cel_converter_invalid_expression_fails_at_compile() {
        // Invalid expressions now fail at compile time
        let compiler = CelCompiler::new();
        let result = compiler.compile("invalid syntax {[");
        assert!(result.is_err());
    }

    #[test]
    fn test_cel_converter_complex_transformation() {
        let converter = CelPayloadConverter::new();

        let payload = vec![0x01, 0x67, 0x01, 0x10]; // 27.2°C

        // Transform to Fahrenheit with metadata
        let expression = r#"
            {
                'temperature_celsius': cayenne_lpp_decode(input).temperature_1,
                'temperature_fahrenheit': cayenne_lpp_decode(input).temperature_1 * 9.0 / 5.0 + 32.0,
                'unit': 'fahrenheit'
            }
        "#;

        let result = converter.transform(&compile(expression), expression, &payload);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.get("temperature_celsius").is_some());
        assert!(json.get("temperature_fahrenheit").is_some());
        assert_eq!(json["unit"], "fahrenheit");
    }

    #[test]
    fn test_cel_converter_default_constructor() {
        let converter = CelPayloadConverter::default();
        let expr = "{'test': true}";
        let result = converter.transform(&compile(expr), expr, &[]);

        assert!(result.is_ok());
    }

    #[test]
    fn test_evaluate_match_true() {
        let converter = CelPayloadConverter::new();
        let expr = "true";
        let result = converter.evaluate_match(&compile(expr), expr, &[]);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_evaluate_match_false() {
        let converter = CelPayloadConverter::new();
        let expr = "false";
        let result = converter.evaluate_match(&compile(expr), expr, &[]);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_evaluate_match_with_payload_size() {
        let converter = CelPayloadConverter::new();
        let expr = "size(input) > 2";
        let result = converter.evaluate_match(&compile(expr), expr, &[1, 2, 3, 4]);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_evaluate_match_invalid_compiled_bytes() {
        let converter = CelPayloadConverter::new();
        let result = converter.evaluate_match(&[0xFF, 0xFE], "bad", &[]);
        assert!(result.is_err());
    }
}

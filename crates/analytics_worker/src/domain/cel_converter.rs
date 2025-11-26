use crate::domain::{CelEnvironment, PayloadConverter};
use common::domain::{DomainError, DomainResult};
use tracing::{debug, error};

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
    fn convert(&self, expression: &str, payload: &[u8]) -> DomainResult<serde_json::Value> {
        debug!(
            expression = %expression,
            payload_size = payload.len(),
            "Converting payload with CEL expression"
        );

        self.cel_env.execute(expression, payload).map_err(|e| {
            error!(
                expression = %expression,
                error = %e,
                "CEL execution failed"
            );
            DomainError::PayloadConversionError(e.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cel_converter_simple_expression() {
        let converter = CelPayloadConverter::new();
        let result = converter.convert("{'result': 42}", &[]);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["result"], 42);
    }

    #[test]
    fn test_cel_converter_cayenne_lpp_decode() {
        let converter = CelPayloadConverter::new();

        // Cayenne LPP payload: channel 1, temperature sensor (0x67), value 27.2°C (0x0110)
        let payload = vec![0x01, 0x67, 0x01, 0x10];

        let result = converter.convert("cayenne_lpp_decode(input)", &payload);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.is_object());
        assert!(json.get("temperature_1").is_some());
    }

    #[test]
    fn test_cel_converter_invalid_expression() {
        let converter = CelPayloadConverter::new();
        let result = converter.convert("invalid syntax {[", &[]);

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DomainError::PayloadConversionError(_))
        ));
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

        let result = converter.convert(expression, &payload);

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.get("temperature_celsius").is_some());
        assert!(json.get("temperature_fahrenheit").is_some());
        assert_eq!(json["unit"], "fahrenheit");
    }

    #[test]
    fn test_cel_converter_default_constructor() {
        let converter = CelPayloadConverter::default();
        let result = converter.convert("{'test': true}", &[]);

        assert!(result.is_ok());
    }
}

//! CEL (Common Expression Language) support for payload decoding
//!
//! This module provides CEL expression evaluation capabilities for transforming
//! binary payloads into JSON using custom decoder functions.
//!
//! # Overview
//!
//! CEL expressions enable configurable, runtime-defined transformations of sensor
//! data. Instead of hardcoding decoding logic, users can provide expressions that
//! call registered decoder functions and apply transformations.
//!
//!
//! # Available Functions
//!
//! ## cayenne_lpp_decode(input) -> json
//!
//! Decodes Cayenne LPP formatted binary payloads into JSON.
//!
//! **Input**: Binary data (bytes)
//! **Output**: JSON object with sensor readings
//!
//! **Example**:
//! ```cel
//! cayenne_lpp_decode(input)
//! // => {"temperature_1": 27.2, "humidity_2": 60.0}
//! ```
//!
//! Supports 27 sensor types including temperature, humidity, GPS, accelerometer,
//! and more. See [`crate::cayenne_lpp`] for full type list.
//!
//! # Expression Examples
//!
//! ## Basic Decoding
//! ```cel
//! cayenne_lpp_decode(input)
//! ```
//!
//! ## Conditional Logic
//! ```cel
//! cayenne_lpp_decode(input).temperature_1 > 30.0
//!   ? {'alert': 'high', 'temp': cayenne_lpp_decode(input).temperature_1}
//!   : {'alert': 'normal', 'temp': cayenne_lpp_decode(input).temperature_1}
//! ```
//!
//! ## Data Enrichment
//! ```cel
//! {
//!   'device_id': 'sensor-001',
//!   'timestamp': '2025-11-19T14:00:00Z',
//!   'readings': cayenne_lpp_decode(input)
//! }
//! ```
//!
//! ## Field Extraction
//! ```cel
//! cayenne_lpp_decode(input).temperature_1
//! ```
//!
//! ## Field Validation
//! ```cel
//! has(cayenne_lpp_decode(input).temperature_1)
//!   ? cayenne_lpp_decode(input)
//!   : {'error': 'missing temperature'}
//! ```
//!
//! # Error Handling
//!
//! CEL execution can fail at multiple stages:
//!
//! - **Compilation errors**: Invalid CEL syntax
//! - **Execution errors**: Undefined variables, type mismatches, decoder failures
//! - **Validation errors**: Output is not valid JSON

use crate::domain::cayenne_lpp::CayenneLppDecoder;
use crate::domain::{PayloadDecoder, PayloadError, Result};
use cel_interpreter::objects::{Key, Map};
use cel_interpreter::{Context, ExecutionError, Program, Value as CelValue};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

/// CEL execution environment with registered custom functions
///
/// The `CelEnvironment` provides a simple API for executing CEL expressions
/// against binary payloads. Custom decoder functions are automatically registered
/// when the environment is created.
pub struct CelEnvironment {
    // Store as owned Context since we can't store references without lifetimes
    _marker: std::marker::PhantomData<()>,
}

impl CelEnvironment {
    /// Create a new CEL environment with cayenne_lpp_decode function registered
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Execute a CEL expression string with binary input data
    ///
    /// # Arguments
    /// * `expression` - CEL expression string (e.g., "cayenne_lpp_decode(input)")
    /// * `input` - Binary payload data
    ///
    /// # Returns
    /// * `Ok(JsonValue)` - Valid JSON output from expression
    /// * `Err(PayloadError)` - Compilation, execution, or validation error
    pub fn execute(&self, expression: &str, input: &[u8]) -> Result<JsonValue> {
        // Compile the CEL expression
        let program = Program::compile(expression)
            .map_err(|e| PayloadError::CelCompilationError(e.to_string()))?;

        // Create a fresh context for execution with custom functions
        let mut context = Context::default();

        // Register cayenne_lpp_decode function
        context.add_function(
            "cayenne_lpp_decode",
            |bytes: Arc<Vec<u8>>| -> std::result::Result<CelValue, ExecutionError> {
                let decoder = CayenneLppDecoder::new();
                decoder
                    .decode(&bytes)
                    .map_err(|e| ExecutionError::FunctionError {
                        function: "cayenne_lpp_decode".to_string(),
                        message: e.to_string(),
                    })
                    .and_then(|json_value| {
                        json_to_cel_value(json_value).map_err(|e| ExecutionError::FunctionError {
                            function: "cayenne_lpp_decode".to_string(),
                            message: e.to_string(),
                        })
                    })
            },
        );

        // Add input variable as bytes
        let input_value = CelValue::Bytes(Arc::new(input.to_vec()));
        context.add_variable_from_value("input", input_value);

        // Execute the program with the context
        let result = program
            .execute(&context)
            .map_err(|e| PayloadError::CelExecutionError(e.to_string()))?;

        // Convert CEL value to JSON and validate
        cel_value_to_json(result)
    }
}

impl Default for CelEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert CEL Value to serde_json::Value with validation
fn cel_value_to_json(value: CelValue) -> Result<JsonValue> {
    match value {
        CelValue::String(s) => {
            // If it's a string, try parsing as JSON first
            // Otherwise return as JSON string
            serde_json::from_str(s.as_ref()).or_else(|_| Ok(JsonValue::String(s.to_string())))
        }
        CelValue::Int(i) => Ok(JsonValue::Number(i.into())),
        CelValue::UInt(u) => serde_json::Number::from_f64(u as f64)
            .map(JsonValue::Number)
            .ok_or(PayloadError::InvalidJsonOutput),
        CelValue::Float(f) => serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .ok_or(PayloadError::InvalidJsonOutput),
        CelValue::Bool(b) => Ok(JsonValue::Bool(b)),
        CelValue::Bytes(b) => {
            // Bytes can't be directly represented in JSON
            // Encode as base64 string
            Ok(JsonValue::String(base64_encode(&b)))
        }
        CelValue::List(items) => {
            let json_items: Result<Vec<JsonValue>> = items
                .iter()
                .map(|item| cel_value_to_json(item.clone()))
                .collect();
            Ok(JsonValue::Array(json_items?))
        }
        CelValue::Map(map) => {
            let mut json_map = serde_json::Map::new();
            for (key, value) in map.map.iter() {
                let key_str = match key {
                    Key::String(s) => s.to_string(),
                    Key::Int(i) => i.to_string(),
                    Key::Uint(u) => u.to_string(),
                    Key::Bool(b) => b.to_string(),
                };
                let json_value = cel_value_to_json(value.clone())?;
                json_map.insert(key_str, json_value);
            }
            Ok(JsonValue::Object(json_map))
        }
        CelValue::Null => Ok(JsonValue::Null),
        _ => Err(PayloadError::InvalidJsonOutput),
    }
}

/// Convert serde_json::Value to CEL Value
fn json_to_cel_value(json: JsonValue) -> Result<CelValue> {
    match json {
        JsonValue::Null => Ok(CelValue::Null),
        JsonValue::Bool(b) => Ok(CelValue::Bool(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(CelValue::Int(i))
            } else if let Some(u) = n.as_u64() {
                Ok(CelValue::UInt(u))
            } else if let Some(f) = n.as_f64() {
                Ok(CelValue::Float(f))
            } else {
                Err(PayloadError::InvalidJsonOutput)
            }
        }
        JsonValue::String(s) => Ok(CelValue::String(Arc::new(s))),
        JsonValue::Array(arr) => {
            let cel_list: Result<Vec<CelValue>> = arr.into_iter().map(json_to_cel_value).collect();
            Ok(CelValue::List(Arc::new(cel_list?)))
        }
        JsonValue::Object(obj) => {
            let mut cel_map: HashMap<Key, CelValue> = HashMap::new();
            for (key, value) in obj {
                cel_map.insert(Key::String(Arc::new(key)), json_to_cel_value(value)?);
            }
            Ok(CelValue::Map(Map {
                map: Arc::new(cel_map),
            }))
        }
    }
}

/// Simple base64 encoding helper (standard alphabet)
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in input.chunks(3) {
        let b1 = chunk[0];
        let b2 = chunk.get(1).copied().unwrap_or(0);
        let b3 = chunk.get(2).copied().unwrap_or(0);

        result.push(ALPHABET[(b1 >> 2) as usize] as char);
        result.push(ALPHABET[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);
        result.push(if chunk.len() > 1 {
            ALPHABET[(((b2 & 0x0f) << 2) | (b3 >> 6)) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            ALPHABET[(b3 & 0x3f) as usize] as char
        } else {
            '='
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_cel_expression() {
        let env = CelEnvironment::new();

        // Simple arithmetic expression (no input needed)
        let result = env.execute("1 + 1", &[]).unwrap();
        assert_eq!(result, JsonValue::Number(2.into()));
    }

    #[test]
    fn test_cel_value_conversions() {
        let env = CelEnvironment::new();

        // Test various CEL value types
        assert_eq!(
            env.execute("'hello'", &[]).unwrap(),
            JsonValue::String("hello".to_string())
        );

        assert_eq!(env.execute("true", &[]).unwrap(), JsonValue::Bool(true));

        assert_eq!(env.execute("null", &[]).unwrap(), JsonValue::Null);
    }

    #[test]
    fn test_cel_map_to_json() {
        let env = CelEnvironment::new();

        let result = env.execute("{'key': 'value', 'number': 42}", &[]).unwrap();

        let expected = serde_json::json!({
            "key": "value",
            "number": 42
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_invalid_expression() {
        let env = CelEnvironment::new();

        let result = env.execute("invalid syntax here!", &[]);
        assert!(result.is_err());
        assert!(matches!(result, Err(PayloadError::CelCompilationError(_))));
    }

    #[test]
    fn test_undefined_variable() {
        let env = CelEnvironment::new();

        let result = env.execute("undefined_var", &[]);
        assert!(result.is_err());
        assert!(matches!(result, Err(PayloadError::CelExecutionError(_))));
    }

    #[test]
    fn test_cayenne_lpp_decode_function() {
        let env = CelEnvironment::new();

        // Temperature sensor: Channel 1, Type 103 (temp), Value 0x0110 (27.2°C)
        let payload = vec![0x01, 0x67, 0x01, 0x10];

        let result = env.execute("cayenne_lpp_decode(input)", &payload).unwrap();

        let expected = serde_json::json!({
            "temperature_1": 27.2
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_json_to_cel_value_conversions() {
        // Test all JSON types convert to CEL correctly
        let null_val = json_to_cel_value(JsonValue::Null).unwrap();
        assert!(matches!(null_val, CelValue::Null));

        let bool_val = json_to_cel_value(JsonValue::Bool(true)).unwrap();
        assert!(matches!(bool_val, CelValue::Bool(true)));

        let int_val = json_to_cel_value(serde_json::json!(42)).unwrap();
        assert!(matches!(int_val, CelValue::Int(42)));

        let str_val = json_to_cel_value(JsonValue::String("test".into())).unwrap();
        assert!(matches!(str_val, CelValue::String(_)));

        let arr_val = json_to_cel_value(serde_json::json!([1, 2, 3])).unwrap();
        assert!(matches!(arr_val, CelValue::List(_)));

        let obj_val = json_to_cel_value(serde_json::json!({"key": "value"})).unwrap();
        assert!(matches!(obj_val, CelValue::Map(_)));
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"test"), "dGVzdA==");
        assert_eq!(base64_encode(b"a"), "YQ==");
    }

    #[test]
    fn test_cayenne_lpp_decode_multi_sensor() {
        let env = CelEnvironment::new();

        // Multiple sensors: Temperature + Humidity
        let payload = vec![
            0x00, 0x67, 0x00, 0xFA, // Channel 0, Temperature, 25.0°C
            0x01, 0x68, 0x78, // Channel 1, Humidity, 60.0%
        ];

        let result = env.execute("cayenne_lpp_decode(input)", &payload).unwrap();

        let expected = serde_json::json!({
            "temperature_0": 25.0,
            "humidity_1": 60.0
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_cayenne_lpp_decode_invalid_payload() {
        let env = CelEnvironment::new();

        // Invalid: insufficient data
        let payload = vec![0x01, 0x67]; // Missing data bytes

        let result = env.execute("cayenne_lpp_decode(input)", &payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_cayenne_lpp_decode_unsupported_type() {
        let env = CelEnvironment::new();

        // Invalid: unsupported sensor type 0xFF
        let payload = vec![0x01, 0xFF, 0x00, 0x00];

        let result = env.execute("cayenne_lpp_decode(input)", &payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_greenhouse_transformation() {
        let env = CelEnvironment::new();

        // Greenhouse payload: Temperature on channel 0, Humidity on channel 1
        let payload = vec![
            0x00, 0x67, 0x00, 0xFA, // Channel 0, Temperature, 25.0°C
            0x01, 0x68, 0x78, // Channel 1, Humidity, 60.0%
        ];

        // Transform Cayenne LPP output to custom greenhouse format
        // Note: We decode once and use field access to extract values
        let expression = r#"
            {
                'greenhouse_temperature': cayenne_lpp_decode(input).temperature_0,
                'greenhouse_humidity': cayenne_lpp_decode(input).humidity_1,
                'sensor_type': 'greenhouse'
            }
        "#;

        let result = env.execute(expression, &payload).unwrap();

        let expected = serde_json::json!({
            "greenhouse_temperature": 25.0,
            "greenhouse_humidity": 60.0,
            "sensor_type": "greenhouse"
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_complex_transformation_with_units() {
        let env = CelEnvironment::new();

        // Multi-sensor payload
        let payload = vec![
            0x00, 0x67, 0x01, 0x10, // Temperature: 27.2°C
            0x01, 0x68, 0x78, // Humidity: 60.0%
        ];

        // Transform with unit information and metadata
        // Note: Multiple calls to cayenne_lpp_decode, but the decoder is stateless so it's fine
        let expression = r#"
            {
                'device_id': 'greenhouse-001',
                'timestamp': '2025-11-19T14:00:00Z',
                'readings': {
                    'temperature': {
                        'value': cayenne_lpp_decode(input).temperature_0,
                        'unit': 'celsius'
                    },
                    'humidity': {
                        'value': cayenne_lpp_decode(input).humidity_1,
                        'unit': 'percent'
                    }
                },
                'alerts': cayenne_lpp_decode(input).temperature_0 > 25.0 ? ['high_temperature'] : []
            }
        "#;

        let result = env.execute(expression, &payload).unwrap();

        let expected = serde_json::json!({
            "device_id": "greenhouse-001",
            "timestamp": "2025-11-19T14:00:00Z",
            "readings": {
                "temperature": {
                    "value": 27.2,
                    "unit": "celsius"
                },
                "humidity": {
                    "value": 60.0,
                    "unit": "percent"
                }
            },
            "alerts": ["high_temperature"]
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_conditional_field_mapping() {
        let env = CelEnvironment::new();

        // Temperature sensor
        let payload = vec![0x00, 0x67, 0x00, 0xC8]; // 20.0°C

        // Map to different field names based on value
        let expression = r#"
            cayenne_lpp_decode(input).temperature_0 > 25.0
                ? {'status': 'hot', 'temp_celsius': cayenne_lpp_decode(input).temperature_0}
                : {'status': 'comfortable', 'temp_celsius': cayenne_lpp_decode(input).temperature_0}
        "#;

        let result = env.execute(expression, &payload).unwrap();

        let expected = serde_json::json!({
            "status": "comfortable",
            "temp_celsius": 20.0
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_map_macro_single_decode() {
        let env = CelEnvironment::new();

        // Greenhouse payload: Temperature on channel 0, Humidity on channel 1
        let payload = vec![
            0x00, 0x67, 0x00, 0xFA, // Channel 0, Temperature, 25.0°C
            0x01, 0x68, 0x78, // Channel 1, Humidity, 60.0%
        ];

        // Use map() to bind decoded result to variable and reference it multiple times
        let expression = r#"
            [cayenne_lpp_decode(input)].map(decoded, {
                'greenhouse_temperature': decoded.temperature_0,
                'greenhouse_humidity': decoded.humidity_1,
                'sensor_type': 'greenhouse',
                'alert': decoded.temperature_0 > 24.0 && decoded.humidity_1 > 50.0 ? 'high_both' : 'normal'
            })[0]
        "#;

        let result = env.execute(expression, &payload).unwrap();

        let expected = serde_json::json!({
            "greenhouse_temperature": 25.0,
            "greenhouse_humidity": 60.0,
            "sensor_type": "greenhouse",
            "alert": "high_both"
        });

        assert_eq!(result, expected);
    }
}

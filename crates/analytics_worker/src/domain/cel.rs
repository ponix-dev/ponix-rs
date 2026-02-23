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
use cel_core::types::{CelType, FunctionDecl, OverloadDecl};
use cel_core::{
    Ast, Env, EvalError, EvalErrorKind, MapActivation, MapKey, Value as CelValue, ValueMap,
};
use cel_core_proto::{check_result_from_proto, from_parsed_expr, CheckedExpr};
use prost::Message;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Create a CEL environment with standard library, `input` variable, and custom functions.
///
/// This is the shared environment setup used by both `CelEnvironment` (runtime execution)
/// and `CelCompiler` (compile-time expression compilation).
pub fn create_cel_env() -> Env {
    let cayenne_lpp_decode = FunctionDecl::new("cayenne_lpp_decode").with_overload(
        OverloadDecl::function(
            "cayenne_lpp_decode_bytes",
            vec![CelType::Bytes],
            CelType::Dyn,
        )
        .with_impl(|args: &[CelValue]| {
            let bytes: Arc<[u8]> = match &args[0] {
                CelValue::Bytes(b) => Arc::clone(b),
                other => {
                    return CelValue::error(EvalError::new(
                        EvalErrorKind::InvalidArgument,
                        format!(
                            "cayenne_lpp_decode expects bytes, got {:?}",
                            other.cel_type()
                        ),
                    ));
                }
            };

            let decoder = CayenneLppDecoder::new();
            match decoder.decode(&bytes) {
                Ok(json_value) => match json_to_cel_value(json_value) {
                    Ok(cel_val) => cel_val,
                    Err(e) => CelValue::error(EvalError::new(
                        EvalErrorKind::InvalidArgument,
                        format!("cayenne_lpp_decode conversion error: {}", e),
                    )),
                },
                Err(e) => CelValue::error(EvalError::new(
                    EvalErrorKind::InvalidArgument,
                    format!("cayenne_lpp_decode error: {}", e),
                )),
            }
        }),
    );

    Env::with_standard_library()
        .with_variable("input", CelType::Bytes)
        .with_function(cayenne_lpp_decode)
}

/// CEL execution environment with registered custom functions
///
/// The `CelEnvironment` provides a simple API for executing CEL expressions
/// against binary payloads. Custom decoder functions are automatically registered
/// when the environment is created.
pub struct CelEnvironment {
    env: Env,
}

impl CelEnvironment {
    /// Create a new CEL environment with cayenne_lpp_decode function registered
    pub fn new() -> Self {
        Self {
            env: create_cel_env(),
        }
    }

    /// Execute a pre-compiled CEL expression with binary input data
    ///
    /// # Arguments
    /// * `compiled` - Serialized CheckedExpr bytes (from CelCompiler)
    /// * `source` - Original expression source (for error messages/tracing)
    /// * `input` - Binary payload data
    ///
    /// # Returns
    /// * `Ok(JsonValue)` - Valid JSON output from expression
    /// * `Err(PayloadError)` - Deserialization, execution, or validation error
    #[instrument(
        name = "cel_execute",
        skip(self, compiled, input),
        fields(
            source = %source,
            input_size = input.len(),
        )
    )]
    pub fn execute(&self, compiled: &[u8], source: &str, input: &[u8]) -> Result<JsonValue> {
        let cel_value = self.execute_raw(compiled, source, CelValue::from(input.to_vec()))?;
        let json_result = cel_value_to_json(cel_value)?;
        debug!("CEL expression executed successfully");
        Ok(json_result)
    }

    /// Evaluate a pre-compiled CEL expression that should return a boolean
    ///
    /// # Arguments
    /// * `compiled` - Serialized CheckedExpr bytes (from CelCompiler)
    /// * `source` - Original expression source (for error messages/tracing)
    /// * `input` - CelValue to bind as the `input` variable
    ///
    /// # Returns
    /// * `Ok(bool)` - The boolean result
    /// * `Err(PayloadError)` - If expression doesn't return a boolean or fails
    pub fn evaluate_bool(&self, compiled: &[u8], source: &str, input: CelValue) -> Result<bool> {
        let result = self.execute_raw(compiled, source, input)?;
        match result {
            CelValue::Bool(b) => Ok(b),
            other => Err(PayloadError::CelExecutionError(format!(
                "Expected boolean result, got {:?}",
                other.cel_type()
            ))),
        }
    }

    /// Internal: deserialize compiled bytes → AST → Program → eval
    fn execute_raw(&self, compiled: &[u8], source: &str, input: CelValue) -> Result<CelValue> {
        if compiled.is_empty() {
            return Err(PayloadError::CelCompilationError(
                format!("Missing pre-compiled CEL expression for '{}' — contracts must be compiled at write time", source),
            ));
        }

        let checked = CheckedExpr::decode(compiled).map_err(|e| {
            PayloadError::CelCompilationError(format!("Failed to decode compiled expression: {e}"))
        })?;

        let parsed_expr = checked.expr.as_ref().ok_or_else(|| {
            PayloadError::CelCompilationError("Missing expr in CheckedExpr".to_string())
        })?;

        let spanned = from_parsed_expr(&cel_core_proto::ParsedExpr {
            expr: Some(parsed_expr.clone()),
            source_info: checked.source_info.clone(),
        })
        .map_err(|e| {
            PayloadError::CelCompilationError(format!("Failed to reconstruct AST: {e}"))
        })?;

        let check_result = check_result_from_proto(&checked).map_err(|e| {
            PayloadError::CelCompilationError(format!("Failed to reconstruct type info: {e}"))
        })?;

        let ast = Ast::new_checked(spanned, source, check_result);

        let program = self
            .env
            .program(&ast)
            .map_err(|e| PayloadError::CelCompilationError(e.to_string()))?;

        let mut activation = MapActivation::new();
        activation.insert("input", input);

        let result = program.eval(&activation);

        if result.is_error() {
            return Err(PayloadError::CelExecutionError(format!("{}", result)));
        }

        Ok(result)
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
            serde_json::from_str(&s).or_else(|_| Ok(JsonValue::String(s.to_string())))
        }
        CelValue::Int(i) => Ok(JsonValue::Number(i.into())),
        CelValue::UInt(u) => serde_json::Number::from_f64(u as f64)
            .map(JsonValue::Number)
            .ok_or(PayloadError::InvalidJsonOutput),
        CelValue::Double(f) => serde_json::Number::from_f64(f)
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
            for (key, value) in map.iter() {
                let key_str = match key {
                    MapKey::String(s) => s.to_string(),
                    MapKey::Int(i) => i.to_string(),
                    MapKey::UInt(u) => u.to_string(),
                    MapKey::Bool(b) => b.to_string(),
                };
                let json_value = cel_value_to_json(value.clone())?;
                json_map.insert(key_str, json_value);
            }
            Ok(JsonValue::Object(json_map))
        }
        CelValue::Null => Ok(JsonValue::Null),
        CelValue::Error(err) => Err(PayloadError::CelExecutionError(err.to_string())),
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
                Ok(CelValue::Double(f))
            } else {
                Err(PayloadError::InvalidJsonOutput)
            }
        }
        JsonValue::String(s) => Ok(CelValue::String(Arc::from(s.as_str()))),
        JsonValue::Array(arr) => {
            let cel_list: Result<Vec<CelValue>> = arr.into_iter().map(json_to_cel_value).collect();
            Ok(CelValue::List(Arc::from(cel_list?)))
        }
        JsonValue::Object(obj) => {
            let mut cel_map = ValueMap::new();
            for (key, value) in obj {
                cel_map.insert(
                    MapKey::String(Arc::from(key.as_str())),
                    json_to_cel_value(value)?,
                );
            }
            Ok(CelValue::Map(Arc::new(cel_map)))
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
    use crate::domain::CelCompiler;
    use common::cel::CelExpressionCompiler;

    fn compile(expr: &str) -> Vec<u8> {
        CelCompiler::new().compile(expr).unwrap()
    }

    #[test]
    fn test_basic_cel_expression() {
        let env = CelEnvironment::new();

        // Simple arithmetic expression (no input needed)
        let result = env.execute(&compile("1 + 1"), "1 + 1", &[]).unwrap();
        assert_eq!(result, JsonValue::Number(2.into()));
    }

    #[test]
    fn test_cel_value_conversions() {
        let env = CelEnvironment::new();

        // Test various CEL value types
        assert_eq!(
            env.execute(&compile("'hello'"), "'hello'", &[]).unwrap(),
            JsonValue::String("hello".to_string())
        );

        assert_eq!(
            env.execute(&compile("true"), "true", &[]).unwrap(),
            JsonValue::Bool(true)
        );

        assert_eq!(
            env.execute(&compile("null"), "null", &[]).unwrap(),
            JsonValue::Null
        );
    }

    #[test]
    fn test_cel_map_to_json() {
        let env = CelEnvironment::new();

        let expr = "{'key': 'value', 'number': 42}";
        let result = env.execute(&compile(expr), expr, &[]).unwrap();

        let expected = serde_json::json!({
            "key": "value",
            "number": 42
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_invalid_expression_fails_at_compile() {
        // Invalid expressions now fail at compile time, not at execute time
        let compiler = CelCompiler::new();
        let result = compiler.compile("invalid syntax here!");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_compiled_bytes_rejects_with_clear_error() {
        let env = CelEnvironment::new();

        let result = env.execute(&[], "cayenne_lpp_decode(input)", &[]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, PayloadError::CelCompilationError(_)));
        let msg = err.to_string();
        assert!(
            msg.contains("Missing pre-compiled CEL expression"),
            "Expected clear error message, got: {}",
            msg
        );
    }

    #[test]
    fn test_empty_compiled_bytes_rejects_evaluate_bool() {
        let env = CelEnvironment::new();

        let result = env.evaluate_bool(&[], "true", CelValue::from(vec![0u8]));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Missing pre-compiled CEL expression"),
            "Expected clear error message, got: {}",
            msg
        );
    }

    #[test]
    fn test_invalid_compiled_bytes() {
        let env = CelEnvironment::new();

        // Garbage bytes should fail to deserialize
        let result = env.execute(&[0xFF, 0xFE, 0xFD], "bad bytes", &[]);
        assert!(result.is_err());
        assert!(matches!(result, Err(PayloadError::CelCompilationError(_))));
    }

    #[test]
    fn test_cayenne_lpp_decode_function() {
        let env = CelEnvironment::new();

        // Temperature sensor: Channel 1, Type 103 (temp), Value 0x0110 (27.2°C)
        let payload = vec![0x01, 0x67, 0x01, 0x10];

        let expr = "cayenne_lpp_decode(input)";
        let result = env.execute(&compile(expr), expr, &payload).unwrap();

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

        let expr = "cayenne_lpp_decode(input)";
        let result = env.execute(&compile(expr), expr, &payload).unwrap();

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

        let expr = "cayenne_lpp_decode(input)";
        let result = env.execute(&compile(expr), expr, &payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_cayenne_lpp_decode_unsupported_type() {
        let env = CelEnvironment::new();

        // Invalid: unsupported sensor type 0xFF
        let payload = vec![0x01, 0xFF, 0x00, 0x00];

        let expr = "cayenne_lpp_decode(input)";
        let result = env.execute(&compile(expr), expr, &payload);
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

        let result = env
            .execute(&compile(expression), expression, &payload)
            .unwrap();

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

        let result = env
            .execute(&compile(expression), expression, &payload)
            .unwrap();

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

        let result = env
            .execute(&compile(expression), expression, &payload)
            .unwrap();

        let expected = serde_json::json!({
            "status": "comfortable",
            "temp_celsius": 20.0
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_evaluate_bool_true() {
        let env = CelEnvironment::new();
        let result = env
            .evaluate_bool(&compile("true"), "true", CelValue::from(vec![0u8]))
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_evaluate_bool_false() {
        let env = CelEnvironment::new();
        let result = env
            .evaluate_bool(&compile("false"), "false", CelValue::from(vec![0u8]))
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_bool_expression() {
        let env = CelEnvironment::new();
        // Payload size > 2
        let expr = "size(input) > 2";
        let result = env
            .evaluate_bool(&compile(expr), expr, CelValue::from(vec![1u8, 2, 3, 4]))
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_evaluate_bool_non_bool_result() {
        let env = CelEnvironment::new();
        let result = env.evaluate_bool(&compile("42"), "42", CelValue::from(vec![0u8]));
        assert!(result.is_err());
        assert!(matches!(result, Err(PayloadError::CelExecutionError(_))));
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

        let result = env
            .execute(&compile(expression), expression, &payload)
            .unwrap();

        let expected = serde_json::json!({
            "greenhouse_temperature": 25.0,
            "greenhouse_humidity": 60.0,
            "sensor_type": "greenhouse",
            "alert": "high_both"
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_compile_serialize_deserialize_execute_roundtrip() {
        let env = CelEnvironment::new();

        let expression = "cayenne_lpp_decode(input)";
        let payload = vec![0x01, 0x67, 0x01, 0x10]; // Temperature: 27.2°C

        // Compile to bytes
        let compiled_bytes = compile(expression);

        // Simulate serialization roundtrip: clone the bytes as if they were
        // stored in a database and retrieved later
        let serialized = compiled_bytes.clone();
        let deserialized = serialized;

        // Execute with the original compiled bytes
        let result1 = env.execute(&compiled_bytes, expression, &payload).unwrap();

        // Execute with the deserialized bytes
        let result2 = env.execute(&deserialized, expression, &payload).unwrap();

        let expected = serde_json::json!({
            "temperature_1": 27.2
        });

        assert_eq!(result1, expected);
        assert_eq!(result2, expected);
        assert_eq!(result1, result2);
    }
}

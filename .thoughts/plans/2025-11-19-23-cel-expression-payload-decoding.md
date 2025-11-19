# CEL Expression Support for Configurable Payload Decoding Implementation Plan

## Overview

Add CEL (Common Expression Language) support to the `ponix-payload` crate to enable configurable, expression-based payload decoding. This allows users to write CEL expressions that transform raw binary data into JSON using pre-registered decoder functions, starting with `cayenne_lpp_decode` that leverages the existing Cayenne LPP decoder implementation from issue #22.

## Current State Analysis

### Existing Implementation
- **ponix-payload crate** at [crates/ponix-payload/](crates/ponix-payload/) is fully functional
- **CayenneLppDecoder** implemented in [cayenne_lpp.rs:175-528](crates/ponix-payload/src/cayenne_lpp.rs#L175-L528)
- **PayloadDecoder trait** defined in [lib.rs:7-11](crates/ponix-payload/src/lib.rs#L7-L11)
- Decoder outputs `serde_json::Value` directly (already valid JSON)
- Comprehensive test coverage with 100+ unit tests
- Current dependencies: serde, serde_json, thiserror, chrono

### Key Discoveries
- Cayenne LPP decoder is production-ready with 27 sensor type support
- Public API: `CayenneLppDecoder::new().decode(bytes) -> Result<Value>`
- Error handling via [PayloadError](crates/ponix-payload/src/error.rs:4-19) enum
- Binary data handling already proven with extensive test coverage
- JSON output validation built-in via serde_json

### CEL Library Selection
Using the **`cel-interpreter`** crate (version 0.11+) because:
- Pure Rust implementation (no C++ dependencies)
- MIT licensed
- Supports custom function registration via `Context`
- Active maintenance by cel-rust organization
- Matches user preference for `cel` crate specifically

## Desired End State

Users can execute CEL expressions that transform binary payloads into JSON, with support for both string expressions and precompiled protobuf format:

### String Expression Execution
```rust
use ponix_payload::cel::CelEnvironment;

// 1. Create CEL environment (cayenne_lpp_decode is automatically registered)
let env = CelEnvironment::new();

// 2. Execute expression with binary input
let expression = "cayenne_lpp_decode(input)";
let binary_data = vec![0x01, 0x67, 0x01, 0x10]; // Temperature sensor
let result = env.execute(expression, &binary_data)?;

// 3. Result is guaranteed valid JSON
assert_eq!(result, json!({"temperature_1": 27.2}));
```

### Precompiled Expression Support (CheckedExpr)
```rust
use ponix_payload::cel::CelEnvironment;

let env = CelEnvironment::new();

// Compile expression to CheckedExpr protobuf format for reuse
// This includes type-checking information for fast evaluation
let expression = "cayenne_lpp_decode(input)";
let compiled_proto = env.compile_to_proto(expression)?;

// Store compiled_proto in database/config for later use...

// Execute precompiled expression (much faster - skips parsing and type-checking)
let binary_data = vec![0x01, 0x67, 0x01, 0x10];
let result = env.execute_compiled(&compiled_proto, &binary_data)?;

assert_eq!(result, json!({"temperature_1": 27.2}));
```

### Verification Criteria
- CEL expressions can be provided as runtime strings (configurable)
- Expressions can be precompiled to Google CEL `CheckedExpr` protobuf format for performance
- Precompiled expressions can be executed with same API
- Precompiled execution skips expensive parsing and type-checking steps
- `cayenne_lpp_decode(input)` function works with binary data
- Output is always valid JSON or returns error
- Simple initialization with `CelEnvironment::new()`
- No integration with NATS pipeline (standalone capability)

## What We're NOT Doing

- NATS pipeline integration (deferred to future work)
- Dynamic function registration after environment creation
- Storage or caching of compiled expressions (caller's responsibility)
- Expression validation UI/tooling
- Multi-expression composition or chaining
- Async CEL execution
- Custom CEL type definitions beyond standard types
- Expression sandboxing/resource limits
- In-memory caching of CheckedExpr or Program instances

## Implementation Approach

**Architecture**: Extend ponix-payload with new `cel` module that wraps cel-interpreter library. The `CelEnvironment` struct will automatically register the `cayenne_lpp_decode` function during initialization. Functions bridge between CEL's type system and Rust native types, with special handling for binary data (Vec<u8>) input and JSON output validation.

**Key Design Decisions**:
1. **Input variable name**: Use `input` (matches issue example)
2. **Function registration**: Automatic during `CelEnvironment::new()` initialization
3. **Error handling**: Propagate errors as Result types with custom CelError variants
4. **JSON validation**: Automatic via serde_json serialization round-trip
5. **Simplicity**: No builder pattern - just `new()` for straightforward initialization
6. **Precompiled expressions**: Support both string expressions and precompiled protobuf format for performance

## Phase 1: Add CEL Dependency and Error Types

### Overview
Add the cel-interpreter crate to workspace dependencies and create error types for CEL-specific failures that integrate with existing PayloadError.

### Changes Required

#### 1. Workspace Dependencies
**File**: [Cargo.toml:21-34](Cargo.toml#L21-L34)
**Changes**: Add cel-interpreter and cel-parser to workspace dependencies

```toml
[workspace.dependencies]
tokio = { version = "1.40", features = ["full"] }
tokio-util = "0.7"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1.0"
thiserror = "1.0"
prost = "0.13"
prost-types = "0.13"
xid = "1.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
cel-interpreter = "0.11"  # Add this line
cel-parser = "0.11"  # Add this line for CheckedExpr support
```

#### 2. Crate Dependencies
**File**: [crates/ponix-payload/Cargo.toml:6-10](crates/ponix-payload/Cargo.toml#L6-L10)
**Changes**: Add cel-interpreter and cel-parser as dependencies

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
cel-interpreter = { workspace = true }  # Add this line
cel-parser = { workspace = true }  # Add this line for CheckedExpr support
prost = { workspace = true }  # For protobuf serialization
```

#### 3. CEL Error Types
**File**: [crates/ponix-payload/src/error.rs:4-19](crates/ponix-payload/src/error.rs#L4-L19)
**Changes**: Extend PayloadError with CEL-specific variants

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PayloadError {
    #[error("insufficient data: expected {expected} bytes, got {actual}")]
    InsufficientData { expected: usize, actual: usize },

    #[error("unsupported sensor type: {0}")]
    UnsupportedType(u8),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    // Add these new variants:
    #[error("CEL compilation error: {0}")]
    CelCompilationError(String),

    #[error("CEL execution error: {0}")]
    CelExecutionError(String),

    #[error("invalid JSON output from CEL expression")]
    InvalidJsonOutput,
}

pub type Result<T> = std::result::Result<T, PayloadError>;
```

### Success Criteria

#### Automated Verification
- [x] Project builds successfully: `cargo build -p ponix-payload`
- [x] No dependency resolution errors: `cargo check -p ponix-payload`
- [x] Workspace dependencies resolve correctly: `cargo tree -p ponix-payload | grep cel-interpreter`

#### Manual Verification
- [x] New error variants compile without warnings
- [x] Error display strings are clear and actionable

**Implementation Note**: After completing this phase and all automated verification passes, proceed to Phase 2.

---

## Phase 2: Create CEL Module with Environment

### Overview
Create the core CEL integration module with environment management and the basic infrastructure for executing CEL expressions with binary input. The `cayenne_lpp_decode` function will be automatically registered during initialization.

### Changes Required

#### 1. New CEL Module
**File**: `crates/ponix-payload/src/cel.rs` (new file)
**Changes**: Create comprehensive CEL environment infrastructure

```rust
use crate::cayenne_lpp::CayenneLppDecoder;
use crate::error::{PayloadError, Result};
use cel_interpreter::{Context, Program, Value as CelValue};
use cel_parser::Checked;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// CEL execution environment with registered custom functions
#[derive(Clone)]
pub struct CelEnvironment {
    context: Arc<Context>,
}

impl CelEnvironment {
    /// Create a new CEL environment with cayenne_lpp_decode function registered
    pub fn new() -> Self {
        let mut context = Context::default();

        // Register cayenne_lpp_decode function
        let decoder = CayenneLppDecoder::new();
        context.add_function(
            "cayenne_lpp_decode",
            move |bytes: Vec<u8>| -> std::result::Result<CelValue, String> {
                decoder.decode(&bytes)
                    .map_err(|e| e.to_string())
                    .and_then(|json_value| {
                        json_to_cel_value(json_value)
                            .map_err(|e| e.to_string())
                    })
            }
        );

        Self {
            context: Arc::new(context),
        }
    }

    /// Compile a CEL expression to Google CEL CheckedExpr protobuf format
    ///
    /// Converts a CEL expression string into a protobuf-encoded `CheckedExpr`
    /// following the Google CEL specification. This includes type-checking information,
    /// allowing the expression to be converted back to an AST and evaluated without
    /// re-compilation. The caller is responsible for storing this protobuf (e.g., in
    /// database, config files, etc.).
    ///
    /// # Arguments
    /// * `expression` - CEL expression string (e.g., "cayenne_lpp_decode(input)")
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - Protobuf-encoded `CheckedExpr` message
    /// * `Err(PayloadError)` - Compilation or type-checking error
    ///
    /// # Example
    /// ```rust
    /// let env = CelEnvironment::new();
    /// let proto = env.compile_to_proto("cayenne_lpp_decode(input)")?;
    /// // Caller stores proto bytes externally (database, etc.)
    /// ```
    pub fn compile_to_proto(&self, expression: &str) -> Result<Vec<u8>> {
        // Parse and type-check the expression
        let checked = cel_parser::parse(expression)
            .map_err(|e| PayloadError::CelCompilationError(e.to_string()))?;

        // Serialize the CheckedExpr to protobuf bytes
        let proto_bytes = checked.serialize()
            .map_err(|e| PayloadError::CelCompilationError(e.to_string()))?;

        Ok(proto_bytes)
    }

    /// Execute a protobuf-encoded CEL expression with binary input
    ///
    /// Takes a protobuf `CheckedExpr` message (from `compile_to_proto()`),
    /// converts it to an AST, and executes it with the input data. This approach
    /// is much faster than re-compiling from a string because the type-checking
    /// step is skipped.
    ///
    /// # Arguments
    /// * `compiled_proto` - Protobuf-encoded `CheckedExpr` message
    /// * `input` - Binary payload data
    ///
    /// # Returns
    /// * `Ok(JsonValue)` - Valid JSON output from expression
    /// * `Err(PayloadError)` - Deserialization or execution error
    ///
    /// # Example
    /// ```rust
    /// let env = CelEnvironment::new();
    /// let proto = env.compile_to_proto("cayenne_lpp_decode(input)")?;
    /// let result = env.execute_compiled(&proto, &payload)?;
    /// ```
    pub fn execute_compiled(&self, compiled_proto: &[u8], input: &[u8]) -> Result<JsonValue> {
        // Deserialize CheckedExpr from protobuf bytes
        let checked = Checked::deserialize(compiled_proto)
            .map_err(|e| PayloadError::CelCompilationError(e.to_string()))?;

        // Convert CheckedExpr to Program (AST)
        // This is much faster than re-parsing and type-checking from scratch
        let program = checked.to_program()
            .map_err(|e| PayloadError::CelCompilationError(e.to_string()))?;

        // Execute using shared logic
        self.execute_program(&program, input)
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

        // Execute using shared logic
        self.execute_program(&program, input)
    }

    /// Internal helper to execute a compiled Program with input data
    fn execute_program(&self, program: &Program, input: &[u8]) -> Result<JsonValue> {
        // Create a child context with the input variable bound
        let mut exec_context = Context::default();

        // Add input variable as bytes
        // Note: cel-interpreter's API for add_variable needs verification
        // This is the pattern based on available documentation
        let input_value = CelValue::Bytes(input.to_vec().into());
        exec_context.add_variable("input", input_value)
            .map_err(|e| PayloadError::CelExecutionError(e.to_string()))?;

        // Execute the program with merged context (parent functions + input variable)
        let result = program.execute_with_context(&exec_context, &self.context)
            .map_err(|e| PayloadError::CelExecutionError(e.to_string()))?;

        // Convert CEL value to JSON and validate
        self.cel_value_to_json(result)
    }

    /// Convert CEL Value to serde_json::Value with validation
    fn cel_value_to_json(&self, value: CelValue) -> Result<JsonValue> {
        match value {
            CelValue::String(s) => {
                // If it's a string, try parsing as JSON first
                // Otherwise return as JSON string
                serde_json::from_str(&s)
                    .or_else(|_| Ok(JsonValue::String(s)))
            }
            CelValue::Int(i) => Ok(JsonValue::Number(i.into())),
            CelValue::UInt(u) => Ok(JsonValue::Number(u.into())),
            CelValue::Float(f) => {
                serde_json::Number::from_f64(f)
                    .map(JsonValue::Number)
                    .ok_or(PayloadError::InvalidJsonOutput)
            }
            CelValue::Bool(b) => Ok(JsonValue::Bool(b)),
            CelValue::Bytes(b) => {
                // Bytes can't be directly represented in JSON
                // Encode as base64 string
                Ok(JsonValue::String(base64::encode(&b)))
            }
            CelValue::List(items) => {
                let json_items: Result<Vec<JsonValue>> = items
                    .into_iter()
                    .map(|item| self.cel_value_to_json(item))
                    .collect();
                Ok(JsonValue::Array(json_items?))
            }
            CelValue::Map(map) => {
                let mut json_map = serde_json::Map::new();
                for (key, value) in map {
                    let key_str = match key {
                        CelValue::String(s) => s,
                        _ => return Err(PayloadError::InvalidJsonOutput),
                    };
                    let json_value = self.cel_value_to_json(value)?;
                    json_map.insert(key_str, json_value);
                }
                Ok(JsonValue::Object(json_map))
            }
            CelValue::Null => Ok(JsonValue::Null),
            _ => Err(PayloadError::InvalidJsonOutput),
        }
    }
}

impl Default for CelEnvironment {
    fn default() -> Self {
        Self::new()
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
        JsonValue::String(s) => Ok(CelValue::String(s.into())),
        JsonValue::Array(arr) => {
            let cel_list: Result<Vec<CelValue>> = arr
                .into_iter()
                .map(json_to_cel_value)
                .collect();
            Ok(CelValue::List(cel_list?.into()))
        }
        JsonValue::Object(obj) => {
            let mut cel_map = std::collections::HashMap::new();
            for (key, value) in obj {
                cel_map.insert(
                    CelValue::String(key.into()),
                    json_to_cel_value(value)?
                );
            }
            Ok(CelValue::Map(cel_map.into()))
        }
    }
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

        assert_eq!(
            env.execute("true", &[]).unwrap(),
            JsonValue::Bool(true)
        );

        assert_eq!(
            env.execute("null", &[]).unwrap(),
            JsonValue::Null
        );
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
    fn test_compile_to_proto() {
        let env = CelEnvironment::new();

        // Should successfully compile expression to protobuf
        let compiled = env.compile_to_proto("cayenne_lpp_decode(input)").unwrap();

        // Should produce non-empty protobuf bytes
        assert!(!compiled.is_empty());
    }

    #[test]
    fn test_execute_compiled() {
        let env = CelEnvironment::new();

        // Compile expression
        let expression = "cayenne_lpp_decode(input)";
        let compiled = env.compile_to_proto(expression).unwrap();

        // Execute precompiled expression
        let payload = vec![0x01, 0x67, 0x01, 0x10]; // Temperature: 27.2°C
        let result = env.execute_compiled(&compiled, &payload).unwrap();

        let expected = serde_json::json!({
            "temperature_1": 27.2
        });

        assert_eq!(result, expected);
    }

    #[test]
    fn test_compiled_vs_string_equivalence() {
        let env = CelEnvironment::new();

        let expression = "cayenne_lpp_decode(input)";
        let payload = vec![0x00, 0x67, 0x00, 0xFA, 0x01, 0x68, 0x78]; // Temp + Humidity

        // Execute as string
        let result_string = env.execute(expression, &payload).unwrap();

        // Execute as compiled
        let compiled = env.compile_to_proto(expression).unwrap();
        let result_compiled = env.execute_compiled(&compiled, &payload).unwrap();

        // Both should produce identical results
        assert_eq!(result_string, result_compiled);
    }

    #[test]
    fn test_compile_invalid_expression() {
        let env = CelEnvironment::new();

        // Should fail to compile invalid syntax
        let result = env.compile_to_proto("invalid syntax!");
        assert!(result.is_err());
        assert!(matches!(result, Err(PayloadError::CelCompilationError(_))));
    }
}
```

#### 2. Module Registration
**File**: [crates/ponix-payload/src/lib.rs:1-12](crates/ponix-payload/src/lib.rs#L1-L12)
**Changes**: Add cel module and export public types

```rust
pub mod cayenne_lpp;
mod error;
pub mod cel;  // Add this line

pub use cayenne_lpp::CayenneLppDecoder;
pub use error::{PayloadError, Result};

// Add this export:
pub use cel::CelEnvironment;

pub trait PayloadDecoder {
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value>;
}
```

### Success Criteria

#### Automated Verification
- [ ] Module compiles without errors: `cargo build -p ponix-payload`
- [ ] All unit tests pass: `cargo test -p ponix-payload`
- [ ] No clippy warnings: `cargo clippy -p ponix-payload -- -D warnings`
- [ ] Exports are public and accessible: `cargo doc -p ponix-payload --no-deps`

#### Manual Verification
- [ ] Simple initialization with `CelEnvironment::new()` works
- [ ] Error messages are clear when expressions fail
- [ ] JSON conversion handles all CEL value types correctly
- [ ] `cayenne_lpp_decode(input)` function is available after initialization
- [ ] Expression compilation to protobuf works
- [ ] Precompiled expressions execute correctly
- [ ] String and compiled execution produce identical results

**Implementation Note**: This implementation uses the Google CEL `CheckedExpr` protobuf format:

1. **Compilation Flow**: `string -> cel_parser::parse() -> Checked -> serialize() -> Vec<u8>`
   - `cel_parser::parse()` parses and type-checks the expression
   - The resulting `Checked` struct represents a `CheckedExpr` protobuf message
   - `serialize()` converts it to protobuf bytes for storage

2. **Execution Flow**: `Vec<u8> -> Checked::deserialize() -> to_program() -> Program -> eval()`
   - `Checked::deserialize()` reconstructs the `CheckedExpr` from bytes
   - `to_program()` converts the checked expression to an executable `Program` (AST)
   - This skips the expensive parsing and type-checking steps
   - Evaluation is then performed against the input data

3. **Performance Benefits**:
   - Compilation (parse + type-check) is expensive and done once
   - Execution (AST evaluation) is fast and can be repeated many times
   - Storing `CheckedExpr` allows evaluation without re-compilation

4. **API Verification Needed**:
   - Verify `cel_parser::parse()` API and error handling
   - Confirm `Checked::serialize()` and `Checked::deserialize()` methods exist
   - Verify `Checked::to_program()` conversion
   - Check `Program::eval()` or equivalent execution method
   - Verify how custom functions are registered with `Context`

After completing this phase and all automated verification passes, proceed to Phase 3.

---

## Phase 3: Additional Tests for cayenne_lpp_decode

### Overview
Add comprehensive tests for the `cayenne_lpp_decode` function to verify it works correctly with various sensor payloads and error cases.

### Changes Required

#### 1. Additional Unit Tests
**File**: [crates/ponix-payload/src/cel.rs](crates/ponix-payload/src/cel.rs)
**Changes**: Add more comprehensive tests to the existing test module

```rust
// Add to the existing #[cfg(test)] mod tests block:

#[test]
fn test_cayenne_lpp_decode_multi_sensor() {
    let env = CelEnvironment::new();

    // Multiple sensors: Temperature + Humidity
    let payload = vec![
        0x00, 0x67, 0x00, 0xFA, // Channel 0, Temperature, 25.0°C
        0x01, 0x68, 0x78,       // Channel 1, Humidity, 60.0%
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
```

### Success Criteria

#### Automated Verification
- [ ] All tests pass: `cargo test -p ponix-payload`
- [ ] No clippy warnings: `cargo clippy -p ponix-payload -- -D warnings`
- [ ] Test coverage includes error cases: `cargo test -p ponix-payload -- --nocapture`

#### Manual Verification
- [ ] `cayenne_lpp_decode(input)` expression works with real sensor data
- [ ] Multi-sensor payloads decode correctly
- [ ] Invalid payloads produce clear error messages
- [ ] JSON output matches Cayenne LPP decoder exactly

After completing this phase and all automated verification passes, proceed to Phase 4.

---

## Phase 4: Integration Tests and Documentation

### Overview
Add comprehensive integration tests demonstrating real-world usage patterns and document the public API with examples.

### Changes Required

#### 1. Integration Tests
**File**: `crates/ponix-payload/tests/cel_integration_tests.rs` (new file)
**Changes**: Create integration tests for CEL expressions

```rust
use ponix_payload::cel::CelEnvironment;
use serde_json::json;

#[test]
fn test_simple_cayenne_expression() {
    let env = CelEnvironment::new();

    // Single temperature sensor
    let payload = vec![0x01, 0x67, 0x01, 0x10]; // 27.2°C
    let result = env.execute("cayenne_lpp_decode(input)", &payload).unwrap();

    assert_eq!(result, json!({"temperature_1": 27.2}));
}

#[test]
fn test_environmental_monitoring_payload() {
    let env = CelEnvironment::new();

    // Complex payload: Temperature, Humidity, Barometer
    let payload = vec![
        0x00, 0x67, 0x00, 0xFA, // Temperature: 25.0°C
        0x01, 0x68, 0x78,       // Humidity: 60.0%
        0x02, 0x73, 0x27, 0x10, // Barometer: 1000.0 hPa
    ];

    let result = env.execute("cayenne_lpp_decode(input)", &payload).unwrap();

    let expected = json!({
        "temperature_0": 25.0,
        "humidity_1": 60.0,
        "barometer_2": 1000.0
    });

    assert_eq!(result, expected);
}

#[test]
fn test_gps_tracking_payload() {
    let env = CelEnvironment::new()
        .register_cayenne_lpp_decoder()
        .build()
        .unwrap();

    // GPS coordinates: 42.3519°N, 87.9094°W, 10m altitude
    let payload = vec![
        0x01, 0x88, // Channel 1, GPS type
        0x06, 0x76, 0x5f, // Latitude: 4235190 -> 42.3519°
        0xfd, 0x89, 0xa1, // Longitude: -879094 -> -87.9094°
        0x00, 0x00, 0x64, // Altitude: 100 -> 10.0m
    ];

    let result = env.execute("cayenne_lpp_decode(input)", &payload).unwrap();

    let expected = json!({
        "gps_1": {
            "latitude": 42.3519,
            "longitude": -87.9094,
            "altitude": 10.0
        }
    });

    assert_eq!(result, expected);
}

#[test]
fn test_conditional_expression() {
    let env = CelEnvironment::new()
        .register_cayenne_lpp_decoder()
        .build()
        .unwrap();

    // Temperature sensor
    let payload = vec![0x01, 0x67, 0x01, 0x10]; // 27.2°C

    // CEL expression with conditional logic
    let expression = r#"
        cayenne_lpp_decode(input).temperature_1 > 25.0
        ? {'status': 'hot', 'temp': cayenne_lpp_decode(input).temperature_1}
        : {'status': 'ok', 'temp': cayenne_lpp_decode(input).temperature_1}
    "#;

    let result = env.execute(expression, &payload).unwrap();

    let expected = json!({
        "status": "hot",
        "temp": 27.2
    });

    assert_eq!(result, expected);
}

#[test]
fn test_expression_with_transformation() {
    let env = CelEnvironment::new()
        .register_cayenne_lpp_decoder()
        .build()
        .unwrap();

    // Humidity sensor: 60%
    let payload = vec![0x01, 0x68, 0x78];

    // CEL expression that transforms the output
    let expression = r#"
        {
            'device_id': 'sensor-001',
            'readings': cayenne_lpp_decode(input),
            'timestamp': '2025-11-19T14:00:00Z'
        }
    "#;

    let result = env.execute(expression, &payload).unwrap();

    let expected = json!({
        "device_id": "sensor-001",
        "readings": {
            "humidity_1": 60.0
        },
        "timestamp": "2025-11-19T14:00:00Z"
    });

    assert_eq!(result, expected);
}

#[test]
fn test_multiple_decode_calls() {
    let env = CelEnvironment::new()
        .register_cayenne_lpp_decoder()
        .build()
        .unwrap();

    // Single sensor payload
    let payload = vec![0x01, 0x67, 0x01, 0x10];

    // Expression calls decode multiple times (inefficient but valid)
    let expression = r#"
        {
            'data': cayenne_lpp_decode(input),
            'has_temperature': has(cayenne_lpp_decode(input).temperature_1)
        }
    "#;

    let result = env.execute(expression, &payload).unwrap();

    assert!(result["has_temperature"].as_bool().unwrap());
    assert_eq!(result["data"]["temperature_1"], 27.2);
}

#[test]
fn test_error_handling_in_expression() {
    let env = CelEnvironment::new()
        .register_cayenne_lpp_decoder()
        .build()
        .unwrap();

    // Invalid payload (too short)
    let payload = vec![0x01, 0x67];

    let result = env.execute("cayenne_lpp_decode(input)", &payload);

    // Should propagate error from decoder
    assert!(result.is_err());
}

#[test]
fn test_configurable_expression_at_runtime() {
    let env = CelEnvironment::new();

    let payload = vec![0x01, 0x67, 0x01, 0x10]; // 27.2°C

    // Simulate different configurations
    let expressions = vec![
        "cayenne_lpp_decode(input)",
        "{'decoded': cayenne_lpp_decode(input)}",
        "cayenne_lpp_decode(input).temperature_1",
    ];

    for expr in expressions {
        let result = env.execute(expr, &payload);
        assert!(result.is_ok(), "Expression failed: {}", expr);
    }
}

#[test]
fn test_precompiled_expression_performance() {
    let env = CelEnvironment::new();

    let expression = "cayenne_lpp_decode(input)";
    let payload = vec![0x01, 0x67, 0x01, 0x10];

    // Compile once
    let compiled = env.compile_to_proto(expression).unwrap();

    // Execute multiple times with precompiled version (simulating repeated use)
    for _ in 0..100 {
        let result = env.execute_compiled(&compiled, &payload);
        assert!(result.is_ok());
    }
}

#[test]
fn test_store_and_load_compiled_expression() {
    let env = CelEnvironment::new();

    // Compile expression
    let expression = "cayenne_lpp_decode(input)";
    let compiled = env.compile_to_proto(expression).unwrap();

    // Simulate storing to database/config (just clone the bytes)
    let stored_bytes = compiled.clone();

    // Simulate loading from storage and executing
    let payload = vec![0x01, 0x67, 0x01, 0x10];
    let result = env.execute_compiled(&stored_bytes, &payload).unwrap();

    assert_eq!(result["temperature_1"], 27.2);
}

#[test]
fn test_compiled_expression_with_complex_payload() {
    let env = CelEnvironment::new();

    // Compile complex expression
    let expression = r#"
        {
            'decoded': cayenne_lpp_decode(input),
            'device_id': 'sensor-001',
            'has_temp': has(cayenne_lpp_decode(input).temperature_0)
        }
    "#;
    let compiled = env.compile_to_proto(expression).unwrap();

    // Execute with multi-sensor payload
    let payload = vec![
        0x00, 0x67, 0x00, 0xFA, // Temperature: 25.0°C
        0x01, 0x68, 0x78,       // Humidity: 60.0%
    ];

    let result = env.execute_compiled(&compiled, &payload).unwrap();

    assert!(result["has_temp"].as_bool().unwrap());
    assert_eq!(result["device_id"], "sensor-001");
    assert_eq!(result["decoded"]["temperature_0"], 25.0);
    assert_eq!(result["decoded"]["humidity_1"], 60.0);
}
```

#### 2. Module Documentation
**File**: [crates/ponix-payload/src/cel.rs](crates/ponix-payload/src/cel.rs)
**Changes**: Add comprehensive module-level documentation

```rust
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
//! # Quick Start
//!
//! ## String Expression Execution
//!
//! ```rust
//! use ponix_payload::cel::CelEnvironment;
//! use serde_json::json;
//!
//! // Create CEL environment with Cayenne LPP decoder
//! let env = CelEnvironment::new();
//!
//! // Binary payload: Temperature sensor reading 27.2°C
//! let payload = vec![0x01, 0x67, 0x01, 0x10];
//!
//! // Execute CEL expression
//! let result = env.execute("cayenne_lpp_decode(input)", &payload).unwrap();
//!
//! assert_eq!(result, json!({"temperature_1": 27.2}));
//! ```
//!
//! ## Precompiled Expression Execution (Faster)
//!
//! ```rust
//! use ponix_payload::cel::CelEnvironment;
//!
//! let env = CelEnvironment::new();
//!
//! // Compile once, store the result
//! let expression = "cayenne_lpp_decode(input)";
//! let compiled = env.compile_to_proto(expression).unwrap();
//! // Store compiled_proto in database/config...
//!
//! // Execute many times with precompiled version (much faster)
//! let payload = vec![0x01, 0x67, 0x01, 0x10];
//! let result = env.execute_compiled(&compiled, &payload).unwrap();
//! ```
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
//!
//! All errors are returned as [`crate::PayloadError`] variants:
//!
//! ```rust
//! use ponix_payload::cel::CelEnvironment;
//! use ponix_payload::PayloadError;
//!
//! let env = CelEnvironment::new()
//!     .register_cayenne_lpp_decoder()
//!     .build()
//!     .unwrap();
//!
//! // Invalid expression syntax
//! let result = env.execute("invalid syntax!", &[]);
//! assert!(matches!(result, Err(PayloadError::CelCompilationError(_))));
//!
//! // Decoder error (invalid payload)
//! let result = env.execute("cayenne_lpp_decode(input)", &[0x01]);
//! assert!(matches!(result, Err(PayloadError::CelExecutionError(_))));
//! ```
//!
//! # Extensibility
//!
//! The builder pattern allows registration of additional decoder functions:
//!
//! ```rust,ignore
//! let env = CelEnvironment::new()
//!     .register_cayenne_lpp_decoder()
//!     .register_custom_decoder("my_format", decoder_fn)
//!     .build()
//!     .unwrap();
//! ```
//!
//! Future decoder functions can be added following the same pattern.

// ... (existing code continues)
```

#### 3. README Example
**File**: `crates/ponix-payload/README.md` (new file)
**Changes**: Create crate-level README with CEL examples

```markdown
# ponix-payload

Payload decoding library for Ponix IoT platform, supporting Cayenne LPP and configurable CEL expressions.

## Features

- **Cayenne LPP Decoder**: Supports 27 sensor types with JSON output
- **CEL Expressions**: Runtime-configurable payload transformations
- **Type-Safe**: Strongly typed Rust API with comprehensive error handling
- **Extensible**: Builder pattern for registering custom decoder functions

## Quick Start

### Basic Cayenne LPP Decoding

```rust
use ponix_payload::{CayenneLppDecoder, PayloadDecoder};

let decoder = CayenneLppDecoder::new();
let payload = vec![0x01, 0x67, 0x01, 0x10]; // Temperature: 27.2°C

let result = decoder.decode(&payload)?;
println!("{}", result); // {"temperature_1": 27.2}
```

### CEL Expression-Based Decoding

```rust
use ponix_payload::cel::{CelEnvironment};

// Create environment with registered functions
let env = CelEnvironment::new()
    .register_cayenne_lpp_decoder()
    .build()?;

// Execute configurable expression
let expression = "cayenne_lpp_decode(input)";
let payload = vec![0x01, 0x67, 0x01, 0x10];

let result = env.execute(expression, &payload)?;
// {"temperature_1": 27.2}
```

### Advanced CEL Expressions

```rust
// Conditional logic
let expr = r#"
    cayenne_lpp_decode(input).temperature_1 > 30.0
    ? {'status': 'alert', 'value': cayenne_lpp_decode(input).temperature_1}
    : {'status': 'normal', 'value': cayenne_lpp_decode(input).temperature_1}
"#;

// Data enrichment
let expr = r#"
    {
        'device_id': 'sensor-001',
        'readings': cayenne_lpp_decode(input),
        'timestamp': timestamp_now()
    }
"#;
```

## Supported Sensor Types

Cayenne LPP supports 27 sensor types including:

- Temperature, Humidity, Barometer
- GPS, Accelerometer, Gyrometer
- Voltage, Current, Power, Energy
- Digital/Analog I/O
- And more...

See [cayenne_lpp module documentation](https://docs.rs/ponix-payload/latest/ponix_payload/cayenne_lpp/) for complete list.

## CEL Functions

### cayenne_lpp_decode(input)

Decodes Cayenne LPP binary payloads to JSON.

**Input**: Binary data (bytes)
**Output**: JSON object

```cel
cayenne_lpp_decode(input)
```

## License

MIT
```

### Success Criteria

#### Automated Verification
- [ ] All integration tests pass: `cargo test -p ponix-payload --test cel_integration_tests`
- [ ] All unit tests pass: `cargo test -p ponix-payload`
- [ ] Documentation builds: `cargo doc -p ponix-payload --open`
- [ ] Examples in docs are valid: `cargo test -p ponix-payload --doc`
- [ ] README examples compile: `cargo test -p ponix-payload --features _readme_examples` (if configured)

#### Manual Verification
- [ ] Documentation is clear and comprehensive
- [ ] Examples cover common use cases
- [ ] API feels intuitive for new users
- [ ] Error messages guide users to solutions

**Implementation Note**: After completing this phase, the feature is complete for standalone use. Future work would integrate this into the NATS pipeline.

---

## Testing Strategy

### Unit Tests
- CEL value type conversions (JSON ↔ CEL)
- Error handling at each layer (compilation, execution, validation)
- Edge cases: empty payloads, null values, nested structures
- Function registration and invocation
- Builder pattern behavior

### Integration Tests
- Real-world sensor payloads (environmental, GPS, multi-sensor)
- Complex CEL expressions (conditionals, transformations, field access)
- Multiple decoder calls in single expression
- Runtime expression configuration
- Error propagation through expression evaluation

### Manual Testing Steps
1. Build the project: `cargo build -p ponix-payload`
2. Run all tests: `cargo test -p ponix-payload`
3. Try expressions in documentation examples
4. Test with actual Cayenne LPP hardware payloads if available
5. Verify error messages are helpful for debugging
6. Confirm JSON output is valid (can be parsed by external tools)
7. Check performance with large payloads (100+ sensors)

## Performance Considerations

- **Expression Compilation**: CEL expressions are compiled once using `compile_to_proto()`, which performs parsing and type-checking. Store the resulting `CheckedExpr` protobuf bytes for reuse.
- **CheckedExpr to Program**: Converting `CheckedExpr` back to `Program` (AST) is much faster than re-compilation because it skips parsing and type-checking
- **Clone vs Reference**: `CelEnvironment` uses `Arc<Context>` for cheap cloning
- **Memory Allocation**: Each execution creates new context; for high-throughput use cases, consider context pooling
- **Decoder Overhead**: `cayenne_lpp_decode` calls are stateless; multiple calls in one expression will re-decode
- **JSON Conversion**: Two conversions occur (decoder → JSON → CEL → JSON); optimize if hot path

**Performance Best Practices**:
1. Compile expressions once with `compile_to_proto()` and store the bytes
2. Reuse compiled expressions with `execute_compiled()` for repeated evaluation
3. Avoid calling `execute()` repeatedly with the same expression string

**Optimization Opportunities** (future work - OUT OF SCOPE for this issue):
- In-memory caching of compiled Programs (caller's responsibility)
- Memoize decoder results within expression scope
- Use zero-copy for binary data passing
- Implement streaming for large payloads

## Migration Notes

No migration required - this is a new feature addition to ponix-payload crate.

### Adding to Existing Code

If you have existing code using `CayenneLppDecoder` directly:

```rust
// Before (still works)
let decoder = CayenneLppDecoder::new();
let result = decoder.decode(bytes)?;

// After (optional CEL layer)
let env = CelEnvironment::new()
    .register_cayenne_lpp_decoder()
    .build()?;
let result = env.execute("cayenne_lpp_decode(input)", bytes)?;
```

Both approaches coexist without breaking changes.

## Future Extensibility

### Adding New Decoder Functions

To add additional decoder functions in the future, modify the `CelEnvironment::new()` method:

```rust
impl CelEnvironment {
    pub fn new() -> Self {
        let mut context = Context::default();

        // Register cayenne_lpp_decode function
        let cayenne_decoder = CayenneLppDecoder::new();
        context.add_function(
            "cayenne_lpp_decode",
            move |bytes: Vec<u8>| -> std::result::Result<CelValue, String> {
                cayenne_decoder.decode(&bytes)
                    .map_err(|e| e.to_string())
                    .and_then(|json_value| {
                        json_to_cel_value(json_value)
                            .map_err(|e| e.to_string())
                    })
            }
        );

        // Future: Add more decoder functions here
        // let my_decoder = MyDecoder::new();
        // context.add_function("my_decode", move |bytes: Vec<u8>| { ... });

        Self {
            context: Arc::new(context),
        }
    }
}
```

Alternatively, if flexible initialization is needed later, add a method to register additional functions after creation (though this would require making Context mutable).

### Planned Functions (Not in Scope)
- `base64_decode(string) -> bytes` - Base64 decoding
- `hex_decode(string) -> bytes` - Hex string to bytes
- `modbus_decode(bytes) -> json` - Modbus protocol
- `lorawan_decode(bytes, port) -> json` - LoRaWAN payloads

## References

- Original issue: [ponix-dev/ponix-rs#23](https://github.com/ponix-dev/ponix-rs/issues/23)
- Cayenne LPP spec: [ElectronicCats/CayenneLPP](https://github.com/ElectronicCats/CayenneLPP)
- CEL specification: [google/cel-spec](https://github.com/google/cel-spec)
- cel-interpreter crate: [cel-rust/cel-rust](https://github.com/cel-rust/cel-rust)
- Dependency PR: [ponix-dev/ponix-rs#22](https://github.com/ponix-dev/ponix-rs/issues/22)

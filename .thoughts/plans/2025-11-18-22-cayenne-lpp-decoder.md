# Cayenne LPP Binary to JSON Decoder Implementation Plan

## Overview

Create a new `ponix-payload` crate to handle payload decoding for various IoT formats, starting with Cayenne LPP (Low Power Payload) support. This crate will provide a trait-based architecture for extensibility while implementing the complete Cayenne LPP specification based on the reference implementation from ChirpStack Application Server.

## Current State Analysis

**Existing Architecture:**
- The ponix-rs codebase uses a Cargo workspace with 7 crates following DDD principles
- Data flows: Producer → NATS JetStream → Consumer → ClickHouse
- Protobuf messages are decoded using `ponix-nats/src/protobuf.rs:78-139` pattern
- Generic `BatchProcessor` pattern allows custom message processing ([ponix-nats/src/consumer.rs:46-47](crates/ponix-nats/src/consumer.rs#L46-L47))
- No existing payload decoding infrastructure beyond protobuf

**What's Missing:**
- No support for binary payload formats like Cayenne LPP
- No trait-based decoder abstraction for multiple payload formats
- No conversion from raw binary sensor data to JSON for ClickHouse storage

**Key Constraints:**
- Must integrate with existing NATS consumer pipeline
- Should follow workspace patterns (minimal dependencies, trait-based design)
- Must produce `serde_json::Value` for ClickHouse JSON column storage
- Should use big-endian byte order for Cayenne LPP multi-byte values

## Desired End State

A production-ready `ponix-payload` crate that:
- Provides a `PayloadDecoder` trait for extensible payload format support
- Implements complete Cayenne LPP specification (12 sensor types)
- Converts binary payloads to JSON with correct scaling and data types
- Has comprehensive test coverage with known test vectors
- Is ready for integration into NATS consumer pipeline (future phase)

### Verification:
- All unit tests pass: `cargo test -p ponix-payload`
- Decoder handles all 12 Cayenne LPP sensor types correctly
- Error handling validates insufficient data and edge cases
- Documentation includes usage examples and sensor type reference

## What We're NOT Doing

- NATS pipeline integration (deferred to future implementation)
- Encoding (JSON to binary) - decoder only for now
- Support for other payload formats beyond Cayenne LPP in this phase
- Payload validation beyond format correctness (e.g., physical range checks)
- Performance optimizations or zero-copy parsing

## Implementation Approach

Following the existing codebase patterns:
1. Create new crate with workspace dependency management
2. Define trait-based decoder interface for extensibility
3. Implement Cayenne LPP decoder with all sensor types
4. Build comprehensive test suite with reference test vectors
5. Add documentation with usage examples

The Cayenne LPP decoder will handle the binary format:
- **Structure**: `[Channel:1][Type:1][Data:variable]` triplets
- **12 Sensor Types**: Digital I/O, Analog I/O, Temperature, Humidity, Accelerometer, Barometer, Gyrometer, GPS, Illuminance, Presence
- **Big-Endian Encoding**: All multi-byte values use network byte order
- **Channel Multiplexing**: 0-255 channels per sensor type
- **JSON Output**: Nested structure `{typeID: {channelID: value}}`

## Phase 1: Crate Setup and Core Architecture

### Overview
Establish the `ponix-payload` crate structure, define the core `PayloadDecoder` trait, and set up error handling infrastructure.

### Changes Required:

#### 1. Workspace Configuration
**File**: `Cargo.toml`
**Changes**: Add ponix-payload to workspace members

```toml
[workspace]
members = [
    "crates/runner",
    "crates/ponix-domain",
    "crates/ponix-nats",
    "crates/ponix-clickhouse",
    "crates/ponix-postgres",
    "crates/ponix-grpc",
    "crates/ponix-all-in-one",
    "crates/ponix-payload",  # Add this line
]
```

#### 2. Crate Manifest
**File**: `crates/ponix-payload/Cargo.toml`
**Changes**: Create new crate with minimal dependencies

```toml
[package]
name = "ponix-payload"
version = "0.1.0"
edition = "2021"

[dependencies]
serde_json = { workspace = true }
thiserror = { workspace = true }
```

#### 3. Error Types
**File**: `crates/ponix-payload/src/error.rs`
**Changes**: Define payload decoding errors

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("insufficient data: expected at least {expected} bytes, got {actual}")]
    InsufficientData { expected: usize, actual: usize },

    #[error("unsupported sensor type: {0}")]
    UnsupportedType(u8),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("json serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, PayloadError>;
```

#### 4. Decoder Trait
**File**: `crates/ponix-payload/src/lib.rs`
**Changes**: Define core abstraction and module structure

```rust
mod error;
pub mod cayenne_lpp;

pub use error::{PayloadError, Result};

/// Trait for decoding binary payload formats to JSON
pub trait PayloadDecoder {
    /// Decode binary payload to JSON value
    fn decode(&self, bytes: &[u8]) -> Result<serde_json::Value>;
}
```

### Success Criteria:

#### Automated Verification:
- [x] Workspace builds successfully: `cargo build`
- [x] New crate compiles: `cargo build -p ponix-payload`
- [x] Type checking passes: `cargo check -p ponix-payload`
- [x] No linting errors: `cargo clippy -p ponix-payload`

#### Manual Verification:
- [ ] Crate structure follows workspace conventions
- [ ] Error types cover all expected failure modes
- [ ] Trait signature supports future payload formats

---

## Phase 2: Cayenne LPP Decoder Implementation

### Overview
Implement the complete Cayenne LPP specification with all 12 sensor types, proper scaling, and error handling.

### Changes Required:

#### 1. Cayenne LPP Constants
**File**: `crates/ponix-payload/src/cayenne_lpp/mod.rs`
**Changes**: Define sensor type IDs and data sizes

```rust
mod decoder;
mod types;

pub use decoder::CayenneLppDecoder;

// Sensor type IDs from Cayenne LPP specification
pub const TYPE_DIGITAL_INPUT: u8 = 0;
pub const TYPE_DIGITAL_OUTPUT: u8 = 1;
pub const TYPE_ANALOG_INPUT: u8 = 2;
pub const TYPE_ANALOG_OUTPUT: u8 = 3;
pub const TYPE_ILLUMINANCE: u8 = 101;
pub const TYPE_PRESENCE: u8 = 102;
pub const TYPE_TEMPERATURE: u8 = 103;
pub const TYPE_HUMIDITY: u8 = 104;
pub const TYPE_ACCELEROMETER: u8 = 113;
pub const TYPE_BAROMETER: u8 = 115;
pub const TYPE_GYROMETER: u8 = 134;
pub const TYPE_GPS: u8 = 136;

// Data sizes for each type (in bytes, excluding channel and type bytes)
pub const SIZE_DIGITAL: usize = 1;
pub const SIZE_ANALOG: usize = 2;
pub const SIZE_ILLUMINANCE: usize = 2;
pub const SIZE_PRESENCE: usize = 1;
pub const SIZE_TEMPERATURE: usize = 2;
pub const SIZE_HUMIDITY: usize = 1;
pub const SIZE_ACCELEROMETER: usize = 6;
pub const SIZE_BAROMETER: usize = 2;
pub const SIZE_GYROMETER: usize = 6;
pub const SIZE_GPS: usize = 9;
```

#### 2. Value Types
**File**: `crates/ponix-payload/src/cayenne_lpp/types.rs`
**Changes**: Define sensor value types

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccelerometerValue {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GyrometerValue {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpsValue {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
}
```

#### 3. Core Decoder Implementation
**File**: `crates/ponix-payload/src/cayenne_lpp/decoder.rs`
**Changes**: Implement CayenneLppDecoder with all sensor types

```rust
use super::types::{AccelerometerValue, GpsValue, GyrometerValue};
use super::*;
use crate::{PayloadDecoder, PayloadError, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub struct CayenneLppDecoder;

impl CayenneLppDecoder {
    pub fn new() -> Self {
        Self
    }

    fn get_data_size(type_id: u8) -> Result<usize> {
        match type_id {
            TYPE_DIGITAL_INPUT | TYPE_DIGITAL_OUTPUT => Ok(SIZE_DIGITAL),
            TYPE_ANALOG_INPUT | TYPE_ANALOG_OUTPUT => Ok(SIZE_ANALOG),
            TYPE_ILLUMINANCE => Ok(SIZE_ILLUMINANCE),
            TYPE_PRESENCE => Ok(SIZE_PRESENCE),
            TYPE_TEMPERATURE => Ok(SIZE_TEMPERATURE),
            TYPE_HUMIDITY => Ok(SIZE_HUMIDITY),
            TYPE_ACCELEROMETER => Ok(SIZE_ACCELEROMETER),
            TYPE_BAROMETER => Ok(SIZE_BAROMETER),
            TYPE_GYROMETER => Ok(SIZE_GYROMETER),
            TYPE_GPS => Ok(SIZE_GPS),
            _ => Err(PayloadError::UnsupportedType(type_id)),
        }
    }

    fn read_i16_be(data: &[u8]) -> i16 {
        i16::from_be_bytes([data[0], data[1]])
    }

    fn read_u16_be(data: &[u8]) -> u16 {
        u16::from_be_bytes([data[0], data[1]])
    }

    fn read_i24_be(data: &[u8]) -> i32 {
        // Sign-extend 24-bit to 32-bit
        let value = (i32::from(data[0]) << 24)
            | (i32::from(data[1]) << 16)
            | (i32::from(data[2]) << 8);
        value >> 8
    }

    fn decode_sensor_value(&self, type_id: u8, data: &[u8]) -> Result<Value> {
        let value = match type_id {
            TYPE_DIGITAL_INPUT | TYPE_DIGITAL_OUTPUT => {
                json!(data[0])
            }
            TYPE_ANALOG_INPUT | TYPE_ANALOG_OUTPUT => {
                let raw = Self::read_i16_be(data);
                json!(raw as f64 / 100.0)
            }
            TYPE_ILLUMINANCE => {
                let raw = Self::read_u16_be(data);
                json!(raw)
            }
            TYPE_PRESENCE => {
                json!(data[0])
            }
            TYPE_TEMPERATURE => {
                let raw = Self::read_i16_be(data);
                json!(raw as f64 / 10.0)
            }
            TYPE_HUMIDITY => {
                json!(data[0] as f64 / 2.0)
            }
            TYPE_ACCELEROMETER => {
                let x = Self::read_i16_be(&data[0..2]) as f64 / 1000.0;
                let y = Self::read_i16_be(&data[2..4]) as f64 / 1000.0;
                let z = Self::read_i16_be(&data[4..6]) as f64 / 1000.0;
                json!(AccelerometerValue { x, y, z })
            }
            TYPE_BAROMETER => {
                let raw = Self::read_u16_be(data);
                json!(raw as f64 / 10.0)
            }
            TYPE_GYROMETER => {
                let x = Self::read_i16_be(&data[0..2]) as f64 / 100.0;
                let y = Self::read_i16_be(&data[2..4]) as f64 / 100.0;
                let z = Self::read_i16_be(&data[4..6]) as f64 / 100.0;
                json!(GyrometerValue { x, y, z })
            }
            TYPE_GPS => {
                let lat = Self::read_i24_be(&data[0..3]) as f64 / 10000.0;
                let lon = Self::read_i24_be(&data[3..6]) as f64 / 10000.0;
                let alt = Self::read_i24_be(&data[6..9]) as f64 / 100.0;
                json!(GpsValue {
                    latitude: lat,
                    longitude: lon,
                    altitude: alt
                })
            }
            _ => return Err(PayloadError::UnsupportedType(type_id)),
        };
        Ok(value)
    }
}

impl Default for CayenneLppDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl PayloadDecoder for CayenneLppDecoder {
    fn decode(&self, bytes: &[u8]) -> Result<Value> {
        // Result structure: {typeID: {channelID: value}}
        let mut result: HashMap<String, Map<String, Value>> = HashMap::new();
        let mut offset = 0;

        while offset < bytes.len() {
            // Need at least 2 bytes for channel + type
            if offset + 2 > bytes.len() {
                return Err(PayloadError::InsufficientData {
                    expected: 2,
                    actual: bytes.len() - offset,
                });
            }

            let channel = bytes[offset];
            let type_id = bytes[offset + 1];
            offset += 2;

            let data_size = Self::get_data_size(type_id)?;

            if offset + data_size > bytes.len() {
                return Err(PayloadError::InsufficientData {
                    expected: data_size,
                    actual: bytes.len() - offset,
                });
            }

            let data = &bytes[offset..offset + data_size];
            let value = self.decode_sensor_value(type_id, data)?;
            offset += data_size;

            // Store value in nested map structure
            result
                .entry(type_id.to_string())
                .or_insert_with(Map::new)
                .insert(channel.to_string(), value);
        }

        // Convert HashMap to JSON object
        let mut json_result = Map::new();
        for (type_id, channels) in result {
            json_result.insert(type_id, Value::Object(channels));
        }

        Ok(Value::Object(json_result))
    }
}
```

### Success Criteria:

#### Automated Verification:
- [x] Crate builds successfully: `cargo build -p ponix-payload`
- [x] No type errors: `cargo check -p ponix-payload`
- [x] No linting warnings: `cargo clippy -p ponix-payload`
- [x] All sensor types compile correctly

#### Manual Verification:
- [ ] Decoder implements all 12 Cayenne LPP sensor types
- [ ] Big-endian byte order handled correctly
- [ ] Scaling factors match specification
- [ ] 24-bit GPS sign extension implemented correctly

---

## Phase 3: Testing Infrastructure

### Overview
Create comprehensive test suite with test vectors covering all sensor types, edge cases, and error conditions.

### Changes Required:

#### 1. Test Module Setup
**File**: `crates/ponix-payload/src/cayenne_lpp/decoder.rs`
**Changes**: Add test module at end of file

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::PayloadDecoder;

    #[test]
    fn test_empty_payload() {
        let decoder = CayenneLppDecoder::new();
        let result = decoder.decode(&[]).unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn test_digital_input() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Digital Input (type 0), Value 100
        let payload = vec![0x03, 0x00, 0x64];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"0": {"3": 100}}));
    }

    #[test]
    fn test_digital_output() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Digital Output (type 1), Value 255
        let payload = vec![0x05, 0x01, 0xFF];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"1": {"5": 255}}));
    }

    #[test]
    fn test_analog_input() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Analog Input (type 2), Value 0.1 (raw: 10)
        let payload = vec![0x03, 0x02, 0x00, 0x0A];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"2": {"3": 0.1}}));
    }

    #[test]
    fn test_analog_output() {
        let decoder = CayenneLppDecoder::new();
        // Channel 7, Analog Output (type 3), Value -1.5 (raw: -150)
        let payload = vec![0x07, 0x03, 0xFF, 0x6A];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"3": {"7": -1.5}}));
    }

    #[test]
    fn test_temperature() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Temperature (type 103), Value 27.2°C (raw: 272)
        let payload = vec![0x03, 0x67, 0x01, 0x10];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"103": {"3": 27.2}}));
    }

    #[test]
    fn test_temperature_negative() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Temperature (type 103), Value -0.1°C (raw: -1)
        let payload = vec![0x05, 0x67, 0xFF, 0xFF];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"103": {"5": -0.1}}));
    }

    #[test]
    fn test_humidity() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Humidity (type 104), Value 50% (raw: 100)
        let payload = vec![0x05, 0x68, 0x64];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"104": {"5": 50.0}}));
    }

    #[test]
    fn test_humidity_full() {
        let decoder = CayenneLppDecoder::new();
        // Channel 1, Humidity (type 104), Value 100% (raw: 200)
        let payload = vec![0x01, 0x68, 0xC8];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"104": {"1": 100.0}}));
    }

    #[test]
    fn test_illuminance() {
        let decoder = CayenneLppDecoder::new();
        // Channel 2, Illuminance (type 101), Value 1000 lux
        let payload = vec![0x02, 0x65, 0x03, 0xE8];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"101": {"2": 1000}}));
    }

    #[test]
    fn test_presence() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Presence (type 102), Value 1 (detected)
        let payload = vec![0x05, 0x66, 0x01];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"102": {"5": 1}}));
    }

    #[test]
    fn test_accelerometer() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Accelerometer (type 113), X=0.001, Y=0.002, Z=0.003
        let payload = vec![0x03, 0x71, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"113": {"3": {"x": 0.001, "y": 0.002, "z": 0.003}}})
        );
    }

    #[test]
    fn test_barometer() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Barometer (type 115), Value 1013.2 hPa (raw: 10132)
        let payload = vec![0x03, 0x73, 0x27, 0x94];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"115": {"3": 1013.2}}));
    }

    #[test]
    fn test_gyrometer() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Gyrometer (type 134), X=1.0, Y=2.0, Z=3.0
        let payload = vec![0x03, 0x86, 0x00, 0x64, 0x00, 0xC8, 0x01, 0x2C];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"134": {"3": {"x": 1.0, "y": 2.0, "z": 3.0}}})
        );
    }

    #[test]
    fn test_gps() {
        let decoder = CayenneLppDecoder::new();
        // Channel 1, GPS (type 136)
        // Lat: 42.3519° (raw: 423519), Lon: -87.9094° (raw: -879094), Alt: 10m (raw: 1000)
        let payload = vec![0x01, 0x88, 0x06, 0x76, 0x5F, 0xF2, 0x96, 0x0A, 0x00, 0x03, 0xE8];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"136": {"1": {
                "latitude": 42.3519,
                "longitude": -87.9094,
                "altitude": 10.0
            }}})
        );
    }

    #[test]
    fn test_multiple_sensors() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3 Temperature 27.2°C + Channel 5 Humidity 50%
        let payload = vec![0x03, 0x67, 0x01, 0x10, 0x05, 0x68, 0x64];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({
                "103": {"3": 27.2},
                "104": {"5": 50.0}
            })
        );
    }

    #[test]
    fn test_multiple_channels_same_type() {
        let decoder = CayenneLppDecoder::new();
        // Two temperature sensors: Channel 3: 27.2°C, Channel 5: -0.1°C
        let payload = vec![0x03, 0x67, 0x01, 0x10, 0x05, 0x67, 0xFF, 0xFF];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({
                "103": {
                    "3": 27.2,
                    "5": -0.1
                }
            })
        );
    }

    #[test]
    fn test_insufficient_data_for_header() {
        let decoder = CayenneLppDecoder::new();
        // Only 1 byte (need 2 for channel + type)
        let payload = vec![0x03];
        let result = decoder.decode(&payload);
        assert!(matches!(result, Err(PayloadError::InsufficientData { .. })));
    }

    #[test]
    fn test_insufficient_data_for_sensor() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Temperature (type 103) but only 1 byte of data (need 2)
        let payload = vec![0x03, 0x67, 0x01];
        let result = decoder.decode(&payload);
        assert!(matches!(result, Err(PayloadError::InsufficientData { .. })));
    }

    #[test]
    fn test_unsupported_type() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Invalid type 99
        let payload = vec![0x03, 0x63, 0x01, 0x02];
        let result = decoder.decode(&payload);
        assert!(matches!(result, Err(PayloadError::UnsupportedType(99))));
    }

    #[test]
    fn test_complex_multi_sensor() {
        let decoder = CayenneLppDecoder::new();
        // Environmental sensor suite:
        // Channel 0 Temp 23.0°C + Channel 1 Humidity 100% +
        // Channel 2 Illuminance 1000 lux + Channel 3 Barometer 1018.0 hPa
        let payload = vec![
            0x00, 0x67, 0x00, 0xE6, // Temp
            0x01, 0x68, 0xC8, // Humidity
            0x02, 0x65, 0x03, 0xE8, // Illuminance
            0x03, 0x73, 0x27, 0xC4, // Barometer
        ];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({
                "103": {"0": 23.0},
                "104": {"1": 100.0},
                "101": {"2": 1000},
                "115": {"3": 1018.0}
            })
        );
    }
}
```

#### 2. Integration Test
**File**: `crates/ponix-payload/tests/cayenne_lpp_integration.rs`
**Changes**: Add integration test file

```rust
use ponix_payload::{cayenne_lpp::CayenneLppDecoder, PayloadDecoder};
use serde_json::json;

#[test]
fn test_real_world_sensor_packet() {
    // Simulated real-world packet from multi-sensor device
    let decoder = CayenneLppDecoder::new();

    // Temperature (27.2°C) + Humidity (65%) + GPS + Accelerometer
    let payload = vec![
        0x03, 0x67, 0x01, 0x10, // Temperature
        0x05, 0x68, 0x82, // Humidity
        0x01, 0x88, 0x06, 0x76, 0x5F, 0xF2, 0x96, 0x0A, 0x00, 0x03, 0xE8, // GPS
        0x06, 0x71, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, // Accelerometer
    ];

    let result = decoder.decode(&payload).unwrap();

    // Verify all sensor types decoded correctly
    assert!(result["103"]["3"].is_number());
    assert!(result["104"]["5"].is_number());
    assert!(result["136"]["1"].is_object());
    assert!(result["113"]["6"].is_object());
}

#[test]
fn test_decoder_trait_usage() {
    // Test that decoder can be used through trait object
    let decoder: Box<dyn PayloadDecoder> = Box::new(CayenneLppDecoder::new());
    let payload = vec![0x03, 0x67, 0x01, 0x10];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"103": {"3": 27.2}}));
}
```

### Success Criteria:

#### Automated Verification:
- [x] All unit tests pass: `cargo test -p ponix-payload`
- [x] Integration tests pass: `cargo test -p ponix-payload --test cayenne_lpp_integration`
- [x] No test warnings: `cargo test -p ponix-payload -- --nocapture`
- [x] Code coverage includes all sensor types and error paths

#### Manual Verification:
- [ ] Test vectors match Cayenne LPP specification
- [ ] Error cases (insufficient data, unsupported types) handled correctly
- [ ] Multi-sensor payloads decode properly
- [ ] Negative values (temperature, analog) decode correctly
- [ ] 24-bit GPS values with sign extension work correctly

**Implementation Note**: After completing this phase and all automated verification passes, the decoder is ready for use. Integration with NATS consumer pipeline can be done in a future phase.

---

## Testing Strategy

### Unit Tests:
- Each sensor type tested individually with known test vectors
- Negative values tested for signed types (temperature, analog, GPS)
- Edge cases: empty payload, maximum values, minimum values
- Error handling: insufficient data at various points, unsupported types
- Multi-sensor payloads with same and different types

### Integration Tests:
- Real-world sensor packet simulation
- Trait object usage (Box<dyn PayloadDecoder>)
- Round-trip validation (if encoder added later)

### Manual Testing Steps:
1. Create test payloads using online Cayenne LPP encoder tools
2. Verify JSON output matches expected sensor readings
3. Test with actual LoRaWAN device payloads (if available)
4. Verify big-endian byte order with hex dump inspection
5. Test decoder performance with large multi-sensor payloads

## Performance Considerations

- Big-endian byte reads use standard library's `from_be_bytes()` (optimized)
- HashMap used for result accumulation (O(1) insert, reasonable for typical payload sizes)
- No heap allocations during sensor value parsing (stack-based)
- Single-pass algorithm with O(n) complexity where n = payload size
- Zero-copy parsing deferred to future optimization phase

## References

- Original issue: [ponix-dev/ponix-rs#22](https://github.com/ponix-dev/ponix-rs/issues/22)
- Reference implementation: [ChirpStack Cayenne LPP](https://github.com/brocaar/chirpstack-application-server/blob/master/internal/codec/cayennelpp/cayennelpp.go)
- Cayenne LPP Specification: Based on ChirpStack reference implementation
- Workspace patterns: [ponix-rs CLAUDE.md](CLAUDE.md)
- NATS consumer pattern: [ponix-nats/src/consumer.rs:49-222](crates/ponix-nats/src/consumer.rs#L49-L222)

# Extended Cayenne LPP Sensor Types Implementation Plan

## Overview

Extend the Cayenne LPP decoder in the `ponix-payload` crate to support 15 additional sensor types from the ElectronicCats extended Cayenne LPP format, bringing total type support from 12 to 27 sensor types.

## Current State Analysis

### Existing Implementation (PR #24)

The `ponix-payload` crate currently implements the standard 12 Cayenne LPP sensor types:

**Location**: `crates/ponix-payload/src/cayenne_lpp.rs` (523 lines)

**Implemented Types**:
- Digital I/O (0, 1) - 1 byte each
- Analog I/O (2, 3) - 2 bytes, 0.01 scaling
- Illuminance (101) - 2 bytes unsigned
- Presence (102) - 1 byte boolean
- Temperature (103) - 2 bytes, 0.1°C signed
- Humidity (104) - 1 byte, 0.5% unsigned
- Accelerometer (113) - 6 bytes, 0.001g signed per axis
- Barometer (115) - 2 bytes, 0.1 hPa unsigned
- Gyrometer (134) - 6 bytes, 0.01°/s signed per axis
- GPS (136) - 9 bytes, complex 24-bit encoding

**Architecture**:
- `PayloadDecoder` trait for extensibility
- Constants for type IDs and data sizes
- Helper methods: `read_i16_be()`, `read_u16_be()`, `read_i24_be()`
- Single-pass O(n) decoder with early validation
- Comprehensive test coverage (28 tests)

**Key Design Patterns**:
- Type-specific scaling in `decode_sensor_value()`
- Big-endian byte order for multi-byte values
- JSON output with `{type_name}_{channel}` field naming
- Complex types use dedicated structs (AccelerometerValue, GyrometerValue, GpsValue)

### Key Discoveries

1. **No existing Voltage/Current types** - Confirmed via grep, these are new additions
2. **Consistent scaling pattern** - All types use division for scaling (e.g., `raw / 10.0`, `raw / 100.0`)
3. **Test coverage standard** - 2-3 tests per type (basic value, edge cases, negative values where applicable)
4. **Big-endian helpers** - Existing helper methods cover i16/u16/i24, need to add u32 support
5. **Struct-based complex types** - GPS, Accelerometer, and Gyrometer use dedicated structs with serde support

## Desired End State

After implementation, the Cayenne LPP decoder will:
- Support all 27 sensor types (12 standard + 15 extended)
- Handle variable-length Polyline type with coordinate compression
- Represent RGB colors as hex strings: `"#FF8000"`
- Represent Unix timestamps as ISO 8601 strings: `"2023-11-13T20:00:00Z"`
- Maintain comprehensive test coverage (60+ tests total)
- Follow existing architecture patterns and code style

### Verification Criteria

The implementation is complete when:
1. All 15 extended types decode correctly with proper scaling
2. Polyline type handles variable-length payloads with delta decoding
3. All automated tests pass: `cargo test -p ponix-payload`
4. Integration tests with multi-sensor payloads work correctly
5. Code follows existing patterns and style conventions

## What We're NOT Doing

- Not modifying the `PayloadDecoder` trait interface
- Not changing the JSON output format structure (still `{type_name}_{channel}`)
- Not adding support for encoding/writing Cayenne LPP payloads (decode-only)
- Not implementing simplification algorithms for Polyline (decode existing data only)
- Not adding support for non-Cayenne LPP payload formats in this phase
- Not modifying the existing 12 standard types or their tests

## Implementation Approach

Follow the established pattern from PR #24:
1. Add type ID and size constants
2. Add helper methods for new byte widths (u32, i32)
3. Create dedicated structs for complex types (Colour, PolylineValue)
4. Extend `get_type_name()` and `get_data_size()` functions
5. Add decoding logic to `decode_sensor_value()` with proper scaling
6. Write comprehensive unit tests for each type
7. Add integration tests for multi-sensor payloads

**Special Considerations**:
- Polyline (240) requires variable-length handling with special size validation
- Unix Time (133) needs conversion to ISO 8601 format using chrono crate
- Colour (135) needs RGB to hex string conversion (`#RRGGBB` format)

## Phase 1: Basic Extended Types (Simple Scalars)

### Overview
Implement 9 straightforward extended types that follow existing patterns with simple scalar values and fixed sizes.

### Changes Required

#### 1. Type Constants (`cayenne_lpp.rs:18-29`)

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add new type ID constants after line 17:

```rust
// Extended sensor type IDs
pub const TYPE_GENERIC_SENSOR: u8 = 100;
pub const TYPE_VOLTAGE: u8 = 116;
pub const TYPE_CURRENT: u8 = 117;
pub const TYPE_FREQUENCY: u8 = 118;
pub const TYPE_PERCENTAGE: u8 = 120;
pub const TYPE_ALTITUDE: u8 = 121;
pub const TYPE_CONCENTRATION: u8 = 125;
pub const TYPE_POWER: u8 = 128;
pub const TYPE_DISTANCE: u8 = 130;
pub const TYPE_ENERGY: u8 = 131;
pub const TYPE_DIRECTION: u8 = 132;
pub const TYPE_SWITCH: u8 = 142;
```

Add new size constants after line 29:

```rust
// Extended type data sizes
pub const SIZE_GENERIC_SENSOR: usize = 4;
pub const SIZE_VOLTAGE: usize = 2;
pub const SIZE_CURRENT: usize = 2;
pub const SIZE_FREQUENCY: usize = 4;
pub const SIZE_PERCENTAGE: usize = 1;
pub const SIZE_ALTITUDE: usize = 2;
pub const SIZE_CONCENTRATION: usize = 2;
pub const SIZE_POWER: usize = 2;
pub const SIZE_DISTANCE: usize = 4;
pub const SIZE_ENERGY: usize = 4;
pub const SIZE_DIRECTION: usize = 2;
pub const SIZE_SWITCH: usize = 1;
```

#### 2. Helper Methods for 32-bit Values (`cayenne_lpp.rs:93-107`)

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add after `read_i24_be()` method (line 107):

```rust
fn read_u32_be(data: &[u8]) -> u32 {
    u32::from_be_bytes([data[0], data[1], data[2], data[3]])
}

fn read_i32_be(data: &[u8]) -> i32 {
    i32::from_be_bytes([data[0], data[1], data[2], data[3]])
}
```

#### 3. Update `get_type_name()` (`cayenne_lpp.rs:59-75`)

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add cases before the `_ => "unknown"` line (line 73):

```rust
TYPE_GENERIC_SENSOR => "generic_sensor",
TYPE_VOLTAGE => "voltage",
TYPE_CURRENT => "current",
TYPE_FREQUENCY => "frequency",
TYPE_PERCENTAGE => "percentage",
TYPE_ALTITUDE => "altitude",
TYPE_CONCENTRATION => "concentration",
TYPE_POWER => "power",
TYPE_DISTANCE => "distance",
TYPE_ENERGY => "energy",
TYPE_DIRECTION => "direction",
TYPE_SWITCH => "switch",
```

#### 4. Update `get_data_size()` (`cayenne_lpp.rs:77-91`)

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add cases before the `_ => Err(...)` line (line 89):

```rust
TYPE_GENERIC_SENSOR => Ok(SIZE_GENERIC_SENSOR),
TYPE_VOLTAGE => Ok(SIZE_VOLTAGE),
TYPE_CURRENT => Ok(SIZE_CURRENT),
TYPE_FREQUENCY => Ok(SIZE_FREQUENCY),
TYPE_PERCENTAGE => Ok(SIZE_PERCENTAGE),
TYPE_ALTITUDE => Ok(SIZE_ALTITUDE),
TYPE_CONCENTRATION => Ok(SIZE_CONCENTRATION),
TYPE_POWER => Ok(SIZE_POWER),
TYPE_DISTANCE => Ok(SIZE_DISTANCE),
TYPE_ENERGY => Ok(SIZE_ENERGY),
TYPE_DIRECTION => Ok(SIZE_DIRECTION),
TYPE_SWITCH => Ok(SIZE_SWITCH),
```

#### 5. Update `decode_sensor_value()` (`cayenne_lpp.rs:109-161`)

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add decoding logic before the `_ => return Err(...)` line (line 158):

```rust
TYPE_GENERIC_SENSOR => {
    let raw = Self::read_u32_be(data);
    json!(raw)
}
TYPE_VOLTAGE => {
    let raw = Self::read_u16_be(data);
    json!(raw as f64 / 100.0)
}
TYPE_CURRENT => {
    let raw = Self::read_u16_be(data);
    json!(raw as f64 / 1000.0)
}
TYPE_FREQUENCY => {
    let raw = Self::read_u32_be(data);
    json!(raw)
}
TYPE_PERCENTAGE => {
    json!(data[0])
}
TYPE_ALTITUDE => {
    let raw = Self::read_i16_be(data);
    json!(raw)
}
TYPE_CONCENTRATION => {
    let raw = Self::read_u16_be(data);
    json!(raw)
}
TYPE_POWER => {
    let raw = Self::read_u16_be(data);
    json!(raw)
}
TYPE_DISTANCE => {
    let raw = Self::read_u32_be(data);
    json!(raw as f64 / 1000.0)
}
TYPE_ENERGY => {
    let raw = Self::read_u32_be(data);
    json!(raw as f64 / 1000.0)
}
TYPE_DIRECTION => {
    let raw = Self::read_u16_be(data);
    json!(raw)
}
TYPE_SWITCH => {
    json!(data[0])
}
```

#### 6. Unit Tests for Basic Extended Types

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add after existing tests (line 523):

```rust
// Extended type tests
#[test]
fn test_generic_sensor() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Generic Sensor (type 100), Value 12345678
    let payload = vec![0x01, 0x64, 0x00, 0xBC, 0x61, 0x4E];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"generic_sensor_1": 12345678}));
}

#[test]
fn test_voltage() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Voltage (type 116), Value 3.3V (raw: 330)
    let payload = vec![0x02, 0x74, 0x01, 0x4A];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"voltage_2": 3.3}));
}

#[test]
fn test_voltage_high() {
    let decoder = CayenneLppDecoder::new();
    // Channel 3, Voltage (type 116), Value 12.5V (raw: 1250)
    let payload = vec![0x03, 0x74, 0x04, 0xE2];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"voltage_3": 12.5}));
}

#[test]
fn test_current() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Current (type 117), Value 1.5A (raw: 1500)
    let payload = vec![0x01, 0x75, 0x05, 0xDC];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"current_1": 1.5}));
}

#[test]
fn test_current_low() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Current (type 117), Value 0.025A (raw: 25)
    let payload = vec![0x02, 0x75, 0x00, 0x19];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"current_2": 0.025}));
}

#[test]
fn test_frequency() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Frequency (type 118), Value 50Hz
    let payload = vec![0x01, 0x76, 0x00, 0x00, 0x00, 0x32];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"frequency_1": 50}));
}

#[test]
fn test_frequency_high() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Frequency (type 118), Value 2400000Hz (2.4 GHz)
    let payload = vec![0x02, 0x76, 0x00, 0x24, 0x9F, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"frequency_2": 2400000}));
}

#[test]
fn test_percentage() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Percentage (type 120), Value 75%
    let payload = vec![0x01, 0x78, 0x4B];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"percentage_1": 75}));
}

#[test]
fn test_percentage_zero() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Percentage (type 120), Value 0%
    let payload = vec![0x02, 0x78, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"percentage_2": 0}));
}

#[test]
fn test_percentage_full() {
    let decoder = CayenneLppDecoder::new();
    // Channel 3, Percentage (type 120), Value 100%
    let payload = vec![0x03, 0x78, 0x64];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"percentage_3": 100}));
}

#[test]
fn test_altitude() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Altitude (type 121), Value 1500m
    let payload = vec![0x01, 0x79, 0x05, 0xDC];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"altitude_1": 1500}));
}

#[test]
fn test_altitude_negative() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Altitude (type 121), Value -50m (below sea level)
    let payload = vec![0x02, 0x79, 0xFF, 0xCE];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"altitude_2": -50}));
}

#[test]
fn test_concentration() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Concentration (type 125), Value 450 PPM
    let payload = vec![0x01, 0x7D, 0x01, 0xC2];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"concentration_1": 450}));
}

#[test]
fn test_concentration_high() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Concentration (type 125), Value 10000 PPM
    let payload = vec![0x02, 0x7D, 0x27, 0x10];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"concentration_2": 10000}));
}

#[test]
fn test_power() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Power (type 128), Value 1500W
    let payload = vec![0x01, 0x80, 0x05, 0xDC];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"power_1": 1500}));
}

#[test]
fn test_power_low() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Power (type 128), Value 5W
    let payload = vec![0x02, 0x80, 0x00, 0x05];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"power_2": 5}));
}

#[test]
fn test_distance() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Distance (type 130), Value 12.345m (raw: 12345)
    let payload = vec![0x01, 0x82, 0x00, 0x00, 0x30, 0x39];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"distance_1": 12.345}));
}

#[test]
fn test_distance_short() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Distance (type 130), Value 0.5m (raw: 500)
    let payload = vec![0x02, 0x82, 0x00, 0x00, 0x01, 0xF4];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"distance_2": 0.5}));
}

#[test]
fn test_energy() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Energy (type 131), Value 123.456 kWh (raw: 123456)
    let payload = vec![0x01, 0x83, 0x00, 0x01, 0xE2, 0x40];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"energy_1": 123.456}));
}

#[test]
fn test_energy_low() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Energy (type 131), Value 0.001 kWh (raw: 1)
    let payload = vec![0x02, 0x83, 0x00, 0x00, 0x00, 0x01];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"energy_2": 0.001}));
}

#[test]
fn test_direction() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Direction (type 132), Value 180° (South)
    let payload = vec![0x01, 0x84, 0x00, 0xB4];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"direction_1": 180}));
}

#[test]
fn test_direction_north() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Direction (type 132), Value 0° (North)
    let payload = vec![0x02, 0x84, 0x00, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"direction_2": 0}));
}

#[test]
fn test_direction_full_circle() {
    let decoder = CayenneLppDecoder::new();
    // Channel 3, Direction (type 132), Value 359°
    let payload = vec![0x03, 0x84, 0x01, 0x67];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"direction_3": 359}));
}

#[test]
fn test_switch_on() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Switch (type 142), Value 1 (on)
    let payload = vec![0x01, 0x8E, 0x01];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"switch_1": 1}));
}

#[test]
fn test_switch_off() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Switch (type 142), Value 0 (off)
    let payload = vec![0x02, 0x8E, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"switch_2": 0}));
}
```

### Success Criteria

#### Automated Verification:
- [x] All unit tests pass: `cargo test -p ponix-payload`
- [x] Code compiles without warnings: `cargo build -p ponix-payload`
- [x] Linting passes: `cargo clippy -p ponix-payload`
- [x] Formatting is correct: `cargo fmt --check -p ponix-payload`
- [x] All 12 basic extended types decode with correct scaling
- [x] Edge cases (0, max values, negative where applicable) are covered

#### Manual Verification:
- [ ] Test vectors match ElectronicCats reference implementation behavior
- [ ] Code follows existing patterns from PR #24
- [ ] No regressions in existing 12 standard types

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that the behavior matches expectations before proceeding to the next phase.

---

## Phase 2: Complex Extended Types (Colour, Unix Time)

### Overview
Implement the two extended types that require special handling: RGB color representation and Unix timestamp to ISO 8601 conversion.

### Changes Required

#### 1. Add Dependency for Time Handling

**File**: `crates/ponix-payload/Cargo.toml`

Add chrono dependency to workspace dependencies in root `Cargo.toml` if not already present, then reference it:

```toml
[dependencies]
chrono = { workspace = true }
```

If chrono is not in workspace dependencies, add to `Cargo.toml`:

```toml
[dependencies]
chrono = "0.4"
```

#### 2. No Colour Struct Needed

**Note**: Colour will be represented as a hex string, so no dedicated struct is required.

#### 3. Type Constants

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add to type constants (after Phase 1 constants):

```rust
pub const TYPE_UNIX_TIME: u8 = 133;
pub const TYPE_COLOUR: u8 = 135;
```

Add to size constants:

```rust
pub const SIZE_UNIX_TIME: usize = 4;
pub const SIZE_COLOUR: usize = 3;
```

#### 4. Update `get_type_name()`

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add cases:

```rust
TYPE_UNIX_TIME => "unix_time",
TYPE_COLOUR => "colour",
```

#### 5. Update `get_data_size()`

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add cases:

```rust
TYPE_UNIX_TIME => Ok(SIZE_UNIX_TIME),
TYPE_COLOUR => Ok(SIZE_COLOUR),
```

#### 6. Import chrono Types

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add to imports at the top (line 1):

```rust
use chrono::{DateTime, Utc};
```

#### 7. Update `decode_sensor_value()`

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add decoding logic:

```rust
TYPE_UNIX_TIME => {
    let timestamp = Self::read_u32_be(data) as i64;
    match DateTime::from_timestamp(timestamp, 0) {
        Some(dt) => json!(dt.to_rfc3339()),
        None => return Err(PayloadError::InvalidPayload(
            format!("Invalid Unix timestamp: {}", timestamp)
        )),
    }
}
TYPE_COLOUR => {
    // Convert RGB bytes to hex string format #RRGGBB
    let hex = format!("#{:02X}{:02X}{:02X}", data[0], data[1], data[2]);
    json!(hex)
}
```

#### 8. Unit Tests for Complex Types

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add tests:

```rust
#[test]
fn test_unix_time() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Unix Time (type 133), Value 1699920000 (2023-11-14T00:00:00Z)
    let payload = vec![0x01, 0x85, 0x65, 0x50, 0xB4, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"unix_time_1": "2023-11-14T00:00:00+00:00"}));
}

#[test]
fn test_unix_time_epoch() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Unix Time (type 133), Value 0 (1970-01-01T00:00:00Z)
    let payload = vec![0x02, 0x85, 0x00, 0x00, 0x00, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"unix_time_2": "1970-01-01T00:00:00+00:00"}));
}

#[test]
fn test_unix_time_recent() {
    let decoder = CayenneLppDecoder::new();
    // Channel 3, Unix Time (type 133), Value 1700000000 (2023-11-15T00:53:20Z)
    let payload = vec![0x03, 0x85, 0x65, 0x51, 0xE8, 0x80];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"unix_time_3": "2023-11-15T00:53:20+00:00"}));
}

#[test]
fn test_colour_white() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Colour (type 135), RGB (255, 255, 255) white = #FFFFFF
    let payload = vec![0x01, 0x87, 0xFF, 0xFF, 0xFF];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"colour_1": "#FFFFFF"}));
}

#[test]
fn test_colour_red() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Colour (type 135), RGB (255, 0, 0) red = #FF0000
    let payload = vec![0x02, 0x87, 0xFF, 0x00, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"colour_2": "#FF0000"}));
}

#[test]
fn test_colour_green() {
    let decoder = CayenneLppDecoder::new();
    // Channel 3, Colour (type 135), RGB (0, 255, 0) green = #00FF00
    let payload = vec![0x03, 0x87, 0x00, 0xFF, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"colour_3": "#00FF00"}));
}

#[test]
fn test_colour_blue() {
    let decoder = CayenneLppDecoder::new();
    // Channel 4, Colour (type 135), RGB (0, 0, 255) blue = #0000FF
    let payload = vec![0x04, 0x87, 0x00, 0x00, 0xFF];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"colour_4": "#0000FF"}));
}

#[test]
fn test_colour_black() {
    let decoder = CayenneLppDecoder::new();
    // Channel 5, Colour (type 135), RGB (0, 0, 0) black = #000000
    let payload = vec![0x05, 0x87, 0x00, 0x00, 0x00];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"colour_5": "#000000"}));
}

#[test]
fn test_colour_custom() {
    let decoder = CayenneLppDecoder::new();
    // Channel 6, Colour (type 135), RGB (128, 64, 192) custom purple = #8040C0
    let payload = vec![0x06, 0x87, 0x80, 0x40, 0xC0];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(result, json!({"colour_6": "#8040C0"}));
}
```

### Success Criteria

#### Automated Verification:
- [x] All unit tests pass: `cargo test -p ponix-payload`
- [x] Code compiles without warnings: `cargo build -p ponix-payload`
- [x] Linting passes: `cargo clippy -p ponix-payload`
- [x] Unix Time correctly converts to ISO 8601 format
- [x] Colour values serialize as hex strings
- [x] Edge cases (epoch time, pure colors) are covered

#### Manual Verification:
- [ ] ISO 8601 timestamps include timezone information
- [ ] Hex color strings follow `#RRGGBB` format with uppercase letters
- [ ] Invalid timestamps are properly rejected

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that the time format and color representation meet requirements before proceeding to the next phase.

---

## Phase 3: Variable-Length Polyline Type

### Overview
Implement the most complex extended type: Polyline (240) with variable-length encoding, delta compression, and precision factor handling.

### Changes Required

#### 1. Polyline Value Struct (`cayenne_lpp.rs:31-50`)

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add after ColourValue struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatePoint {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolylineValue {
    pub points: Vec<CoordinatePoint>,
}
```

#### 2. Precision Factor Mapping

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add helper function in `impl CayenneLppDecoder`:

```rust
fn get_precision_factor(factor_code: u8) -> Option<f64> {
    // Based on ElectronicCats s_valueMap
    match factor_code {
        227 => Some(1.0),
        228 => Some(2.0),
        229 => Some(5.0),
        230 => Some(10.0),
        231 => Some(20.0),
        232 => Some(50.0),
        233 => Some(100.0),
        234 => Some(200.0),
        235 => Some(500.0),
        236 => Some(1000.0),
        237 => Some(2000.0),
        238 => Some(5000.0),
        239 => Some(10000.0),
        _ => None,
    }
}
```

#### 3. Type Constants

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add:

```rust
pub const TYPE_POLYLINE: u8 = 240;
pub const MIN_POLYLINE_SIZE: usize = 8; // size byte + factor + 3-byte lat + 3-byte lon
```

#### 4. Update `get_type_name()`

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add case:

```rust
TYPE_POLYLINE => "polyline",
```

#### 5. Special Handling for Variable-Length Type

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Modify `get_data_size()` to handle Polyline's variable length:

```rust
fn get_data_size(type_id: u8) -> Result<usize> {
    match type_id {
        // ... existing cases ...
        TYPE_POLYLINE => Ok(0), // Variable length, will be validated separately
        _ => Err(PayloadError::UnsupportedType(type_id)),
    }
}
```

#### 6. Update Decoder Main Loop

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Modify the `decode()` method (lines 170-210) to handle Polyline's variable length:

```rust
impl PayloadDecoder for CayenneLppDecoder {
    fn decode(&self, bytes: &[u8]) -> Result<Value> {
        let mut result = Map::new();
        let mut offset = 0;

        while offset < bytes.len() {
            if offset + 2 > bytes.len() {
                return Err(PayloadError::InsufficientData {
                    expected: 2,
                    actual: bytes.len() - offset,
                });
            }

            let channel = bytes[offset];
            let type_id = bytes[offset + 1];
            offset += 2;

            // Special handling for variable-length Polyline type
            let data_size = if type_id == TYPE_POLYLINE {
                // Polyline uses first byte as size indicator
                if offset >= bytes.len() {
                    return Err(PayloadError::InsufficientData {
                        expected: 1,
                        actual: 0,
                    });
                }
                let size = bytes[offset] as usize;

                if size < MIN_POLYLINE_SIZE {
                    return Err(PayloadError::InvalidPayload(
                        format!("Polyline size {} is less than minimum {}", size, MIN_POLYLINE_SIZE)
                    ));
                }

                size
            } else {
                Self::get_data_size(type_id)?
            };

            if offset + data_size > bytes.len() {
                return Err(PayloadError::InsufficientData {
                    expected: data_size,
                    actual: bytes.len() - offset,
                });
            }

            let data = &bytes[offset..offset + data_size];
            let value = self.decode_sensor_value(type_id, data)?;
            offset += data_size;

            let type_name = Self::get_type_name(type_id);
            let field_name = format!("{}_{}", type_name, channel);
            result.insert(field_name, value);
        }

        Ok(Value::Object(result))
    }
}
```

#### 7. Polyline Decoding Logic

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add to `decode_sensor_value()`:

```rust
TYPE_POLYLINE => {
    // data[0] = size (already validated)
    // data[1] = precision factor code
    let factor = Self::get_precision_factor(data[1])
        .ok_or_else(|| PayloadError::InvalidPayload(
            format!("Invalid Polyline precision factor: {}", data[1])
        ))?;

    const SCALE_FACTOR: f64 = 10000.0;

    // Decode base coordinates (bytes 2-7)
    let base_lat_raw = Self::read_i24_be(&data[2..5]);
    let base_lon_raw = Self::read_i24_be(&data[5..8]);

    let mut prev_lat = (base_lat_raw as f64) * factor;
    let mut prev_lon = (base_lon_raw as f64) * factor;

    let mut points = vec![CoordinatePoint {
        latitude: prev_lat / SCALE_FACTOR,
        longitude: prev_lon / SCALE_FACTOR,
    }];

    // Decode delta coordinates (bytes 8+)
    // Each byte contains two 4-bit signed nibbles: [lat_delta:4][lon_delta:4]
    let mut i = 8;
    while i < data.len() {
        let delta_byte = data[i] as i8;

        // Extract 4-bit signed deltas
        // Upper nibble: latitude delta
        let lat_delta = ((delta_byte >> 4) & 0x0F) as i8;
        let lat_delta = if lat_delta > 7 { lat_delta - 16 } else { lat_delta };

        // Lower nibble: longitude delta
        let lon_delta = (delta_byte & 0x0F) as i8;
        let lon_delta = if lon_delta > 7 { lon_delta - 16 } else { lon_delta };

        prev_lat += (lat_delta as f64) * factor;
        prev_lon += (lon_delta as f64) * factor;

        points.push(CoordinatePoint {
            latitude: prev_lat / SCALE_FACTOR,
            longitude: prev_lon / SCALE_FACTOR,
        });

        i += 1;
    }

    json!(PolylineValue { points })
}
```

#### 8. Unit Tests for Polyline Type

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add comprehensive tests:

```rust
#[test]
fn test_polyline_minimum() {
    let decoder = CayenneLppDecoder::new();
    // Channel 1, Polyline (type 240)
    // Size: 8, Factor: 239 (10000.0), Base: Lat 42.3519° (423519), Lon -87.9094° (-879094)
    // 8 bytes minimum with no delta points
    let payload = vec![
        0x01, 0xF0, // Channel 1, Type 240
        0x08, 0xEF, // Size: 8, Factor: 239
        0x06, 0x76, 0x5F, // Lat: 423519 (42.3519°)
        0xF2, 0x96, 0x0A, // Lon: -879094 (-87.9094°)
    ];
    let result = decoder.decode(&payload).unwrap();
    let expected = json!({
        "polyline_1": {
            "points": [
                {"latitude": 42.3519, "longitude": -87.9094}
            ]
        }
    });
    assert_eq!(result, expected);
}

#[test]
fn test_polyline_two_points() {
    let decoder = CayenneLppDecoder::new();
    // Channel 2, Polyline with base point and one delta
    // Base: (42.0°, -87.0°), Delta: (+0.001°, +0.001°)
    // Factor 239 (10000.0): delta nibbles would be 0,0 to 1,1
    let payload = vec![
        0x02, 0xF0, // Channel 2, Type 240
        0x09, 0xEF, // Size: 9, Factor: 239
        0x06, 0x69, 0x78, // Lat: 420000 (42.0°)
        0xF2, 0xA3, 0x10, // Lon: -870000 (-87.0°)
        0x11, // Delta: lat+1, lon+1 (both nibbles = 0x1)
    ];
    let result = decoder.decode(&payload).unwrap();

    // Expected: base (42.0, -87.0) + delta (0.0001, 0.0001)
    let polyline = result.get("polyline_2").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();
    assert_eq!(points.len(), 2);

    // Verify base point
    assert_eq!(points[0].get("latitude").unwrap().as_f64().unwrap(), 42.0);
    assert_eq!(points[0].get("longitude").unwrap().as_f64().unwrap(), -87.0);

    // Verify second point (delta applied)
    let lat2 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon2 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat2 - 42.0001).abs() < 0.00001);
    assert!((lon2 - -86.9999).abs() < 0.00001);
}

#[test]
fn test_polyline_negative_deltas() {
    let decoder = CayenneLppDecoder::new();
    // Base point + negative deltas
    // Delta nibbles: 0xF (-1) for both lat and lon
    let payload = vec![
        0x03, 0xF0, // Channel 3, Type 240
        0x09, 0xEF, // Size: 9, Factor: 239
        0x06, 0x69, 0x78, // Lat: 420000 (42.0°)
        0xF2, 0xA3, 0x10, // Lon: -870000 (-87.0°)
        0xFF, // Delta: lat-1, lon-1 (both nibbles = 0xF = -1)
    ];
    let result = decoder.decode(&payload).unwrap();

    let polyline = result.get("polyline_3").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();
    assert_eq!(points.len(), 2);

    // Verify second point has negative delta
    let lat2 = points[1].get("latitude").unwrap().as_f64().unwrap();
    let lon2 = points[1].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat2 - 41.9999).abs() < 0.00001);
    assert!((lon2 - -87.0001).abs() < 0.00001);
}

#[test]
fn test_polyline_multiple_points() {
    let decoder = CayenneLppDecoder::new();
    // Base point + 3 delta points
    let payload = vec![
        0x04, 0xF0, // Channel 4, Type 240
        0x0B, 0xEF, // Size: 11, Factor: 239
        0x06, 0x69, 0x78, // Base Lat: 42.0°
        0xF2, 0xA3, 0x10, // Base Lon: -87.0°
        0x11, // Point 2: +1, +1
        0x22, // Point 3: +2, +2
        0xF0, // Point 4: -1, 0
    ];
    let result = decoder.decode(&payload).unwrap();

    let polyline = result.get("polyline_4").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();
    assert_eq!(points.len(), 4); // Base + 3 deltas
}

#[test]
fn test_polyline_invalid_size() {
    let decoder = CayenneLppDecoder::new();
    // Size less than minimum (MIN_POLYLINE_SIZE = 8)
    let payload = vec![
        0x01, 0xF0, // Channel 1, Type 240
        0x07, 0xEF, // Size: 7 (too small!)
        0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    let result = decoder.decode(&payload);
    assert!(matches!(result, Err(PayloadError::InvalidPayload(_))));
}

#[test]
fn test_polyline_invalid_factor() {
    let decoder = CayenneLppDecoder::new();
    // Invalid precision factor code
    let payload = vec![
        0x01, 0xF0, // Channel 1, Type 240
        0x08, 0x00, // Size: 8, Factor: 0 (invalid!)
        0x06, 0x76, 0x5F,
        0xF2, 0x96, 0x0A,
    ];
    let result = decoder.decode(&payload);
    assert!(matches!(result, Err(PayloadError::InvalidPayload(_))));
}

#[test]
fn test_polyline_different_precision() {
    let decoder = CayenneLppDecoder::new();
    // Test with different precision factor (233 = 100.0)
    let payload = vec![
        0x05, 0xF0, // Channel 5, Type 240
        0x08, 0xE9, // Size: 8, Factor: 233 (100.0)
        0x00, 0x28, 0xB0, // Lat: 10416 * 100 / 10000 = 104.16°
        0xFF, 0xD7, 0x50, // Lon: -10416 * 100 / 10000 = -104.16°
    ];
    let result = decoder.decode(&payload).unwrap();

    let polyline = result.get("polyline_5").unwrap();
    let points = polyline.get("points").unwrap().as_array().unwrap();
    assert_eq!(points.len(), 1);

    let lat = points[0].get("latitude").unwrap().as_f64().unwrap();
    let lon = points[0].get("longitude").unwrap().as_f64().unwrap();
    assert!((lat - 104.16).abs() < 0.01);
    assert!((lon - -104.16).abs() < 0.01);
}
```

### Success Criteria

#### Automated Verification:
- [ ] All unit tests pass: `cargo test -p ponix-payload`
- [ ] Code compiles without warnings: `cargo build -p ponix-payload`
- [ ] Linting passes: `cargo clippy -p ponix-payload`
- [ ] Variable-length parsing handles size byte correctly
- [ ] Precision factors map correctly (codes 227-239)
- [ ] Base coordinates decode with sign extension
- [ ] Delta nibbles extract and apply correctly (signed 4-bit)
- [ ] Invalid sizes and factors are rejected

#### Manual Verification:
- [ ] Polyline coordinates are accurate compared to reference implementation
- [ ] Multi-point polylines maintain coordinate sequence
- [ ] Negative deltas work correctly
- [ ] Different precision factors scale appropriately

**Implementation Note**: After completing this phase and all automated verification passes, pause here for manual confirmation that the Polyline implementation is correct and robust before proceeding to final integration tests.

---

## Phase 4: Integration Testing & Documentation

### Overview
Add comprehensive integration tests with multi-sensor payloads combining standard and extended types, and update documentation.

### Changes Required

#### 1. Multi-Sensor Integration Tests

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Add integration tests combining standard and extended types:

```rust
#[test]
fn test_extended_multi_sensor() {
    let decoder = CayenneLppDecoder::new();
    // Power monitoring system:
    // Ch 0: Voltage 230V, Ch 1: Current 5A, Ch 2: Power 1150W, Ch 3: Energy 123.456 kWh
    let payload = vec![
        0x00, 0x74, 0x59, 0xD6, // Voltage: 23000 = 230.0V
        0x01, 0x75, 0x13, 0x88, // Current: 5000 = 5.0A
        0x02, 0x80, 0x04, 0x7E, // Power: 1150W
        0x03, 0x83, 0x00, 0x01, 0xE2, 0x40, // Energy: 123456 = 123.456 kWh
    ];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(
        result,
        json!({
            "voltage_0": 230.0,
            "current_1": 5.0,
            "power_2": 1150,
            "energy_3": 123.456
        })
    );
}

#[test]
fn test_environmental_extended_suite() {
    let decoder = CayenneLppDecoder::new();
    // Extended environmental monitoring:
    // Ch 0: Temp 25°C, Ch 1: Humidity 60%, Ch 2: Pressure 1013.2hPa,
    // Ch 3: Altitude 500m, Ch 4: CO2 Concentration 450ppm
    let payload = vec![
        0x00, 0x67, 0x00, 0xFA, // Temperature: 250 = 25.0°C
        0x01, 0x68, 0x78, // Humidity: 120 = 60.0%
        0x02, 0x73, 0x27, 0x94, // Barometer: 10132 = 1013.2 hPa
        0x03, 0x79, 0x01, 0xF4, // Altitude: 500m
        0x04, 0x7D, 0x01, 0xC2, // Concentration: 450 ppm
    ];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(
        result,
        json!({
            "temperature_0": 25.0,
            "humidity_1": 60.0,
            "barometer_2": 1013.2,
            "altitude_3": 500,
            "concentration_4": 450
        })
    );
}

#[test]
fn test_navigation_extended() {
    let decoder = CayenneLppDecoder::new();
    // Navigation system: GPS + Direction + Distance + Altitude
    let payload = vec![
        0x00, 0x88, 0x06, 0x76, 0x5F, 0xF2, 0x96, 0x0A, 0x00, 0x03, 0xE8, // GPS
        0x01, 0x84, 0x00, 0xB4, // Direction: 180° (South)
        0x02, 0x82, 0x00, 0x00, 0x30, 0x39, // Distance: 12.345m
        0x03, 0x79, 0x05, 0xDC, // Altitude: 1500m
    ];
    let result = decoder.decode(&payload).unwrap();

    assert_eq!(result.get("direction_1"), Some(&json!(180)));
    assert_eq!(result.get("distance_2"), Some(&json!(12.345)));
    assert_eq!(result.get("altitude_3"), Some(&json!(1500)));
}

#[test]
fn test_smart_home_extended() {
    let decoder = CayenneLppDecoder::new();
    // Smart home: Switch states + Presence + Colour (RGB LED)
    let payload = vec![
        0x00, 0x8E, 0x01, // Switch 0: ON
        0x01, 0x8E, 0x00, // Switch 1: OFF
        0x02, 0x66, 0x01, // Presence: detected
        0x03, 0x87, 0xFF, 0xA5, 0x00, // Colour: Orange #FFA500
    ];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(
        result,
        json!({
            "switch_0": 1,
            "switch_1": 0,
            "presence_2": 1,
            "colour_3": "#FFA500"
        })
    );
}

#[test]
fn test_mixed_standard_and_extended() {
    let decoder = CayenneLppDecoder::new();
    // Mix of standard (analog, digital) and extended (voltage, percentage)
    let payload = vec![
        0x00, 0x00, 0x64, // Digital Input: 100
        0x01, 0x02, 0x00, 0x0A, // Analog Input: 0.1
        0x02, 0x74, 0x01, 0x4A, // Voltage: 3.3V (extended)
        0x03, 0x78, 0x4B, // Percentage: 75% (extended)
    ];
    let result = decoder.decode(&payload).unwrap();
    assert_eq!(
        result,
        json!({
            "digital_input_0": 100,
            "analog_input_1": 0.1,
            "voltage_2": 3.3,
            "percentage_3": 75
        })
    );
}

#[test]
fn test_full_sensor_suite() {
    let decoder = CayenneLppDecoder::new();
    // Comprehensive suite combining many types
    let payload = vec![
        0x00, 0x67, 0x01, 0x10, // Temperature: 27.2°C
        0x01, 0x68, 0x64, // Humidity: 50%
        0x02, 0x74, 0x0C, 0xE4, // Voltage: 33.0V
        0x03, 0x75, 0x07, 0xD0, // Current: 2.0A
        0x04, 0x80, 0x02, 0x94, // Power: 660W
        0x05, 0x78, 0x55, // Percentage: 85%
        0x06, 0x8E, 0x01, // Switch: ON
        0x07, 0x85, 0x65, 0x50, 0xB4, 0x00, // Unix Time
    ];
    let result = decoder.decode(&payload).unwrap();

    // Verify all fields exist
    assert!(result.get("temperature_0").is_some());
    assert!(result.get("humidity_1").is_some());
    assert!(result.get("voltage_2").is_some());
    assert!(result.get("current_3").is_some());
    assert!(result.get("power_4").is_some());
    assert!(result.get("percentage_5").is_some());
    assert!(result.get("switch_6").is_some());
    assert!(result.get("unix_time_7").is_some());
}

#[test]
fn test_polyline_with_other_sensors() {
    let decoder = CayenneLppDecoder::new();
    // GPS tracking with polyline path and timestamp
    let payload = vec![
        0x00, 0x88, 0x06, 0x76, 0x5F, 0xF2, 0x96, 0x0A, 0x00, 0x03, 0xE8, // GPS
        0x01, 0xF0, 0x09, 0xEF, 0x06, 0x69, 0x78, 0xF2, 0xA3, 0x10, 0x11, // Polyline
        0x02, 0x85, 0x65, 0x50, 0xB4, 0x00, // Unix Time
        0x03, 0x79, 0x05, 0xDC, // Altitude
    ];
    let result = decoder.decode(&payload).unwrap();

    assert!(result.get("gps_0").is_some());
    assert!(result.get("polyline_1").is_some());
    assert!(result.get("unix_time_2").is_some());
    assert!(result.get("altitude_3").is_some());
}
```

#### 2. Documentation Updates

**File**: `crates/ponix-payload/src/cayenne_lpp.rs`

Update module-level documentation at the top of the file:

```rust
//! Cayenne LPP (Low Power Payload) decoder implementation.
//!
//! This module implements decoding for the Cayenne LPP format, supporting both
//! the standard 12 sensor types and the ElectronicCats extended 15 types (27 total).
//!
//! # Supported Sensor Types
//!
//! ## Standard Types (0-136)
//! - Digital I/O (0, 1)
//! - Analog I/O (2, 3)
//! - Illuminance (101)
//! - Presence (102)
//! - Temperature (103)
//! - Humidity (104)
//! - Accelerometer (113)
//! - Barometer (115)
//! - Gyrometer (134)
//! - GPS (136)
//!
//! ## Extended Types (100-240)
//! - Generic Sensor (100)
//! - Voltage (116)
//! - Current (117)
//! - Frequency (118)
//! - Percentage (120)
//! - Altitude (121)
//! - Concentration (125)
//! - Power (128)
//! - Distance (130)
//! - Energy (131)
//! - Direction (132)
//! - Unix Time (133) - returns ISO 8601 string
//! - Colour (135) - returns RGB object
//! - Switch (142)
//! - Polyline (240) - variable length with coordinate compression
//!
//! # Output Format
//!
//! Decoded payloads produce JSON objects with field names in the format:
//! `{type_name}_{channel}`, where channel is the sensor channel number.
//!
//! Complex types (GPS, Accelerometer, Gyrometer, Polyline) are represented
//! as nested JSON objects with structured fields. Colour is represented as a hex
//! string (#RRGGBB), and Unix Time as an ISO 8601 timestamp string.
//!
//! # Example
//!
//! ```rust
//! use ponix_payload::{PayloadDecoder, CayenneLppDecoder};
//!
//! let decoder = CayenneLppDecoder::new();
//! let payload = vec![0x03, 0x67, 0x01, 0x10]; // Temperature on channel 3
//! let result = decoder.decode(&payload).unwrap();
//! // result: {"temperature_3": 27.2}
//! ```
//!
//! # References
//!
//! - Cayenne LPP Specification: https://developers.mydevices.com/cayenne/docs/lora/
//! - ElectronicCats Extended Types: https://github.com/ElectronicCats/CayenneLPP
```

#### 3. README Update (Optional)

**File**: `crates/ponix-payload/README.md` (create if doesn't exist)

```markdown
# ponix-payload

Payload decoder implementations for the Ponix IoT platform.

## Features

### Cayenne LPP Decoder

Complete implementation of the Cayenne LPP (Low Power Payload) format with support for:

- **27 sensor types** (12 standard + 15 extended)
- **Big-endian byte order** for network compatibility
- **Type-specific scaling** for accurate measurements
- **Complex types** with structured output (GPS, Accelerometer, Gyrometer, Polyline) and string representations (Colour as hex, Unix Time as ISO 8601)
- **Variable-length encoding** for coordinate compression (Polyline type)
- **ISO 8601 timestamps** for Unix Time type
- **Hex color strings** for RGB colors (#RRGGBB format)

## Usage

```rust
use ponix_payload::{PayloadDecoder, CayenneLppDecoder};

let decoder = CayenneLppDecoder::new();
let payload = vec![
    0x00, 0x67, 0x00, 0xFA, // Temperature: 25.0°C
    0x01, 0x74, 0x0C, 0xE4, // Voltage: 33.0V
];

let result = decoder.decode(&payload).unwrap();
// result: {"temperature_0": 25.0, "voltage_1": 33.0}
```

## Testing

Run unit tests:
```bash
cargo test -p ponix-payload
```

## References

- [Cayenne LPP Specification](https://developers.mydevices.com/cayenne/docs/lora/)
- [ElectronicCats Extended Types](https://github.com/ElectronicCats/CayenneLPP)
```

### Success Criteria

#### Automated Verification:
- [ ] All integration tests pass: `cargo test -p ponix-payload`
- [ ] No test failures or panics
- [ ] Test coverage includes all 27 sensor types
- [ ] Multi-sensor payloads decode correctly
- [ ] Code compiles without warnings: `cargo build -p ponix-payload`
- [ ] Documentation builds without errors: `cargo doc -p ponix-payload --no-deps`

#### Manual Verification:
- [ ] Integration tests cover realistic use cases
- [ ] Documentation is clear and accurate
- [ ] All 27 types are documented in module docs
- [ ] Examples in documentation work correctly
- [ ] README provides useful quick-start information

**Implementation Note**: This is the final phase. After all automated tests pass and documentation is complete, the implementation is ready for code review and integration into the main service.

---

## Testing Strategy

### Unit Testing Approach

Each sensor type requires 2-3 tests:
1. **Basic functionality** - Standard value with expected scaling
2. **Edge cases** - Min/max values, zero values
3. **Negative values** - Where applicable (signed types)

**Total Expected Tests**: ~60 tests
- Existing standard types: 28 tests
- Basic extended types (Phase 1): 26 tests (12 types × ~2 tests each)
- Complex types (Phase 2): 9 tests (2 types × 4-5 tests each)
- Polyline type (Phase 3): 8 tests
- Integration tests (Phase 4): 8 tests

### Integration Testing Focus

Integration tests validate:
- Multi-sensor payloads with mixed standard/extended types
- Real-world sensor combinations (power monitoring, environmental, navigation)
- Polyline integration with other sensor types
- Edge cases at payload boundaries

### Test Execution

Run tests with:
```bash
# All tests
cargo test -p ponix-payload

# Specific test
cargo test -p ponix-payload test_voltage

# With output
cargo test -p ponix-payload -- --nocapture

# Watch mode (if cargo-watch installed)
cargo watch -x "test -p ponix-payload"
```

## Performance Considerations

### Computational Complexity

- **Time Complexity**: O(n) where n is payload size (unchanged from current implementation)
- **Space Complexity**: O(m) where m is number of sensors (unchanged)
- **No heap allocations** during parsing for simple types
- **Small allocations** for complex types (Vec for Polyline points)

### Memory Impact

- **Constants**: Additional 24 type constants + 12 size constants (~36 bytes)
- **Structs**: ColourValue (3 bytes), CoordinatePoint (16 bytes), PolylineValue (24 bytes + Vec)
- **Code size**: Estimated ~500 additional lines of decoding logic

### Optimization Notes

- Big-endian helper methods use efficient `from_be_bytes()` stdlib functions
- No intermediate buffers or copies during decoding
- JSON serialization deferred to serde (efficient)
- Polyline precision factor lookup uses match (compile-time optimization)

## Migration Notes

### Backwards Compatibility

- **No breaking changes** to existing API or trait interface
- **Existing 12 types** unchanged in behavior or output format
- **Existing tests** remain valid and passing
- **JSON output format** consistent with current implementation

### Version Compatibility

- Current implementation: Cayenne LPP v1.0 (12 types)
- After implementation: Cayenne LPP v1.0 + ElectronicCats extended (27 types)
- Backward compatible with existing payloads

### Integration Points

No changes required in:
- `PayloadDecoder` trait consumers
- NATS consumer integration
- ClickHouse storage schema (JSON columns handle new types automatically)
- gRPC API (if payload decoder is exposed)

## References

- **Original ticket**: GitHub Issue #25 (ponix-dev/ponix-rs)
- **ElectronicCats Library**: https://github.com/ElectronicCats/CayenneLPP
  - Header file: `src/CayenneLPP.h`
  - Polyline implementation: `src/CayenneLPPPolyline.cpp`
- **Cayenne LPP Specification**: https://developers.mydevices.com/cayenne/docs/lora/
- **Standard implementation (PR #24)**: Commit `0adaed1` (Nov 18, 2025)
- **Existing plan document**: `.thoughts/plans/2025-11-18-22-cayenne-lpp-decoder.md`

## Implementation Timeline

Estimated implementation effort per phase:
- **Phase 1** (Basic Extended Types): 2-3 hours
  - 12 types with straightforward scaling
  - 26 unit tests
- **Phase 2** (Complex Types): 1-2 hours
  - chrono integration for Unix Time
  - RGB struct for Colour
  - 9 unit tests
- **Phase 3** (Polyline): 3-4 hours
  - Variable-length parsing
  - Delta decoding logic
  - Precision factor mapping
  - 8 unit tests
- **Phase 4** (Integration & Docs): 1 hour
  - 8 integration tests
  - Documentation updates

**Total Estimated Time**: 7-10 hours of focused implementation

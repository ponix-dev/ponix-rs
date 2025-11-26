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
//! - Generic Sensor (100): Generic unsigned 32-bit integer
//! - Voltage (116): Electrical voltage in volts (0.01V precision)
//! - Current (117): Electrical current in amperes (0.001A precision)
//! - Frequency (118): Frequency in hertz (1 Hz precision)
//! - Percentage (120): Percentage value 0-100 (1% precision)
//! - Altitude (121): Altitude in meters (1m precision)
//! - Concentration (125): Concentration in parts per million (1 ppm precision)
//! - Power (128): Electrical power in watts (1W precision)
//! - Distance (130): Distance in meters (0.001m precision)
//! - Energy (131): Energy in kilowatt-hours (0.001 kWh precision)
//! - Direction (132): Direction in degrees 0-360 (1° precision)
//! - Unix Time (133): Unix timestamp as ISO 8601 string
//! - Colour (135): RGB color as hex string (#RRGGBB)
//! - Switch (142): Boolean on/off state
//! - Polyline (240): Variable-length GPS path with delta encoding
//!
//! # Payload Format
//!
//! Each sensor reading in a Cayenne LPP payload consists of:
//! - 1 byte: Channel number (0-255)
//! - 1 byte: Sensor type ID
//! - N bytes: Sensor data (size depends on type)
//!
//! Multiple sensors can be encoded in a single payload by concatenating their data.
//!
//! # References
//!
//! - [Cayenne LPP Specification](https://developers.mydevices.com/cayenne/docs/lora/)
//! - [ElectronicCats Extended Types](https://github.com/ElectronicCats/CayenneLPP)

use crate::domain::{PayloadDecoder, PayloadError, Result};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

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
pub const TYPE_UNIX_TIME: u8 = 133;
pub const TYPE_COLOUR: u8 = 135;
pub const TYPE_SWITCH: u8 = 142;
pub const TYPE_POLYLINE: u8 = 240;

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
pub const SIZE_UNIX_TIME: usize = 4;
pub const SIZE_COLOUR: usize = 3;
pub const SIZE_SWITCH: usize = 1;
pub const MIN_POLYLINE_SIZE: usize = 8; // size byte + factor + 3-byte lat + 3-byte lon

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatePoint {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolylineValue {
    pub points: Vec<CoordinatePoint>,
}

pub struct CayenneLppDecoder;

impl CayenneLppDecoder {
    pub fn new() -> Self {
        Self
    }

    fn get_type_name(type_id: u8) -> &'static str {
        match type_id {
            TYPE_DIGITAL_INPUT => "digital_input",
            TYPE_DIGITAL_OUTPUT => "digital_output",
            TYPE_ANALOG_INPUT => "analog_input",
            TYPE_ANALOG_OUTPUT => "analog_output",
            TYPE_ILLUMINANCE => "illuminance",
            TYPE_PRESENCE => "presence",
            TYPE_TEMPERATURE => "temperature",
            TYPE_HUMIDITY => "humidity",
            TYPE_ACCELEROMETER => "accelerometer",
            TYPE_BAROMETER => "barometer",
            TYPE_GYROMETER => "gyrometer",
            TYPE_GPS => "gps",
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
            TYPE_UNIX_TIME => "unix_time",
            TYPE_COLOUR => "colour",
            TYPE_SWITCH => "switch",
            TYPE_POLYLINE => "polyline",
            _ => "unknown",
        }
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
            TYPE_UNIX_TIME => Ok(SIZE_UNIX_TIME),
            TYPE_COLOUR => Ok(SIZE_COLOUR),
            TYPE_SWITCH => Ok(SIZE_SWITCH),
            TYPE_POLYLINE => Ok(0), // Variable length, will be validated separately
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
        let value =
            (i32::from(data[0]) << 24) | (i32::from(data[1]) << 16) | (i32::from(data[2]) << 8);
        value >> 8
    }

    fn read_u32_be(data: &[u8]) -> u32 {
        u32::from_be_bytes([data[0], data[1], data[2], data[3]])
    }

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
            TYPE_UNIX_TIME => {
                let timestamp = Self::read_u32_be(data) as i64;
                match DateTime::from_timestamp(timestamp, 0) {
                    Some(dt) => json!(dt.to_rfc3339()),
                    None => {
                        return Err(PayloadError::InvalidPayload(format!(
                            "Invalid Unix timestamp: {}",
                            timestamp
                        )))
                    }
                }
            }
            TYPE_COLOUR => {
                // Convert RGB bytes to hex string format #RRGGBB
                let hex = format!("#{:02X}{:02X}{:02X}", data[0], data[1], data[2]);
                json!(hex)
            }
            TYPE_SWITCH => {
                json!(data[0] != 0)
            }
            TYPE_POLYLINE => {
                // data[0] = size (already validated)
                // data[1] = precision factor code
                let factor = Self::get_precision_factor(data[1]).ok_or_else(|| {
                    PayloadError::InvalidPayload(format!(
                        "Invalid Polyline precision factor: {}",
                        data[1]
                    ))
                })?;

                // Decode base coordinates (bytes 2-7)
                // The factor represents the precision: higher factor = higher precision raw values
                // Formula: coordinate = raw / factor (factor is the scale of raw values)
                let base_lat_raw = Self::read_i24_be(&data[2..5]);
                let base_lon_raw = Self::read_i24_be(&data[5..8]);

                let mut prev_lat = base_lat_raw as f64;
                let mut prev_lon = base_lon_raw as f64;

                let mut points = vec![CoordinatePoint {
                    latitude: prev_lat / factor,
                    longitude: prev_lon / factor,
                }];

                // Decode delta coordinates (bytes 8+)
                // Each byte contains two 4-bit signed nibbles: [lat_delta:4][lon_delta:4]
                let mut i = 8;
                while i < data.len() {
                    let delta_byte = data[i] as i8;

                    // Extract 4-bit signed deltas
                    // Upper nibble: latitude delta
                    let lat_delta = (delta_byte >> 4) & 0x0F;
                    let lat_delta = if lat_delta > 7 {
                        lat_delta - 16
                    } else {
                        lat_delta
                    };

                    // Lower nibble: longitude delta
                    let lon_delta = delta_byte & 0x0F;
                    let lon_delta = if lon_delta > 7 {
                        lon_delta - 16
                    } else {
                        lon_delta
                    };

                    // Apply delta (delta values are at same precision as base coordinates)
                    prev_lat += lat_delta as f64;
                    prev_lon += lon_delta as f64;

                    points.push(CoordinatePoint {
                        latitude: prev_lat / factor,
                        longitude: prev_lon / factor,
                    });

                    i += 1;
                }

                json!(PolylineValue { points })
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
        // Result structure: {type_name_channel: value}
        let mut result = Map::new();
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
                    return Err(PayloadError::InvalidPayload(format!(
                        "Polyline size {} is less than minimum {}",
                        size, MIN_POLYLINE_SIZE
                    )));
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

            // Create field name in format "type_name_channel"
            let type_name = Self::get_type_name(type_id);
            let field_name = format!("{}_{}", type_name, channel);
            result.insert(field_name, value);
        }

        Ok(Value::Object(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PayloadDecoder;

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
        assert_eq!(result, json!({"digital_input_3": 100}));
    }

    #[test]
    fn test_digital_output() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Digital Output (type 1), Value 255
        let payload = vec![0x05, 0x01, 0xFF];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"digital_output_5": 255}));
    }

    #[test]
    fn test_analog_input() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Analog Input (type 2), Value 0.1 (raw: 10)
        let payload = vec![0x03, 0x02, 0x00, 0x0A];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"analog_input_3": 0.1}));
    }

    #[test]
    fn test_analog_output() {
        let decoder = CayenneLppDecoder::new();
        // Channel 7, Analog Output (type 3), Value -1.5 (raw: -150)
        let payload = vec![0x07, 0x03, 0xFF, 0x6A];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"analog_output_7": -1.5}));
    }

    #[test]
    fn test_temperature() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Temperature (type 103), Value 27.2°C (raw: 272)
        let payload = vec![0x03, 0x67, 0x01, 0x10];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"temperature_3": 27.2}));
    }

    #[test]
    fn test_temperature_negative() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Temperature (type 103), Value -0.1°C (raw: -1)
        let payload = vec![0x05, 0x67, 0xFF, 0xFF];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"temperature_5": -0.1}));
    }

    #[test]
    fn test_humidity() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Humidity (type 104), Value 50% (raw: 100)
        let payload = vec![0x05, 0x68, 0x64];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"humidity_5": 50.0}));
    }

    #[test]
    fn test_humidity_full() {
        let decoder = CayenneLppDecoder::new();
        // Channel 1, Humidity (type 104), Value 100% (raw: 200)
        let payload = vec![0x01, 0x68, 0xC8];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"humidity_1": 100.0}));
    }

    #[test]
    fn test_illuminance() {
        let decoder = CayenneLppDecoder::new();
        // Channel 2, Illuminance (type 101), Value 1000 lux
        let payload = vec![0x02, 0x65, 0x03, 0xE8];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"illuminance_2": 1000}));
    }

    #[test]
    fn test_presence() {
        let decoder = CayenneLppDecoder::new();
        // Channel 5, Presence (type 102), Value 1 (detected)
        let payload = vec![0x05, 0x66, 0x01];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"presence_5": 1}));
    }

    #[test]
    fn test_accelerometer() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Accelerometer (type 113), X=0.001, Y=0.002, Z=0.003
        let payload = vec![0x03, 0x71, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"accelerometer_3": {"x": 0.001, "y": 0.002, "z": 0.003}})
        );
    }

    #[test]
    fn test_barometer() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Barometer (type 115), Value 1013.2 hPa (raw: 10132)
        let payload = vec![0x03, 0x73, 0x27, 0x94];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"barometer_3": 1013.2}));
    }

    #[test]
    fn test_gyrometer() {
        let decoder = CayenneLppDecoder::new();
        // Channel 3, Gyrometer (type 134), X=1.0, Y=2.0, Z=3.0
        let payload = vec![0x03, 0x86, 0x00, 0x64, 0x00, 0xC8, 0x01, 0x2C];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"gyrometer_3": {"x": 1.0, "y": 2.0, "z": 3.0}})
        );
    }

    #[test]
    fn test_gps() {
        let decoder = CayenneLppDecoder::new();
        // Channel 1, GPS (type 136)
        // Lat: 42.3519° (raw: 423519), Lon: -87.9094° (raw: -879094), Alt: 10m (raw: 1000)
        let payload = vec![
            0x01, 0x88, 0x06, 0x76, 0x5F, 0xF2, 0x96, 0x0A, 0x00, 0x03, 0xE8,
        ];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"gps_1": {
                "latitude": 42.3519,
                "longitude": -87.9094,
                "altitude": 10.0
            }})
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
                "temperature_3": 27.2,
                "humidity_5": 50.0
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
                "temperature_3": 27.2,
                "temperature_5": -0.1
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
                "temperature_0": 23.0,
                "humidity_1": 100.0,
                "illuminance_2": 1000,
                "barometer_3": 1018.0
            })
        );
    }

    #[test]
    fn test_analog_input_negative() {
        let decoder = CayenneLppDecoder::new();
        // Channel 1, Analog Input (type 2), Value -2.75 (raw: -275)
        let payload = vec![0x01, 0x02, 0xFE, 0xED];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"analog_input_1": -2.75}));
    }

    #[test]
    fn test_accelerometer_negative() {
        let decoder = CayenneLppDecoder::new();
        // Channel 2, Accelerometer (type 113), X=-0.5, Y=0.0, Z=1.0
        let payload = vec![0x02, 0x71, 0xFE, 0x0C, 0x00, 0x00, 0x03, 0xE8];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"accelerometer_2": {"x": -0.5, "y": 0.0, "z": 1.0}})
        );
    }

    #[test]
    fn test_gyrometer_negative() {
        let decoder = CayenneLppDecoder::new();
        // Channel 4, Gyrometer (type 134), X=-10.5, Y=5.25, Z=0.0
        // X: -10.5 * 100 = -1050 = 0xFBE6, Y: 5.25 * 100 = 525 = 0x020D, Z: 0
        let payload = vec![0x04, 0x86, 0xFB, 0xE6, 0x02, 0x0D, 0x00, 0x00];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"gyrometer_4": {"x": -10.5, "y": 5.25, "z": 0.0}})
        );
    }

    #[test]
    fn test_gps_negative_coordinates() {
        let decoder = CayenneLppDecoder::new();
        // Channel 2, GPS (type 136)
        // Lat: -10.0° (raw: -100000), Lon: 20.5° (raw: 205000), Alt: -15.25m (raw: -1525)
        // -100000 = 0xFE7960, 205000 = 0x0320C8, -1525 = 0xFFFA0B
        let payload = vec![
            0x02, 0x88, 0xFE, 0x79, 0x60, 0x03, 0x20, 0xC8, 0xFF, 0xFA, 0x0B,
        ];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({"gps_2": {
                "latitude": -10.0,
                "longitude": 20.5,
                "altitude": -15.25
            }})
        );
    }

    #[test]
    fn test_digital_input_zero() {
        let decoder = CayenneLppDecoder::new();
        // Channel 0, Digital Input (type 0), Value 0
        let payload = vec![0x00, 0x00, 0x00];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"digital_input_0": 0}));
    }

    #[test]
    fn test_humidity_zero() {
        let decoder = CayenneLppDecoder::new();
        // Channel 0, Humidity (type 104), Value 0% (raw: 0)
        let payload = vec![0x00, 0x68, 0x00];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"humidity_0": 0.0}));
    }

    #[test]
    fn test_barometer_high_pressure() {
        let decoder = CayenneLppDecoder::new();
        // Channel 1, Barometer (type 115), Value 1084.2 hPa (raw: 10842)
        let payload = vec![0x01, 0x73, 0x2A, 0x5A];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"barometer_1": 1084.2}));
    }

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
        assert_eq!(result, json!({"switch_1": true}));
    }

    #[test]
    fn test_switch_off() {
        let decoder = CayenneLppDecoder::new();
        // Channel 2, Switch (type 142), Value 0 (off)
        let payload = vec![0x02, 0x8E, 0x00];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"switch_2": false}));
    }

    #[test]
    fn test_unix_time() {
        let decoder = CayenneLppDecoder::new();
        // Channel 1, Unix Time (type 133), Value 1699787776 (2023-11-12T11:16:16Z)
        let payload = vec![0x01, 0x85, 0x65, 0x50, 0xB4, 0x00];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"unix_time_1": "2023-11-12T11:16:16+00:00"}));
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
        // Channel 3, Unix Time (type 133), Value 1699866752 (2023-11-13T09:12:32Z)
        let payload = vec![0x03, 0x85, 0x65, 0x51, 0xE8, 0x80];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(result, json!({"unix_time_3": "2023-11-13T09:12:32+00:00"}));
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
        // Bytes 0x066978 = 420216, 0xF2A310 = -875760
        // Base: (42.0216°, -87.576°), Delta: (+0.0001°, +0.0001°)
        // Factor 239 (10000.0): delta nibbles of 1 each
        let payload = vec![
            0x02, 0xF0, // Channel 2, Type 240
            0x09, 0xEF, // Size: 9, Factor: 239 (10000.0)
            0x06, 0x69, 0x78, // Lat: 420216 (42.0216°)
            0xF2, 0xA3, 0x10, // Lon: -875760 (-87.576°)
            0x11, // Delta: lat+1, lon+1 (both nibbles = 0x1)
        ];
        let result = decoder.decode(&payload).unwrap();

        let polyline = result.get("polyline_2").unwrap();
        let points = polyline.get("points").unwrap().as_array().unwrap();
        assert_eq!(points.len(), 2);

        // Verify base point (raw / factor)
        let lat1 = points[0].get("latitude").unwrap().as_f64().unwrap();
        let lon1 = points[0].get("longitude").unwrap().as_f64().unwrap();
        assert!((lat1 - 42.0216).abs() < 0.0001);
        assert!((lon1 - -87.576).abs() < 0.001);

        // Verify second point (base + delta of 1 each)
        let lat2 = points[1].get("latitude").unwrap().as_f64().unwrap();
        let lon2 = points[1].get("longitude").unwrap().as_f64().unwrap();
        assert!((lat2 - 42.0217).abs() < 0.0001); // 420217 / 10000
        assert!((lon2 - -87.575).abs() < 0.001); // -875759 / 10000
    }

    #[test]
    fn test_polyline_negative_deltas() {
        let decoder = CayenneLppDecoder::new();
        // Base point + negative deltas
        // Bytes 0x066978 = 420216, 0xF2A310 = -875760
        // Delta nibbles: 0xF (-1) for both lat and lon
        let payload = vec![
            0x03, 0xF0, // Channel 3, Type 240
            0x09, 0xEF, // Size: 9, Factor: 239 (10000.0)
            0x06, 0x69, 0x78, // Lat: 420216 (42.0216°)
            0xF2, 0xA3, 0x10, // Lon: -875760 (-87.576°)
            0xFF, // Delta: lat-1, lon-1 (both nibbles = 0xF = -1)
        ];
        let result = decoder.decode(&payload).unwrap();

        let polyline = result.get("polyline_3").unwrap();
        let points = polyline.get("points").unwrap().as_array().unwrap();
        assert_eq!(points.len(), 2);

        // Verify second point has negative delta (-1 each)
        let lat2 = points[1].get("latitude").unwrap().as_f64().unwrap();
        let lon2 = points[1].get("longitude").unwrap().as_f64().unwrap();
        assert!((lat2 - 42.0215).abs() < 0.0001); // 420215 / 10000
        assert!((lon2 - -87.577).abs() < 0.001); // -875761 / 10000
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
            0x00, 0x00, 0x00, 0x00, 0x00,
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
            0x06, 0x76, 0x5F, 0xF2, 0x96, 0x0A,
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

    // Integration tests combining standard and extended types

    #[test]
    fn test_extended_multi_sensor() {
        let decoder = CayenneLppDecoder::new();
        // Power monitoring system:
        // Ch 0: Voltage 229.98V, Ch 1: Current 5A, Ch 2: Power 1150W, Ch 3: Energy 123.456 kWh
        let payload = vec![
            0x00, 0x74, 0x59, 0xD6, // Voltage: 22998 = 229.98V
            0x01, 0x75, 0x13, 0x88, // Current: 5000 = 5.0A
            0x02, 0x80, 0x04, 0x7E, // Power: 1150W
            0x03, 0x83, 0x00, 0x01, 0xE2, 0x40, // Energy: 123456 = 123.456 kWh
        ];
        let result = decoder.decode(&payload).unwrap();
        assert_eq!(
            result,
            json!({
                "voltage_0": 229.98,
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
                "switch_0": true,
                "switch_1": false,
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

    #[test]
    fn test_frequency_generic_sensor_combo() {
        let decoder = CayenneLppDecoder::new();
        // Radio frequency monitoring with generic sensor
        let payload = vec![
            0x00, 0x76, 0x00, 0x00, 0x27, 0x10, // Frequency: 10000 Hz
            0x01, 0x64, 0x00, 0x00, 0x03, 0xE8, // Generic Sensor: 1000
            0x02, 0x74, 0x00, 0xC8, // Voltage: 2.0V
        ];
        let result = decoder.decode(&payload).unwrap();

        assert_eq!(result.get("frequency_0"), Some(&json!(10000)));
        assert_eq!(result.get("generic_sensor_1"), Some(&json!(1000)));
        assert_eq!(result.get("voltage_2"), Some(&json!(2.0)));
    }
}

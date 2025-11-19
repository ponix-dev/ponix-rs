use crate::{PayloadDecoder, PayloadError, Result};
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
        let payload = vec![0x01, 0x88, 0x06, 0x76, 0x5F, 0xF2, 0x96, 0x0A, 0x00, 0x03, 0xE8];
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
        let payload = vec![0x02, 0x88, 0xFE, 0x79, 0x60, 0x03, 0x20, 0xC8, 0xFF, 0xFA, 0x0B];
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
}

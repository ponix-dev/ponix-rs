use crate::domain::{ProcessedEnvelope, RawEnvelope};
use anyhow::{anyhow, Result};
use ponix_proto_prost::envelope::v1::ProcessedEnvelope as ProtoProcessedEnvelope;
use ponix_proto_prost::envelope::v1::RawEnvelope as ProtoRawEnvelope;
use prost_types::{value::Kind, Timestamp};

/// Convert protobuf ProcessedEnvelope to domain ProcessedEnvelope
pub fn processed_envelop_proto_to_domain(
    proto: ProtoProcessedEnvelope,
) -> Result<ProcessedEnvelope> {
    // Convert timestamps
    let received_at = timestamp_to_datetime(
        proto
            .received_at
            .ok_or_else(|| anyhow!("Missing received_at timestamp"))?,
    )?;

    let processed_at = timestamp_to_datetime(
        proto
            .processed_at
            .ok_or_else(|| anyhow!("Missing processed_at timestamp"))?,
    )?;

    // Convert protobuf Struct to serde_json::Map
    let data = match proto.data {
        Some(struct_val) => {
            let mut map = serde_json::Map::new();
            for (key, value) in struct_val.fields {
                if let Some(json_value) = prost_value_to_json(&value) {
                    map.insert(key, json_value);
                }
            }
            map
        }
        None => serde_json::Map::new(),
    };

    Ok(ProcessedEnvelope {
        organization_id: proto.organization_id,
        end_device_id: proto.end_device_id,
        received_at,
        processed_at,
        data,
    })
}

/// Convert protobuf Timestamp to chrono DateTime
fn timestamp_to_datetime(ts: Timestamp) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;

    chrono::Utc
        .timestamp_opt(ts.seconds, ts.nanos as u32)
        .single()
        .ok_or_else(|| {
            anyhow!(
                "Invalid timestamp: {} seconds, {} nanos",
                ts.seconds,
                ts.nanos
            )
        })
}

/// Convert protobuf Value to serde_json::Value
fn prost_value_to_json(value: &prost_types::Value) -> Option<serde_json::Value> {
    value.kind.as_ref().map(|kind| match kind {
        Kind::NullValue(_) => serde_json::Value::Null,
        Kind::NumberValue(n) => serde_json::json!(n),
        Kind::StringValue(s) => serde_json::Value::String(s.clone()),
        Kind::BoolValue(b) => serde_json::Value::Bool(*b),
        Kind::StructValue(s) => {
            let mut map = serde_json::Map::new();
            for (k, v) in &s.fields {
                if let Some(json_val) = prost_value_to_json(v) {
                    map.insert(k.clone(), json_val);
                }
            }
            serde_json::Value::Object(map)
        }
        Kind::ListValue(list) => {
            let values: Vec<serde_json::Value> =
                list.values.iter().filter_map(prost_value_to_json).collect();
            serde_json::Value::Array(values)
        }
    })
}

/// Convert protobuf RawEnvelope to domain RawEnvelope
pub fn raw_envelope_proto_to_domain(proto: ProtoRawEnvelope) -> Result<RawEnvelope> {
    let received_at = timestamp_to_datetime(
        proto
            .received_at
            .ok_or_else(|| anyhow!("Missing received_at timestamp"))?,
    )?;

    Ok(RawEnvelope {
        organization_id: proto.organization_id,
        end_device_id: proto.device_id,
        received_at,
        payload: proto.payload.to_vec(),
    })
}

/// Convert domain RawEnvelope to protobuf RawEnvelope
pub fn raw_envelope_domain_to_proto(envelope: &RawEnvelope) -> ProtoRawEnvelope {
    ProtoRawEnvelope {
        organization_id: envelope.organization_id.clone(),
        device_id: envelope.end_device_id.clone(),
        received_at: Some(Timestamp {
            seconds: envelope.received_at.timestamp(),
            nanos: envelope.received_at.timestamp_subsec_nanos() as i32,
        }),
        payload: envelope.payload.clone().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::Struct;
    use std::collections::BTreeMap;

    #[test]
    fn test_proto_to_domain_conversion() {
        let proto = ProtoProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            received_at: Some(Timestamp {
                seconds: 1700000000,
                nanos: 0,
            }),
            processed_at: Some(Timestamp {
                seconds: 1700000010,
                nanos: 0,
            }),
            data: Some(Struct {
                fields: BTreeMap::new(),
            }),
        };

        let result = processed_envelop_proto_to_domain(proto);
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, "org-123");
        assert_eq!(domain.end_device_id, "device-456");
    }

    #[test]
    fn test_missing_timestamp_error() {
        let proto = ProtoProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            received_at: None, // Missing timestamp
            processed_at: Some(Timestamp {
                seconds: 1700000010,
                nanos: 0,
            }),
            data: None,
        };

        let result = processed_envelop_proto_to_domain(proto);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("received_at"));
    }
}

pub fn domain_to_proto_envelope(envelope: &ProcessedEnvelope) -> ProtoProcessedEnvelope {
    use prost_types::Timestamp;

    ProtoProcessedEnvelope {
        organization_id: envelope.organization_id.clone(),
        end_device_id: envelope.end_device_id.clone(),
        received_at: Some(Timestamp {
            seconds: envelope.received_at.timestamp(),
            nanos: envelope.received_at.timestamp_subsec_nanos() as i32,
        }),
        processed_at: Some(Timestamp {
            seconds: envelope.processed_at.timestamp(),
            nanos: envelope.processed_at.timestamp_subsec_nanos() as i32,
        }),
        data: Some(json_map_to_prost_struct(&envelope.data)),
    }
}

fn json_map_to_prost_struct(
    map: &serde_json::Map<String, serde_json::Value>,
) -> prost_types::Struct {
    use prost_types::Struct;

    let fields = map
        .iter()
        .map(|(key, value)| {
            let prost_value = json_to_prost_value(value);
            (key.clone(), prost_value)
        })
        .collect();

    Struct { fields }
}

fn json_to_prost_value(value: &serde_json::Value) -> prost_types::Value {
    use prost_types::{value::Kind, ListValue, Value};

    let kind = match value {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Kind::NumberValue(i as f64)
            } else if let Some(u) = n.as_u64() {
                Kind::NumberValue(u as f64)
            } else if let Some(f) = n.as_f64() {
                Kind::NumberValue(f)
            } else {
                Kind::NullValue(0)
            }
        }
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => {
            let values = arr.iter().map(json_to_prost_value).collect();
            Kind::ListValue(ListValue { values })
        }
        serde_json::Value::Object(map) => Kind::StructValue(json_map_to_prost_struct(map)),
    };

    Value { kind: Some(kind) }
}

#[cfg(test)]
mod domain_conversion_tests {
    use super::*;

    #[test]
    fn test_domain_to_proto_envelope() {
        // Arrange
        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));
        data.insert("humidity".to_string(), serde_json::json!(60));
        data.insert("active".to_string(), serde_json::json!(true));

        let received_at = chrono::DateTime::parse_from_rfc3339("2025-11-20T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let processed_at = chrono::DateTime::parse_from_rfc3339("2025-11-20T10:00:01Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let domain_envelope = ProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            received_at,
            processed_at,
            data,
        };

        // Act
        let proto_envelope = domain_to_proto_envelope(&domain_envelope);

        // Assert
        assert_eq!(proto_envelope.organization_id, "org-123");
        assert_eq!(proto_envelope.end_device_id, "device-456");
        assert!(proto_envelope.received_at.is_some());
        assert!(proto_envelope.processed_at.is_some());
        assert!(proto_envelope.data.is_some());

        let data_struct = proto_envelope.data.unwrap();
        assert_eq!(data_struct.fields.len(), 3);
        assert!(data_struct.fields.contains_key("temperature"));
        assert!(data_struct.fields.contains_key("humidity"));
        assert!(data_struct.fields.contains_key("active"));
    }

    #[test]
    fn test_json_map_to_prost_struct() {
        // Arrange
        let mut map = serde_json::Map::new();
        map.insert("string_field".to_string(), serde_json::json!("test"));
        map.insert("number_field".to_string(), serde_json::json!(42.5));
        map.insert("bool_field".to_string(), serde_json::json!(true));
        map.insert("null_field".to_string(), serde_json::json!(null));

        // Act
        let prost_struct = json_map_to_prost_struct(&map);

        // Assert
        assert_eq!(prost_struct.fields.len(), 4);
    }

    #[test]
    fn test_json_to_prost_value_number() {
        let value = serde_json::json!(42);
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::NumberValue(n)) = prost_value.kind {
            assert_eq!(n, 42.0);
        } else {
            panic!("Expected NumberValue");
        }
    }

    #[test]
    fn test_json_to_prost_value_string() {
        let value = serde_json::json!("hello");
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::StringValue(s)) = prost_value.kind {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected StringValue");
        }
    }

    #[test]
    fn test_json_to_prost_value_array() {
        let value = serde_json::json!([1, 2, 3]);
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::ListValue(list)) = prost_value.kind {
            assert_eq!(list.values.len(), 3);
        } else {
            panic!("Expected ListValue");
        }
    }

    #[test]
    fn test_json_to_prost_value_nested_object() {
        let value = serde_json::json!({
            "nested": {
                "field": "value"
            }
        });
        let prost_value = json_to_prost_value(&value);

        assert!(prost_value.kind.is_some());
        if let Some(prost_types::value::Kind::StructValue(s)) = prost_value.kind {
            assert!(s.fields.contains_key("nested"));
        } else {
            panic!("Expected StructValue");
        }
    }
}

#[cfg(test)]
mod raw_envelope_tests {
    use super::*;

    #[test]
    fn test_proto_to_domain_success() {
        let now = chrono::Utc::now();
        let proto = ProtoRawEnvelope {
            organization_id: "org-123".to_string(),
            device_id: "device-456".to_string(),
            received_at: Some(Timestamp {
                seconds: now.timestamp(),
                nanos: now.timestamp_subsec_nanos() as i32,
            }),
            payload: vec![0x01, 0x02, 0x03].into(),
        };

        let result = raw_envelope_proto_to_domain(proto.clone());
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, "org-123");
        assert_eq!(domain.end_device_id, "device-456");
        assert_eq!(domain.payload, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_proto_to_domain_missing_timestamp() {
        let proto = ProtoRawEnvelope {
            organization_id: "org-123".to_string(),
            device_id: "device-456".to_string(),
            received_at: None,
            payload: vec![0x01, 0x02, 0x03].into(),
        };

        let result = raw_envelope_proto_to_domain(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_domain_to_proto_success() {
        let now = chrono::Utc::now();
        let domain = RawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            received_at: now,
            payload: vec![0x01, 0x02, 0x03],
        };

        let proto = raw_envelope_domain_to_proto(&domain);
        assert_eq!(proto.organization_id, "org-123");
        assert_eq!(proto.device_id, "device-456");
        assert_eq!(proto.payload.to_vec(), vec![0x01, 0x02, 0x03]);
        assert!(proto.received_at.is_some());
    }

    #[test]
    fn test_round_trip_conversion() {
        let now = chrono::Utc::now();
        let original = RawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            received_at: now,
            payload: vec![0x01, 0x02, 0x03],
        };

        let proto = raw_envelope_domain_to_proto(&original);
        let result = raw_envelope_proto_to_domain(proto);
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, original.organization_id);
        assert_eq!(domain.end_device_id, original.end_device_id);
        assert_eq!(domain.payload, original.payload);
        // Note: timestamp precision may differ slightly due to conversion
        assert_eq!(
            domain.received_at.timestamp(),
            original.received_at.timestamp()
        );
    }
}

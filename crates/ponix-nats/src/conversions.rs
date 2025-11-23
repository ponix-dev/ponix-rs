use anyhow::{anyhow, Result};
use ponix_domain::ProcessedEnvelope as DomainEnvelope;
use ponix_proto::envelope::v1::ProcessedEnvelope as ProtoEnvelope;
use prost_types::{value::Kind, Timestamp};

/// Convert protobuf ProcessedEnvelope to domain ProcessedEnvelope
pub fn proto_to_domain_envelope(proto: ProtoEnvelope) -> Result<DomainEnvelope> {
    // Convert timestamps
    let occurred_at = timestamp_to_datetime(
        proto
            .occurred_at
            .ok_or_else(|| anyhow!("Missing occurred_at timestamp"))?,
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

    Ok(DomainEnvelope {
        organization_id: proto.organization_id,
        end_device_id: proto.end_device_id,
        occurred_at,
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

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::Struct;
    use std::collections::BTreeMap;

    #[test]
    fn test_proto_to_domain_conversion() {
        let proto = ProtoEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Some(Timestamp {
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

        let result = proto_to_domain_envelope(proto);
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, "org-123");
        assert_eq!(domain.end_device_id, "device-456");
    }

    #[test]
    fn test_missing_timestamp_error() {
        let proto = ProtoEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: None, // Missing timestamp
            processed_at: Some(Timestamp {
                seconds: 1700000010,
                nanos: 0,
            }),
            data: None,
        };

        let result = proto_to_domain_envelope(proto);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("occurred_at"));
    }
}

/// Convert domain ProcessedEnvelope to protobuf ProcessedEnvelope
#[cfg(feature = "processed-envelope")]
pub fn domain_to_proto_envelope(envelope: &DomainEnvelope) -> ProtoEnvelope {
    use prost_types::Timestamp;

    ProtoEnvelope {
        organization_id: envelope.organization_id.clone(),
        end_device_id: envelope.end_device_id.clone(),
        occurred_at: Some(Timestamp {
            seconds: envelope.occurred_at.timestamp(),
            nanos: envelope.occurred_at.timestamp_subsec_nanos() as i32,
        }),
        processed_at: Some(Timestamp {
            seconds: envelope.processed_at.timestamp(),
            nanos: envelope.processed_at.timestamp_subsec_nanos() as i32,
        }),
        data: Some(json_map_to_prost_struct(&envelope.data)),
    }
}

/// Convert serde_json::Map to prost_types::Struct
#[cfg(feature = "processed-envelope")]
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

/// Convert serde_json::Value to prost_types::Value
#[cfg(feature = "processed-envelope")]
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

#[cfg(all(test, feature = "processed-envelope"))]
mod domain_conversion_tests {
    use super::*;

    #[test]
    fn test_domain_to_proto_envelope() {
        // Arrange
        let mut data = serde_json::Map::new();
        data.insert("temperature".to_string(), serde_json::json!(25.5));
        data.insert("humidity".to_string(), serde_json::json!(60));
        data.insert("active".to_string(), serde_json::json!(true));

        let occurred_at = chrono::DateTime::parse_from_rfc3339("2025-11-20T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let processed_at = chrono::DateTime::parse_from_rfc3339("2025-11-20T10:00:01Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let domain_envelope = DomainEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at,
            processed_at,
            data,
        };

        // Act
        let proto_envelope = domain_to_proto_envelope(&domain_envelope);

        // Assert
        assert_eq!(proto_envelope.organization_id, "org-123");
        assert_eq!(proto_envelope.end_device_id, "device-456");
        assert!(proto_envelope.occurred_at.is_some());
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

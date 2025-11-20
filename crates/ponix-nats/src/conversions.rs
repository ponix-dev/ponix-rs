use anyhow::{anyhow, Result};
use ponix_domain::types::ProcessedEnvelope as DomainEnvelope;
use ponix_proto::envelope::v1::ProcessedEnvelope as ProtoEnvelope;
use prost_types::{Timestamp, value::Kind};
use serde_json;

/// Convert protobuf ProcessedEnvelope to domain ProcessedEnvelope
pub fn proto_to_domain_envelope(proto: ProtoEnvelope) -> Result<DomainEnvelope> {
    // Convert timestamps
    let occurred_at = timestamp_to_datetime(
        proto.occurred_at.ok_or_else(|| anyhow!("Missing occurred_at timestamp"))?
    )?;

    let processed_at = timestamp_to_datetime(
        proto.processed_at.ok_or_else(|| anyhow!("Missing processed_at timestamp"))?
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
        .ok_or_else(|| anyhow!("Invalid timestamp: {} seconds, {} nanos", ts.seconds, ts.nanos))
}

/// Convert protobuf Value to serde_json::Value
fn prost_value_to_json(value: &prost_types::Value) -> Option<serde_json::Value> {
    value.kind.as_ref().and_then(|kind| match kind {
        Kind::NullValue(_) => Some(serde_json::Value::Null),
        Kind::NumberValue(n) => Some(serde_json::json!(n)),
        Kind::StringValue(s) => Some(serde_json::Value::String(s.clone())),
        Kind::BoolValue(b) => Some(serde_json::Value::Bool(*b)),
        Kind::StructValue(s) => {
            let mut map = serde_json::Map::new();
            for (k, v) in &s.fields {
                if let Some(json_val) = prost_value_to_json(v) {
                    map.insert(k.clone(), json_val);
                }
            }
            Some(serde_json::Value::Object(map))
        }
        Kind::ListValue(list) => {
            let values: Vec<serde_json::Value> = list
                .values
                .iter()
                .filter_map(prost_value_to_json)
                .collect();
            Some(serde_json::Value::Array(values))
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

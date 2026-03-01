use crate::domain::Document;
use chrono::{DateTime, Utc};
use ponix_proto_prost::document::v1::Document as ProtoDocument;
use prost_types::{value::Kind, ListValue, Struct, Timestamp, Value};

/// Convert domain Document to protobuf Document
pub fn to_proto_document(doc: Document) -> ProtoDocument {
    ProtoDocument {
        document_id: doc.document_id,
        organization_id: doc.organization_id,
        name: doc.name,
        mime_type: doc.mime_type,
        size_bytes: doc.size_bytes,
        checksum: doc.checksum,
        metadata: json_value_to_prost_struct(&doc.metadata),
        created_at: datetime_to_timestamp(doc.created_at),
        updated_at: datetime_to_timestamp(doc.updated_at),
    }
}

/// Convert a serde_json::Value (expected object) to an optional prost Struct
pub fn json_value_to_prost_struct(value: &serde_json::Value) -> Option<Struct> {
    match value {
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_prost_value(v)))
                .collect();
            Some(Struct { fields })
        }
        serde_json::Value::Null => None,
        _ => None,
    }
}

/// Convert an optional prost Struct to serde_json::Value (defaults to empty object)
pub fn prost_struct_to_json_value(s: Option<Struct>) -> serde_json::Value {
    match s {
        Some(struct_val) => {
            let mut map = serde_json::Map::new();
            for (key, value) in struct_val.fields {
                if let Some(json_val) = prost_value_to_json(&value) {
                    map.insert(key, json_val);
                }
            }
            serde_json::Value::Object(map)
        }
        None => serde_json::Value::Object(serde_json::Map::new()),
    }
}

fn datetime_to_timestamp(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(|d| Timestamp {
        seconds: d.timestamp(),
        nanos: d.timestamp_subsec_nanos() as i32,
    })
}

fn json_to_prost_value(value: &serde_json::Value) -> Value {
    let kind = match value {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => {
            Kind::NumberValue(n.as_f64().unwrap_or(0.0))
        }
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => {
            let values = arr.iter().map(json_to_prost_value).collect();
            Kind::ListValue(ListValue { values })
        }
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_prost_value(v)))
                .collect();
            Kind::StructValue(Struct { fields })
        }
    };
    Value { kind: Some(kind) }
}

fn prost_value_to_json(value: &Value) -> Option<serde_json::Value> {
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
    use chrono::Utc;

    #[test]
    fn test_domain_document_to_proto() {
        let now = Utc::now();
        let doc = Document {
            document_id: "doc-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "test.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 1024,
            object_store_key: "org-456/doc-123/test.pdf".to_string(),
            checksum: "abc123".to_string(),
            metadata: serde_json::json!({"author": "Alice"}),
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_document(doc);

        assert_eq!(proto.document_id, "doc-123");
        assert_eq!(proto.organization_id, "org-456");
        assert_eq!(proto.name, "test.pdf");
        assert_eq!(proto.mime_type, "application/pdf");
        assert_eq!(proto.size_bytes, 1024);
        assert_eq!(proto.checksum, "abc123");
        assert!(proto.metadata.is_some());
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());

        let metadata = proto.metadata.unwrap();
        assert!(metadata.fields.contains_key("author"));
    }

    #[test]
    fn test_json_value_to_prost_struct_object() {
        let value = serde_json::json!({"key": "value", "num": 42});
        let result = json_value_to_prost_struct(&value);
        assert!(result.is_some());
        let s = result.unwrap();
        assert_eq!(s.fields.len(), 2);
    }

    #[test]
    fn test_json_value_to_prost_struct_null() {
        let value = serde_json::Value::Null;
        let result = json_value_to_prost_struct(&value);
        assert!(result.is_none());
    }

    #[test]
    fn test_prost_struct_to_json_value_roundtrip() {
        let value = serde_json::json!({"key": "value"});
        let prost_struct = json_value_to_prost_struct(&value);
        let back = prost_struct_to_json_value(prost_struct);
        assert_eq!(back, serde_json::json!({"key": "value"}));
    }

    #[test]
    fn test_prost_struct_to_json_value_none() {
        let result = prost_struct_to_json_value(None);
        assert_eq!(result, serde_json::json!({}));
    }
}

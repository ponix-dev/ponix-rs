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
        metadata: json_value_to_prost_struct(&doc.metadata),
        content_text: doc.content_text,
        content_html: doc.content_html,
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
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
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

/// Convert protobuf Document to domain Document.
///
/// Used for CDC event parsing. Content fields and Yrs state fields
/// are set from the proto — for CDC payloads these will be empty
/// since the converter intentionally excludes them.
fn proto_to_domain_document(proto: &ProtoDocument) -> Document {
    let metadata = prost_struct_to_json_value(proto.metadata.clone());

    let created_at = proto
        .created_at
        .as_ref()
        .and_then(|ts| DateTime::from_timestamp(ts.seconds, ts.nanos as u32));

    let updated_at = proto
        .updated_at
        .as_ref()
        .and_then(|ts| DateTime::from_timestamp(ts.seconds, ts.nanos as u32));

    Document {
        document_id: proto.document_id.clone(),
        organization_id: proto.organization_id.clone(),
        name: proto.name.clone(),
        yrs_state: vec![],
        yrs_state_vector: vec![],
        content_text: proto.content_text.clone(),
        content_html: proto.content_html.clone(),
        metadata,
        deleted_at: None,
        created_at,
        updated_at,
    }
}

/// Parse a document CDC event from a NATS message subject and payload.
///
/// Decodes the protobuf Document, converts to domain Document,
/// and determines the event type from the subject suffix.
pub fn parse_document_cdc_event(
    subject: &str,
    payload: &[u8],
) -> anyhow::Result<crate::domain::DocumentCdcEvent> {
    use anyhow::anyhow;
    use prost::Message;

    let proto_document =
        ProtoDocument::decode(payload).map_err(|e| anyhow!("Failed to decode protobuf: {}", e))?;

    let document = proto_to_domain_document(&proto_document);

    if subject.ends_with(".create") {
        Ok(crate::domain::DocumentCdcEvent::Created { document })
    } else if subject.ends_with(".update") {
        Ok(crate::domain::DocumentCdcEvent::Updated { document })
    } else if subject.ends_with(".delete") {
        Ok(crate::domain::DocumentCdcEvent::Deleted {
            document_id: document.document_id,
        })
    } else {
        Err(anyhow!("Unknown CDC subject: {}", subject))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_document_to_proto() {
        let now = Utc::now();
        let (yrs_state, yrs_state_vector) = crate::yrs::create_empty_document();
        let doc = Document {
            document_id: "doc-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "test doc".to_string(),
            yrs_state,
            yrs_state_vector,
            content_text: "hello world".to_string(),
            content_html: "<p>hello world</p>".to_string(),
            metadata: serde_json::json!({"author": "Alice"}),
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_document(doc);

        assert_eq!(proto.document_id, "doc-123");
        assert_eq!(proto.organization_id, "org-456");
        assert_eq!(proto.name, "test doc");
        assert_eq!(proto.content_text, "hello world");
        assert_eq!(proto.content_html, "<p>hello world</p>");
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

    fn make_test_proto_document() -> ProtoDocument {
        ProtoDocument {
            document_id: "doc-abc".to_string(),
            organization_id: "org-123".to_string(),
            name: "Test Doc".to_string(),
            metadata: None,
            content_text: String::new(),
            content_html: String::new(),
            created_at: Some(prost_types::Timestamp {
                seconds: 1704067200,
                nanos: 0,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: 1704067200,
                nanos: 0,
            }),
        }
    }

    #[test]
    fn test_parse_document_cdc_event_create() {
        use prost::Message;

        let proto = make_test_proto_document();
        let payload = proto.encode_to_vec();

        let event = parse_document_cdc_event("documents.create", &payload).unwrap();
        match event {
            crate::domain::DocumentCdcEvent::Created { document } => {
                assert_eq!(document.document_id, "doc-abc");
                assert_eq!(document.organization_id, "org-123");
                assert_eq!(document.name, "Test Doc");
            }
            _ => panic!("Expected Created event"),
        }
    }

    #[test]
    fn test_parse_document_cdc_event_update() {
        use prost::Message;

        let proto = make_test_proto_document();
        let payload = proto.encode_to_vec();

        let event = parse_document_cdc_event("documents.update", &payload).unwrap();
        match event {
            crate::domain::DocumentCdcEvent::Updated { document } => {
                assert_eq!(document.document_id, "doc-abc");
            }
            _ => panic!("Expected Updated event"),
        }
    }

    #[test]
    fn test_parse_document_cdc_event_delete() {
        use prost::Message;

        let proto = make_test_proto_document();
        let payload = proto.encode_to_vec();

        let event = parse_document_cdc_event("documents.delete", &payload).unwrap();
        match event {
            crate::domain::DocumentCdcEvent::Deleted { document_id } => {
                assert_eq!(document_id, "doc-abc");
            }
            _ => panic!("Expected Deleted event"),
        }
    }

    #[test]
    fn test_parse_document_cdc_event_unknown_subject() {
        use prost::Message;

        let proto = make_test_proto_document();
        let payload = proto.encode_to_vec();

        let result = parse_document_cdc_event("documents.unknown", &payload);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown CDC subject"));
    }
}

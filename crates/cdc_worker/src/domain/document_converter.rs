use crate::domain::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use common::proto::json_value_to_prost_struct;
use ponix_proto_prost::document::v1::Document;
use prost::Message;
use serde_json::Value;

pub struct DocumentConverter;

impl Default for DocumentConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<Document> {
        let document_id = data
            .get("document_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing document_id"))?
            .to_string();

        let organization_id = data
            .get("organization_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing organization_id"))?
            .to_string();

        let name = data
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // metadata is JSONB — CDC delivers as JSON value
        let metadata = data.get("metadata").and_then(|v| {
            if v.is_null() {
                None
            } else {
                json_value_to_prost_struct(v)
            }
        });

        let created_at = data
            .get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        let updated_at = data
            .get("updated_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        // Intentionally exclude content_text/content_html and yrs_state/yrs_state_vector.
        // CDC payload is lightweight — downstream consumers fetch content from PostgreSQL on demand.
        Ok(Document {
            document_id,
            organization_id,
            name,
            metadata,
            content_text: String::new(),
            content_html: String::new(),
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl CdcConverter for DocumentConverter {
    async fn convert_insert(&self, data: Value) -> anyhow::Result<Bytes> {
        let proto = self.value_to_proto(&data)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }

    async fn convert_update(&self, _old: Value, new: Value) -> anyhow::Result<Bytes> {
        let proto = self.value_to_proto(&new)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }

    async fn convert_delete(&self, data: Value) -> anyhow::Result<Bytes> {
        let proto = self.value_to_proto(&data)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }
}

fn parse_timestamp(s: &str) -> anyhow::Result<prost_types::Timestamp> {
    use chrono::DateTime;

    let dt = DateTime::parse_from_rfc3339(s)?;
    Ok(prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_convert_insert() {
        let converter = DocumentConverter::new();
        let data = json!({
            "document_id": "doc-123",
            "organization_id": "org-1",
            "name": "Test Document",
            "metadata": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        let decoded = Document::decode(bytes).unwrap();
        assert_eq!(decoded.document_id, "doc-123");
        assert_eq!(decoded.organization_id, "org-1");
        assert_eq!(decoded.name, "Test Document");
    }

    #[tokio::test]
    async fn test_convert_insert_with_null_metadata() {
        let converter = DocumentConverter::new();
        let data = json!({
            "document_id": "doc-123",
            "organization_id": "org-1",
            "name": "Test Document",
            "metadata": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let bytes = converter.convert_insert(data).await.unwrap();
        let decoded = Document::decode(bytes).unwrap();
        assert!(decoded.metadata.is_none());
    }

    #[tokio::test]
    async fn test_convert_insert_with_metadata() {
        let converter = DocumentConverter::new();
        let data = json!({
            "document_id": "doc-123",
            "organization_id": "org-1",
            "name": "Test Document",
            "metadata": {"author": "Alice", "version": 2},
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let bytes = converter.convert_insert(data).await.unwrap();
        let decoded = Document::decode(bytes).unwrap();
        assert!(decoded.metadata.is_some());
        let metadata = decoded.metadata.unwrap();
        assert!(metadata.fields.contains_key("author"));
        assert!(metadata.fields.contains_key("version"));
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = DocumentConverter::new();
        let old = json!({
            "document_id": "doc-123",
            "organization_id": "org-1",
            "name": "Old Name"
        });
        let new = json!({
            "document_id": "doc-123",
            "organization_id": "org-1",
            "name": "Updated Document",
            "metadata": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Document::decode(bytes).unwrap();
        assert_eq!(decoded.name, "Updated Document");
    }

    #[tokio::test]
    async fn test_convert_delete() {
        let converter = DocumentConverter::new();
        let data = json!({
            "document_id": "doc-123",
            "organization_id": "org-1",
            "name": "Deleted Document"
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Document::decode(bytes).unwrap();
        assert_eq!(decoded.document_id, "doc-123");
    }

    #[tokio::test]
    async fn test_convert_insert_missing_document_id() {
        let converter = DocumentConverter::new();
        let data = json!({
            "organization_id": "org-1",
            "name": "Test"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("document_id"));
    }

    #[tokio::test]
    async fn test_convert_insert_missing_organization_id() {
        let converter = DocumentConverter::new();
        let data = json!({
            "document_id": "doc-123",
            "name": "Test"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("organization_id"));
    }

    #[tokio::test]
    async fn test_content_fields_excluded() {
        let converter = DocumentConverter::new();
        let data = json!({
            "document_id": "doc-123",
            "organization_id": "org-1",
            "name": "Test Document",
            "content_text": "This is the full document text that should NOT appear in CDC",
            "content_html": "<p>This is HTML content that should NOT appear in CDC</p>",
            "yrs_state": [1, 2, 3],
            "yrs_state_vector": [4, 5, 6],
            "metadata": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let bytes = converter.convert_insert(data).await.unwrap();
        let decoded = Document::decode(bytes).unwrap();
        assert_eq!(decoded.content_text, "");
        assert_eq!(decoded.content_html, "");
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.seconds, 1704067200);
        assert_eq!(ts.nanos, 0);
    }
}

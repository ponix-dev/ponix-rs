use crate::domain::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use ponix_proto_prost::workspace::v1::{Workspace, WorkspaceStatus};
use prost::Message;
use serde_json::Value;

pub struct WorkspaceConverter;

impl Default for WorkspaceConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<Workspace> {
        // Extract required fields
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?
            .to_string();

        let name = data
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?
            .to_string();

        let organization_id = data
            .get("organization_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing organization_id"))?
            .to_string();

        // Extract timestamps if available
        let created_at = data
            .get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        let updated_at = data
            .get("updated_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        let deleted_at = data
            .get("deleted_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        // Determine status based on deleted_at
        let status = if deleted_at.is_some() {
            WorkspaceStatus::Deleted as i32
        } else {
            WorkspaceStatus::Active as i32
        };

        Ok(Workspace {
            id,
            name,
            organization_id,
            status,
            created_at,
            updated_at,
            deleted_at,
        })
    }
}

#[async_trait]
impl CdcConverter for WorkspaceConverter {
    async fn convert_insert(&self, data: Value) -> anyhow::Result<Bytes> {
        let proto = self.value_to_proto(&data)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }

    async fn convert_update(&self, _old: Value, new: Value) -> anyhow::Result<Bytes> {
        // For updates, we only send the new state
        let proto = self.value_to_proto(&new)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }

    async fn convert_delete(&self, data: Value) -> anyhow::Result<Bytes> {
        // For deletes, we send the final state
        let proto = self.value_to_proto(&data)?;
        Ok(Bytes::from(proto.encode_to_vec()))
    }
}

/// Parses an ISO 8601 timestamp string into a protobuf Timestamp
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
        let converter = WorkspaceConverter::new();
        let data = json!({
            "id": "ws-123",
            "name": "Test Workspace",
            "organization_id": "org-1",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        // Verify we can decode the protobuf
        let decoded = Workspace::decode(bytes).unwrap();
        assert_eq!(decoded.id, "ws-123");
        assert_eq!(decoded.name, "Test Workspace");
        assert_eq!(decoded.organization_id, "org-1");
        assert_eq!(decoded.status, WorkspaceStatus::Active as i32);
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = WorkspaceConverter::new();
        let old = json!({
            "id": "ws-123",
            "name": "Old Name",
            "organization_id": "org-1"
        });
        let new = json!({
            "id": "ws-123",
            "name": "Updated Workspace",
            "organization_id": "org-1",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Workspace::decode(bytes).unwrap();
        assert_eq!(decoded.name, "Updated Workspace");
    }

    #[tokio::test]
    async fn test_convert_delete() {
        let converter = WorkspaceConverter::new();
        let data = json!({
            "id": "ws-123",
            "name": "Deleted Workspace",
            "organization_id": "org-1",
            "deleted_at": "2024-01-03T00:00:00Z"
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Workspace::decode(bytes).unwrap();
        assert_eq!(decoded.id, "ws-123");
        assert_eq!(decoded.status, WorkspaceStatus::Deleted as i32);
        assert!(decoded.deleted_at.is_some());
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.seconds, 1704067200);
        assert_eq!(ts.nanos, 0);
    }
}

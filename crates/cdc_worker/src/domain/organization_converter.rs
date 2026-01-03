use crate::domain::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use ponix_proto_prost::organization::v1::Organization;
use prost::Message;
use serde_json::Value;

pub struct OrganizationConverter;

impl Default for OrganizationConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl OrganizationConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<Organization> {
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

        // Status: 0 = active, 1 = deleted
        let status = if deleted_at.is_some() { 1 } else { 0 };

        Ok(Organization {
            id,
            name,
            status,
            deleted_at,
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl CdcConverter for OrganizationConverter {
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
        let converter = OrganizationConverter::new();
        let data = json!({
            "id": "org-123",
            "name": "Test Organization",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        let decoded = Organization::decode(bytes).unwrap();
        assert_eq!(decoded.id, "org-123");
        assert_eq!(decoded.name, "Test Organization");
        assert_eq!(decoded.status, 0); // Active
        assert!(decoded.deleted_at.is_none());
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = OrganizationConverter::new();
        let old = json!({
            "id": "org-123",
            "name": "Old Name"
        });
        let new = json!({
            "id": "org-123",
            "name": "Updated Organization",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Organization::decode(bytes).unwrap();
        assert_eq!(decoded.name, "Updated Organization");
    }

    #[tokio::test]
    async fn test_convert_delete_soft_delete() {
        let converter = OrganizationConverter::new();
        let data = json!({
            "id": "org-123",
            "name": "Deleted Organization",
            "deleted_at": "2024-01-03T00:00:00Z",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-03T00:00:00Z"
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Organization::decode(bytes).unwrap();
        assert_eq!(decoded.id, "org-123");
        assert_eq!(decoded.status, 1); // Deleted
        assert!(decoded.deleted_at.is_some());
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.seconds, 1704067200);
        assert_eq!(ts.nanos, 0);
    }
}

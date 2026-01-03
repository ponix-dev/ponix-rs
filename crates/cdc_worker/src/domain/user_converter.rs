use crate::domain::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use ponix_proto_prost::user::v1::User;
use prost::Message;
use serde_json::Value;

pub struct UserConverter;

impl Default for UserConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl UserConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<User> {
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?
            .to_string();

        let email = data
            .get("email")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing email"))?
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

        // NOTE: password_hash is intentionally NOT included in CDC events for security
        Ok(User {
            id,
            email,
            name,
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl CdcConverter for UserConverter {
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
        let converter = UserConverter::new();
        let data = json!({
            "id": "user-123",
            "email": "test@example.com",
            "password_hash": "argon2hash_should_not_appear",
            "name": "Test User",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        let decoded = User::decode(bytes).unwrap();
        assert_eq!(decoded.id, "user-123");
        assert_eq!(decoded.email, "test@example.com");
        assert_eq!(decoded.name, "Test User");
        // password_hash is intentionally not in proto
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = UserConverter::new();
        let old = json!({
            "id": "user-123",
            "email": "old@example.com",
            "name": "Old Name"
        });
        let new = json!({
            "id": "user-123",
            "email": "new@example.com",
            "name": "Updated User",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = User::decode(bytes).unwrap();
        assert_eq!(decoded.name, "Updated User");
        assert_eq!(decoded.email, "new@example.com");
    }

    #[tokio::test]
    async fn test_convert_delete() {
        let converter = UserConverter::new();
        let data = json!({
            "id": "user-123",
            "email": "deleted@example.com",
            "name": "Deleted User"
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = User::decode(bytes).unwrap();
        assert_eq!(decoded.id, "user-123");
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.seconds, 1704067200);
        assert_eq!(ts.nanos, 0);
    }
}

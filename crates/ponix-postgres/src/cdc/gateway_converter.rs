use crate::cdc::traits::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use ponix_ponix_community_neoeinstein_prost::gateway::v1::{
    gateway::Config, EmqxGatewayConfig, Gateway, GatewayStatus, GatewayType,
};
use prost::Message;
use serde_json::Value;

pub struct GatewayConverter;

impl Default for GatewayConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<Gateway> {
        // Extract required fields
        let gateway_id = data
            .get("gateway_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing gateway_id"))?
            .to_string();

        let organization_id = data
            .get("organization_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing organization_id"))?
            .to_string();

        // Extract gateway_type as string and convert to enum
        let gateway_type_str = data
            .get("gateway_type")
            .and_then(|v| v.as_str())
            .unwrap_or("UNSPECIFIED");

        let r#type = string_to_gateway_type(gateway_type_str) as i32;

        // Extract gateway_config JSON
        let gateway_config = data
            .get("gateway_config")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        // Extract name from config
        let name = gateway_config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Extract broker_url from config and create Config oneof
        let config = gateway_config
            .get("broker_url")
            .and_then(|v| v.as_str())
            .map(|broker_url| {
                Config::EmqxConfig(EmqxGatewayConfig {
                    broker_url: broker_url.to_string(),
                })
            });

        // Default status to ACTIVE
        let status = GatewayStatus::Active as i32;

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

        Ok(Gateway {
            gateway_id,
            organization_id,
            name,
            config,
            status,
            r#type,
            created_at,
            updated_at,
            deleted_at,
        })
    }
}

#[async_trait]
impl CdcConverter for GatewayConverter {
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

/// Converts a string gateway type to the protobuf enum
fn string_to_gateway_type(s: &str) -> GatewayType {
    match s.to_uppercase().as_str() {
        "EMQX" => GatewayType::Emqx,
        _ => GatewayType::Unspecified,
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
        let converter = GatewayConverter::new();
        let data = json!({
            "gateway_id": "test-gateway-1",
            "organization_id": "org-1",
            "gateway_type": "EMQX",
            "gateway_config": {
                "name": "Test Gateway"
            },
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        // Verify we can decode the protobuf
        let decoded = Gateway::decode(bytes).unwrap();
        assert_eq!(decoded.gateway_id, "test-gateway-1");
        assert_eq!(decoded.organization_id, "org-1");
        assert_eq!(decoded.name, "Test Gateway");
        assert_eq!(decoded.r#type, GatewayType::Emqx as i32);
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = GatewayConverter::new();
        let old = json!({
            "gateway_id": "test-gateway-1",
            "organization_id": "org-1",
            "gateway_type": "EMQX",
            "gateway_config": {
                "name": "Old Name"
            }
        });
        let new = json!({
            "gateway_id": "test-gateway-1",
            "organization_id": "org-1",
            "gateway_type": "EMQX",
            "gateway_config": {
                "name": "Updated Gateway"
            },
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Gateway::decode(bytes).unwrap();
        assert_eq!(decoded.name, "Updated Gateway");
    }

    #[tokio::test]
    async fn test_convert_delete() {
        let converter = GatewayConverter::new();
        let data = json!({
            "gateway_id": "test-gateway-1",
            "organization_id": "org-1",
            "gateway_type": "EMQX",
            "gateway_config": {
                "name": "Deleted Gateway"
            },
            "deleted_at": "2024-01-03T00:00:00Z"
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = Gateway::decode(bytes).unwrap();
        assert_eq!(decoded.gateway_id, "test-gateway-1");
        assert!(decoded.deleted_at.is_some());
    }

    #[test]
    fn test_string_to_gateway_type() {
        assert_eq!(string_to_gateway_type("EMQX"), GatewayType::Emqx);
        assert_eq!(string_to_gateway_type("emqx"), GatewayType::Emqx);
        assert_eq!(string_to_gateway_type("unknown"), GatewayType::Unspecified);
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.seconds, 1704067200);
        assert_eq!(ts.nanos, 0);
    }
}

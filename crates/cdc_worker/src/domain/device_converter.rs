use crate::domain::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use ponix_proto_prost::end_device::v1::EndDevice;
use prost::Message;
use serde_json::Value;

pub struct DeviceConverter;

impl Default for DeviceConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<EndDevice> {
        let device_id = data
            .get("device_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing device_id"))?
            .to_string();

        let organization_id = data
            .get("organization_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing organization_id"))?
            .to_string();

        let workspace_id = data
            .get("workspace_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing workspace_id"))?
            .to_string();

        // definition_id is nullable
        let definition_id = data
            .get("definition_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let gateway_id = data
            .get("gateway_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing gateway_id"))?
            .to_string();

        // DB column is device_name, proto field is name
        let name = data
            .get("device_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing device_name"))?
            .to_string();

        let created_at = data
            .get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        let updated_at = data
            .get("updated_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        Ok(EndDevice {
            device_id,
            organization_id,
            workspace_id,
            definition_id,
            gateway_id,
            name,
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl CdcConverter for DeviceConverter {
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
        let converter = DeviceConverter::new();
        let data = json!({
            "device_id": "device-123",
            "organization_id": "org-1",
            "workspace_id": "ws-1",
            "definition_id": "def-1",
            "gateway_id": "gw-1",
            "device_name": "Test Device",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        let decoded = EndDevice::decode(bytes).unwrap();
        assert_eq!(decoded.device_id, "device-123");
        assert_eq!(decoded.organization_id, "org-1");
        assert_eq!(decoded.workspace_id, "ws-1");
        assert_eq!(decoded.definition_id, "def-1");
        assert_eq!(decoded.gateway_id, "gw-1");
        assert_eq!(decoded.name, "Test Device");
    }

    #[tokio::test]
    async fn test_convert_insert_with_null_definition() {
        let converter = DeviceConverter::new();
        let data = json!({
            "device_id": "device-123",
            "organization_id": "org-1",
            "workspace_id": "ws-1",
            "definition_id": null,
            "gateway_id": "gw-1",
            "device_name": "Test Device",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = EndDevice::decode(bytes).unwrap();
        assert_eq!(decoded.definition_id, "");
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = DeviceConverter::new();
        let old = json!({
            "device_id": "device-123",
            "organization_id": "org-1",
            "workspace_id": "ws-1",
            "gateway_id": "gw-1",
            "device_name": "Old Name"
        });
        let new = json!({
            "device_id": "device-123",
            "organization_id": "org-1",
            "workspace_id": "ws-1",
            "gateway_id": "gw-1",
            "device_name": "Updated Device",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = EndDevice::decode(bytes).unwrap();
        assert_eq!(decoded.name, "Updated Device");
    }

    #[tokio::test]
    async fn test_convert_delete() {
        let converter = DeviceConverter::new();
        let data = json!({
            "device_id": "device-123",
            "organization_id": "org-1",
            "workspace_id": "ws-1",
            "gateway_id": "gw-1",
            "device_name": "Deleted Device"
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = EndDevice::decode(bytes).unwrap();
        assert_eq!(decoded.device_id, "device-123");
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.seconds, 1704067200);
        assert_eq!(ts.nanos, 0);
    }
}

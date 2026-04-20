use crate::domain::CdcConverter;
use async_trait::async_trait;
use bytes::Bytes;
use ponix_proto_prost::end_device::v1::{
    EndDeviceDefinition, PayloadContract as ProtoPayloadContract,
};
use prost::Message;
use serde_json::Value;

pub struct EndDeviceDefinitionConverter;

impl Default for EndDeviceDefinitionConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl EndDeviceDefinitionConverter {
    pub fn new() -> Self {
        Self
    }

    fn value_to_proto(&self, data: &Value) -> anyhow::Result<EndDeviceDefinition> {
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?
            .to_string();

        let organization_id = data
            .get("organization_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing organization_id"))?
            .to_string();

        let name = data
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing name"))?
            .to_string();

        // Parse contracts from JSONB column (comes as a JSON string or array in CDC)
        let contracts = match data.get("contracts") {
            Some(Value::Array(arr)) => arr
                .iter()
                .map(value_to_proto_contract)
                .collect::<anyhow::Result<Vec<_>>>()?,
            Some(Value::String(s)) => {
                // CDC may send JSONB as a string that needs parsing
                let arr: Vec<Value> = serde_json::from_str(s)?;
                arr.iter()
                    .map(value_to_proto_contract)
                    .collect::<anyhow::Result<Vec<_>>>()?
            }
            _ => vec![],
        };

        let created_at = data
            .get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        let updated_at = data
            .get("updated_at")
            .and_then(|v| v.as_str())
            .and_then(|s| parse_timestamp(s).ok());

        Ok(EndDeviceDefinition {
            id,
            organization_id,
            name,
            contracts,
            created_at,
            updated_at,
        })
    }
}

#[async_trait]
impl CdcConverter for EndDeviceDefinitionConverter {
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

fn value_to_proto_contract(c: &Value) -> anyhow::Result<ProtoPayloadContract> {
    Ok(ProtoPayloadContract {
        match_expression: c
            .get("match_expression")
            .and_then(|v| v.as_str())
            .unwrap_or("true")
            .to_string(),
        transform_expression: c
            .get("transform_expression")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        json_schema: c
            .get("json_schema")
            .and_then(|v| v.as_str())
            .unwrap_or("{}")
            .to_string(),
    })
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
        let converter = EndDeviceDefinitionConverter::new();
        let data = json!({
            "id": "def-123",
            "organization_id": "org-1",
            "name": "Test Definition",
            "contracts": [
                {
                    "match_expression": "true",
                    "transform_expression": "cayenne_lpp_decode(input)",
                    "json_schema": "{\"type\": \"object\"}"
                }
            ],
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        let decoded = EndDeviceDefinition::decode(bytes).unwrap();
        assert_eq!(decoded.id, "def-123");
        assert_eq!(decoded.organization_id, "org-1");
        assert_eq!(decoded.name, "Test Definition");
        assert_eq!(decoded.contracts.len(), 1);
        assert_eq!(decoded.contracts[0].match_expression, "true");
        assert_eq!(
            decoded.contracts[0].transform_expression,
            "cayenne_lpp_decode(input)"
        );
    }

    #[tokio::test]
    async fn test_convert_insert_with_defaults() {
        let converter = EndDeviceDefinitionConverter::new();
        let data = json!({
            "id": "def-123",
            "organization_id": "org-1",
            "name": "Test Definition",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = EndDeviceDefinition::decode(bytes).unwrap();
        assert!(decoded.contracts.is_empty());
    }

    #[tokio::test]
    async fn test_convert_insert_contracts_as_string() {
        let converter = EndDeviceDefinitionConverter::new();
        let data = json!({
            "id": "def-123",
            "organization_id": "org-1",
            "name": "Test Definition",
            "contracts": "[{\"match_expression\":\"true\",\"transform_expression\":\"cayenne_lpp_decode(input)\",\"json_schema\":\"{}\"}]",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        });

        let result = converter.convert_insert(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = EndDeviceDefinition::decode(bytes).unwrap();
        assert_eq!(decoded.contracts.len(), 1);
        assert_eq!(decoded.contracts[0].match_expression, "true");
    }

    #[tokio::test]
    async fn test_convert_update() {
        let converter = EndDeviceDefinitionConverter::new();
        let old = json!({
            "id": "def-123",
            "organization_id": "org-1",
            "name": "Old Name"
        });
        let new = json!({
            "id": "def-123",
            "organization_id": "org-1",
            "name": "Updated Definition",
            "contracts": [
                {
                    "match_expression": "true",
                    "transform_expression": "updated_conversion()",
                    "json_schema": "{\"type\": \"array\"}"
                }
            ],
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        });

        let result = converter.convert_update(old, new).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = EndDeviceDefinition::decode(bytes).unwrap();
        assert_eq!(decoded.name, "Updated Definition");
        assert_eq!(decoded.contracts.len(), 1);
    }

    #[tokio::test]
    async fn test_convert_delete() {
        let converter = EndDeviceDefinitionConverter::new();
        let data = json!({
            "id": "def-123",
            "organization_id": "org-1",
            "name": "Deleted Definition"
        });

        let result = converter.convert_delete(data).await;
        assert!(result.is_ok());

        let bytes = result.unwrap();
        let decoded = EndDeviceDefinition::decode(bytes).unwrap();
        assert_eq!(decoded.id, "def-123");
    }

    #[test]
    fn test_parse_timestamp() {
        let ts = parse_timestamp("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(ts.seconds, 1704067200);
        assert_eq!(ts.nanos, 0);
    }
}

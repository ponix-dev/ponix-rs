use crate::domain::{Device, EndDeviceDefinition, PayloadContract};
use chrono::{DateTime, Utc};
use ponix_proto_prost::end_device::v1::{
    EndDevice, EndDeviceDefinition as ProtoEndDeviceDefinition,
    PayloadContract as ProtoPayloadContract,
};
use prost_types::Timestamp;

/// Convert chrono DateTime to protobuf Timestamp
fn datetime_to_timestamp(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(|d| Timestamp {
        seconds: d.timestamp(),
        nanos: d.timestamp_subsec_nanos() as i32,
    })
}

/// Convert domain Device to protobuf EndDevice
pub fn to_proto_device(device: Device) -> EndDevice {
    EndDevice {
        device_id: device.device_id,
        organization_id: device.organization_id,
        workspace_id: device.workspace_id,
        definition_id: device.definition_id,
        gateway_id: device.gateway_id,
        name: device.name,
        created_at: datetime_to_timestamp(device.created_at),
        updated_at: datetime_to_timestamp(device.updated_at),
    }
}

/// Convert domain EndDeviceDefinition to protobuf EndDeviceDefinition
pub fn to_proto_end_device_definition(def: EndDeviceDefinition) -> ProtoEndDeviceDefinition {
    ProtoEndDeviceDefinition {
        id: def.id,
        organization_id: def.organization_id,
        name: def.name,
        contracts: def
            .contracts
            .into_iter()
            .map(to_proto_payload_contract)
            .collect(),
        created_at: datetime_to_timestamp(def.created_at),
        updated_at: datetime_to_timestamp(def.updated_at),
    }
}

/// Convert domain PayloadContract to protobuf PayloadContract
pub fn to_proto_payload_contract(contract: PayloadContract) -> ProtoPayloadContract {
    ProtoPayloadContract {
        match_expression: contract.match_expression,
        transform_expression: contract.transform_expression,
        json_schema: contract.json_schema,
    }
}

/// Convert protobuf PayloadContract to domain PayloadContract
pub fn from_proto_payload_contract(proto: ProtoPayloadContract) -> PayloadContract {
    PayloadContract {
        match_expression: proto.match_expression,
        transform_expression: proto.transform_expression,
        json_schema: proto.json_schema,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_device_to_proto() {
        let now = Utc::now();
        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-abc".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-xyz".to_string(),
            name: "Test Device".to_string(),
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_device(device);

        assert_eq!(proto.device_id, "device-123");
        assert_eq!(proto.organization_id, "org-456");
        assert_eq!(proto.workspace_id, "ws-abc");
        assert_eq!(proto.definition_id, "def-789");
        assert_eq!(proto.gateway_id, "gw-xyz");
        assert_eq!(proto.name, "Test Device");
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
    }

    #[test]
    fn test_domain_definition_to_proto() {
        let now = Utc::now();
        let def = EndDeviceDefinition {
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: r#"{"type": "object"}"#.to_string(),
            }],
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_end_device_definition(def);

        assert_eq!(proto.id, "def-123");
        assert_eq!(proto.organization_id, "org-456");
        assert_eq!(proto.name, "Test Definition");
        assert_eq!(proto.contracts.len(), 1);
        assert_eq!(proto.contracts[0].match_expression, "true");
        assert_eq!(
            proto.contracts[0].transform_expression,
            "cayenne_lpp_decode(input)"
        );
        assert_eq!(proto.contracts[0].json_schema, r#"{"type": "object"}"#);
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
    }

    #[test]
    fn test_payload_contract_roundtrip() {
        let contract = PayloadContract {
            match_expression: "size(input) > 0".to_string(),
            transform_expression: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "{}".to_string(),
        };

        let proto = to_proto_payload_contract(contract.clone());
        let back = from_proto_payload_contract(proto);

        assert_eq!(back, contract);
    }
}

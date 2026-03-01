use crate::domain::{DataStream, DataStreamDefinition, PayloadContract};
use chrono::{DateTime, Utc};
use ponix_proto_prost::data_stream::v1::{
    DataStream as ProtoDataStream, DataStreamDefinition as ProtoDataStreamDefinition,
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

/// Convert domain DataStream to protobuf DataStream
pub fn to_proto_data_stream(data_stream: DataStream) -> ProtoDataStream {
    ProtoDataStream {
        data_stream_id: data_stream.data_stream_id,
        organization_id: data_stream.organization_id,
        workspace_id: data_stream.workspace_id,
        definition_id: data_stream.definition_id,
        gateway_id: data_stream.gateway_id,
        name: data_stream.name,
        created_at: datetime_to_timestamp(data_stream.created_at),
        updated_at: datetime_to_timestamp(data_stream.updated_at),
    }
}

/// Convert domain DataStreamDefinition to protobuf DataStreamDefinition
pub fn to_proto_data_stream_definition(def: DataStreamDefinition) -> ProtoDataStreamDefinition {
    ProtoDataStreamDefinition {
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
///
/// Compiled bytes are not stored in proto — they are populated by the service layer at write time.
pub fn from_proto_payload_contract(proto: ProtoPayloadContract) -> PayloadContract {
    PayloadContract {
        match_expression: proto.match_expression,
        transform_expression: proto.transform_expression,
        json_schema: proto.json_schema,
        compiled_match: vec![],
        compiled_transform: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_data_stream_to_proto() {
        let now = Utc::now();
        let data_stream = DataStream {
            data_stream_id: "ds-123".to_string(),
            organization_id: "org-456".to_string(),
            workspace_id: "ws-abc".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: "gw-xyz".to_string(),
            name: "Test Data Stream".to_string(),
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_data_stream(data_stream);

        assert_eq!(proto.data_stream_id, "ds-123");
        assert_eq!(proto.organization_id, "org-456");
        assert_eq!(proto.workspace_id, "ws-abc");
        assert_eq!(proto.definition_id, "def-789");
        assert_eq!(proto.gateway_id, "gw-xyz");
        assert_eq!(proto.name, "Test Data Stream");
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
    }

    #[test]
    fn test_domain_definition_to_proto() {
        let now = Utc::now();
        let def = DataStreamDefinition {
            id: "def-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: r#"{"type": "object"}"#.to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            }],
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_data_stream_definition(def);

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
            compiled_match: vec![],
            compiled_transform: vec![],
        };

        let proto = to_proto_payload_contract(contract.clone());
        let back = from_proto_payload_contract(proto);

        assert_eq!(back, contract);
    }
}

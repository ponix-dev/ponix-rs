use crate::domain::{Gateway, GatewayCdcEvent, MqttCredentials};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use ponix_proto_prost::gateway::v1::{
    Gateway as ProtoGateway, MqttCredentials as ProtoMqttCredentials,
};
use prost::Message;
use prost_types::Timestamp;

/// Convert protobuf Gateway to domain Gateway
pub fn proto_to_domain_gateway(proto: &ProtoGateway) -> Result<Gateway> {
    let created_at = proto
        .created_at
        .as_ref()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32));

    let updated_at = proto
        .updated_at
        .as_ref()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32));

    let deleted_at = proto
        .deleted_at
        .as_ref()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32));

    let credentials = proto.credentials.as_ref().map(proto_to_domain_credentials);

    Ok(Gateway {
        gateway_id: proto.gateway_id.clone(),
        organization_id: proto.organization_id.clone(),
        name: proto.name.clone(),
        broker_url: proto.broker_url.clone(),
        credentials,
        created_at,
        updated_at,
        deleted_at,
    })
}

/// Convert protobuf MqttCredentials to domain MqttCredentials
pub fn proto_to_domain_credentials(proto: &ProtoMqttCredentials) -> MqttCredentials {
    MqttCredentials {
        username: proto.username.clone(),
        password: proto.password.clone(),
    }
}

/// Convert domain MqttCredentials to protobuf MqttCredentials
pub fn domain_to_proto_credentials(creds: &MqttCredentials) -> ProtoMqttCredentials {
    ProtoMqttCredentials {
        username: creds.username.clone(),
        password: creds.password.clone(),
    }
}

/// Convert domain Gateway to protobuf Gateway
pub fn to_proto_gateway(gateway: Gateway) -> ProtoGateway {
    ProtoGateway {
        gateway_id: gateway.gateway_id,
        organization_id: gateway.organization_id,
        name: gateway.name,
        status: 1, // GATEWAY_STATUS_ACTIVE — we don't track status in domain yet
        broker_url: gateway.broker_url,
        credentials: gateway
            .credentials
            .as_ref()
            .map(domain_to_proto_credentials),
        created_at: gateway.created_at.map(datetime_to_timestamp),
        updated_at: gateway.updated_at.map(datetime_to_timestamp),
        deleted_at: gateway.deleted_at.map(datetime_to_timestamp),
    }
}

/// Convert DateTime<Utc> to protobuf Timestamp
fn datetime_to_timestamp(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Parse gateway CDC event from NATS message
pub fn parse_gateway_cdc_event(subject: &str, payload: &[u8]) -> Result<GatewayCdcEvent> {
    let proto_gateway =
        ProtoGateway::decode(payload).map_err(|e| anyhow!("Failed to decode protobuf: {}", e))?;
    let gateway = proto_to_domain_gateway(&proto_gateway)?;

    if subject.ends_with(".create") {
        Ok(GatewayCdcEvent::Created { gateway })
    } else if subject.ends_with(".update") {
        Ok(GatewayCdcEvent::Updated { gateway })
    } else if subject.ends_with(".delete") {
        Ok(GatewayCdcEvent::Deleted {
            gateway_id: gateway.gateway_id.clone(),
        })
    } else {
        Err(anyhow!("Unknown CDC subject: {}", subject))
    }
}

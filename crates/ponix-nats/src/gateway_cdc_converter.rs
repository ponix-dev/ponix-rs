use anyhow::{anyhow, Result};
use ponix_domain::{EmqxGatewayConfig, Gateway, GatewayCdcEvent, GatewayConfig};
use ponix_proto::gateway::v1::{gateway::Config, Gateway as ProtoGateway};
use prost::Message;

/// Convert protobuf Gateway to domain Gateway
pub fn proto_to_domain_gateway(proto: &ProtoGateway) -> Result<Gateway> {
    // Parse timestamps
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

    // Convert config from protobuf enum to domain enum
    let gateway_config = match &proto.config {
        Some(Config::EmqxConfig(emqx)) => GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: emqx.broker_url.clone(),
        }),
        None => {
            // Default to empty EMQX config if not provided
            GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: String::new(),
            })
        }
    };

    // Convert gateway type enum to string
    let gateway_type = match proto.r#type {
        0 => "UNSPECIFIED",
        1 => "emqx",
        2 => "mosquitto",
        _ => "UNSPECIFIED",
    }
    .to_string();

    Ok(Gateway {
        gateway_id: proto.gateway_id.clone(),
        organization_id: proto.organization_id.clone(),
        gateway_type,
        gateway_config,
        created_at,
        updated_at,
        deleted_at,
    })
}

/// Parse gateway CDC event from NATS message
pub fn parse_gateway_cdc_event(subject: &str, payload: &[u8]) -> Result<GatewayCdcEvent> {
    let proto_gateway =
        ProtoGateway::decode(payload).map_err(|e| anyhow!("Failed to decode protobuf: {}", e))?;

    let gateway = proto_to_domain_gateway(&proto_gateway)?;

    // Determine event type from subject
    if subject.ends_with(".create") {
        Ok(GatewayCdcEvent::Created { gateway })
    } else if subject.ends_with(".update") {
        Ok(GatewayCdcEvent::Updated {
            gateway,
        })
    } else if subject.ends_with(".delete") {
        Ok(GatewayCdcEvent::Deleted {
            gateway_id: gateway.gateway_id.clone(),
        })
    } else {
        Err(anyhow!("Unknown CDC subject: {}", subject))
    }
}

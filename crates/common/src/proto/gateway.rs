use crate::domain::{
    CreateGatewayInput, DeleteGatewayInput, EmqxGatewayConfig, Gateway, GatewayCdcEvent,
    GatewayConfig, GetGatewayInput, ListGatewaysInput, UpdateGatewayInput,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use ponix_proto_prost::gateway::v1::gateway::Config;
use ponix_proto_prost::gateway::v1::{
    create_gateway_request::Config as CreateConfig, gateway::Config as ProtoGatewayConfig,
    update_gateway_request::Config as UpdateConfig, CreateGatewayRequest, DeleteGatewayRequest,
    EmqxGatewayConfig as ProtoEmqxConfig, Gateway as ProtoGateway, GatewayType, GetGatewayRequest,
    ListGatewaysRequest, UpdateGatewayRequest,
};
use prost::Message;
use prost_types::Timestamp;

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
            subscription_group: emqx.subscription_group.clone(),
        }),
        None => {
            // Default to empty EMQX config if not provided
            GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: String::new(),
                subscription_group: String::new(),
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

pub fn to_create_gateway_input(request: CreateGatewayRequest) -> CreateGatewayInput {
    let gateway_type = gateway_type_to_string(request.r#type());

    // Convert config from protobuf enum to domain enum
    let gateway_config = match request.config {
        Some(CreateConfig::EmqxConfig(emqx)) => GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: emqx.broker_url,
            subscription_group: emqx.subscription_group,
        }),
        None => {
            // Default to empty EMQX config if not provided
            GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: String::new(),
                subscription_group: String::new(),
            })
        }
    };

    CreateGatewayInput {
        organization_id: request.organization_id,
        gateway_type,
        gateway_config,
    }
}

/// Convert GatewayType enum to string
fn gateway_type_to_string(gateway_type: GatewayType) -> String {
    match gateway_type {
        GatewayType::Unspecified => "unspecified".to_string(),
        GatewayType::Emqx => "emqx".to_string(),
    }
}

/// Convert string to GatewayType enum
fn string_to_gateway_type(s: &str) -> GatewayType {
    match s.to_lowercase().as_str() {
        "emqx" => GatewayType::Emqx,
        _ => GatewayType::Unspecified,
    }
}

/// Convert GetGatewayRequest to domain GetGatewayInput
pub fn to_get_gateway_input(request: GetGatewayRequest) -> GetGatewayInput {
    GetGatewayInput {
        gateway_id: request.gateway_id,
        organization_id: request.organization_id,
    }
}

/// Convert ListGatewaysRequest to domain ListGatewaysInput
pub fn to_list_gateways_input(request: ListGatewaysRequest) -> ListGatewaysInput {
    ListGatewaysInput {
        organization_id: request.organization_id,
    }
}

/// Convert UpdateGatewayRequest to domain UpdateGatewayInput
pub fn to_update_gateway_input(request: UpdateGatewayRequest) -> UpdateGatewayInput {
    let gateway_type = if request.r#type != 0 {
        Some(gateway_type_to_string(
            GatewayType::try_from(request.r#type).unwrap_or(GatewayType::Unspecified),
        ))
    } else {
        None
    };

    // Convert config from protobuf enum to domain enum
    let gateway_config = match request.config {
        Some(UpdateConfig::EmqxConfig(emqx)) => Some(GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: emqx.broker_url,
            subscription_group: emqx.subscription_group,
        })),
        None => None,
    };

    UpdateGatewayInput {
        gateway_id: request.gateway_id,
        organization_id: request.organization_id,
        gateway_type,
        gateway_config,
    }
}

/// Convert DeleteGatewayRequest to domain DeleteGatewayInput
pub fn to_delete_gateway_input(request: DeleteGatewayRequest) -> DeleteGatewayInput {
    DeleteGatewayInput {
        gateway_id: request.gateway_id,
        organization_id: request.organization_id,
    }
}

/// Convert domain Gateway to protobuf Gateway
pub fn to_proto_gateway(gateway: Gateway) -> ProtoGateway {
    // Convert domain config enum to protobuf config enum and extract name
    let (config, name) = match &gateway.gateway_config {
        GatewayConfig::Emqx(emqx) => (
            Some(ProtoGatewayConfig::EmqxConfig(ProtoEmqxConfig {
                broker_url: emqx.broker_url.clone(),
                subscription_group: emqx.subscription_group.clone(),
            })),
            emqx.broker_url.clone(), // For backwards compatibility, use broker_url as name
        ),
    };

    ProtoGateway {
        gateway_id: gateway.gateway_id,
        organization_id: gateway.organization_id,
        name,
        config,
        status: 1, // GATEWAY_STATUS_ACTIVE - we don't track status in domain yet
        r#type: string_to_gateway_type(&gateway.gateway_type) as i32,
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

    // Determine event type from subject
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

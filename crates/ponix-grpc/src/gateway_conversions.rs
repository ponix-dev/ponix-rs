use chrono::{DateTime, TimeZone, Utc};
use ponix_domain::{
    gateway::{
        CreateGatewayInput, DeleteGatewayInput, GetGatewayInput, ListGatewaysInput,
        UpdateGatewayInput,
    },
    EmqxGatewayConfig as DomainEmqxConfig, Gateway, GatewayConfig as DomainGatewayConfig,
};
use ponix_proto_prost::gateway::v1::{
    create_gateway_request::Config as CreateConfig, gateway::Config as ProtoGatewayConfig,
    update_gateway_request::Config as UpdateConfig, CreateGatewayRequest, DeleteGatewayRequest,
    EmqxGatewayConfig as ProtoEmqxConfig, Gateway as ProtoGateway, GatewayType, GetGatewayRequest,
    ListGatewaysRequest, UpdateGatewayRequest,
};
use prost_types::Timestamp;

/// Convert CreateGatewayRequest to domain CreateGatewayInput
pub fn to_create_gateway_input(request: CreateGatewayRequest) -> CreateGatewayInput {
    let gateway_type = gateway_type_to_string(request.r#type());

    // Convert config from protobuf enum to domain enum
    let gateway_config = match request.config {
        Some(CreateConfig::EmqxConfig(emqx)) => {
            DomainGatewayConfig::Emqx(DomainEmqxConfig {
                broker_url: emqx.broker_url,
            })
        }
        None => {
            // Default to empty EMQX config if not provided
            DomainGatewayConfig::Emqx(DomainEmqxConfig {
                broker_url: String::new(),
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
        Some(UpdateConfig::EmqxConfig(emqx)) => Some(DomainGatewayConfig::Emqx(
            DomainEmqxConfig {
                broker_url: emqx.broker_url,
            },
        )),
        None => None,
    };

    UpdateGatewayInput {
        gateway_id: request.gateway_id,
        gateway_type,
        gateway_config,
    }
}

/// Convert DeleteGatewayRequest to domain DeleteGatewayInput
pub fn to_delete_gateway_input(request: DeleteGatewayRequest) -> DeleteGatewayInput {
    DeleteGatewayInput {
        gateway_id: request.gateway_id,
    }
}

/// Convert domain Gateway to protobuf Gateway
pub fn to_proto_gateway(gateway: Gateway) -> ProtoGateway {
    // Convert domain config enum to protobuf config enum and extract name
    let (config, name) = match &gateway.gateway_config {
        DomainGatewayConfig::Emqx(emqx) => (
            Some(ProtoGatewayConfig::EmqxConfig(ProtoEmqxConfig {
                broker_url: emqx.broker_url.clone(),
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

/// Convert protobuf Timestamp to DateTime<Utc>
#[allow(dead_code)]
fn timestamp_to_datetime(ts: Timestamp) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.seconds, ts.nanos as u32)
        .single()
        .unwrap_or_else(Utc::now)
}

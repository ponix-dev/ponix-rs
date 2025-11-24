use crate::models::{Device as DbDevice, DeviceRow, GatewayRow};
use ponix_domain::{
    CreateDeviceInputWithId, Device as DomainDevice, EmqxGatewayConfig, Gateway, GatewayConfig,
};

/// Convert domain CreateDeviceInputWithId to database Device (for insert)
impl From<&CreateDeviceInputWithId> for DbDevice {
    fn from(input: &CreateDeviceInputWithId) -> Self {
        DbDevice {
            device_id: input.device_id.clone(),
            organization_id: input.organization_id.clone(),
            device_name: input.name.clone(), // Map name -> device_name
            payload_conversion: input.payload_conversion.clone(),
        }
    }
}

/// Convert database DeviceRow to domain Device
impl From<DeviceRow> for DomainDevice {
    fn from(row: DeviceRow) -> Self {
        DomainDevice {
            device_id: row.device_id,
            organization_id: row.organization_id,
            name: row.device_name, // Map device_name -> name
            payload_conversion: row.payload_conversion,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// Convert database Device (without timestamps) to domain Device
impl From<DbDevice> for DomainDevice {
    fn from(device: DbDevice) -> Self {
        DomainDevice {
            device_id: device.device_id,
            organization_id: device.organization_id,
            name: device.device_name, // Map device_name -> name
            payload_conversion: device.payload_conversion,
            created_at: None,
            updated_at: None,
        }
    }
}

/// Convert database GatewayRow to domain Gateway
impl From<GatewayRow> for Gateway {
    fn from(row: GatewayRow) -> Self {
        // Convert JSON config to domain GatewayConfig enum
        let gateway_config = json_to_gateway_config(&row.gateway_config);

        Gateway {
            gateway_id: row.gateway_id,
            organization_id: row.organization_id,
            gateway_type: row.gateway_type,
            gateway_config,
            deleted_at: row.deleted_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// Convert serde_json::Value to domain GatewayConfig
fn json_to_gateway_config(json: &serde_json::Value) -> GatewayConfig {
    if let Some(broker_url) = json.get("broker_url").and_then(|v| v.as_str()) {
        GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: broker_url.to_string(),
        })
    } else {
        // Default to empty EMQX config if broker_url not found
        GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: String::new(),
        })
    }
}

/// Convert domain GatewayConfig to serde_json::Value
pub fn gateway_config_to_json(config: &GatewayConfig) -> serde_json::Value {
    match config {
        GatewayConfig::Emqx(emqx) => {
            serde_json::json!({
                "broker_url": emqx.broker_url
            })
        }
    }
}

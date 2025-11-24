use crate::gateway_config::GatewayConfig;
use chrono::{DateTime, Utc};

/// Gateway entity representing a configured gateway for an organization
#[derive(Debug, Clone, PartialEq)]
pub struct Gateway {
    pub gateway_id: String,
    pub organization_id: String,
    pub gateway_type: String,
    pub gateway_config: GatewayConfig,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// External input for creating a gateway (no ID)
#[derive(Debug, Clone, PartialEq)]
pub struct CreateGatewayInput {
    pub organization_id: String,
    pub gateway_type: String,
    pub gateway_config: GatewayConfig,
}

/// Internal input with generated ID
#[derive(Debug, Clone, PartialEq)]
pub struct CreateGatewayInputWithId {
    pub gateway_id: String,
    pub organization_id: String,
    pub gateway_type: String,
    pub gateway_config: GatewayConfig,
}

/// Input for getting a gateway by ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGatewayInput {
    pub gateway_id: String,
}

/// Input for updating a gateway
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateGatewayInput {
    pub gateway_id: String,
    pub gateway_type: Option<String>,
    pub gateway_config: Option<GatewayConfig>,
}

/// Input for deleting a gateway
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteGatewayInput {
    pub gateway_id: String,
}

/// Input for listing gateways by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListGatewaysInput {
    pub organization_id: String,
}

use crate::domain::result::DomainResult;
use async_trait::async_trait;
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

/// Gateway configuration types
/// Mirrors the protobuf Gateway config structure but as a domain type
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayConfig {
    /// EMQX gateway configuration
    Emqx(EmqxGatewayConfig),
}

/// EMQX gateway configuration
#[derive(Debug, Clone, PartialEq)]
pub struct EmqxGatewayConfig {
    pub broker_url: String,
}

/// Repository trait for gateway persistence operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait GatewayRepository: Send + Sync {
    /// Create a new gateway
    async fn create_gateway(&self, input: CreateGatewayInputWithId) -> DomainResult<Gateway>;

    /// Get a gateway by ID (excludes soft deleted)
    async fn get_gateway(&self, gateway_id: &str) -> DomainResult<Option<Gateway>>;

    /// Update a gateway
    async fn update_gateway(&self, input: UpdateGatewayInput) -> DomainResult<Gateway>;

    /// Soft delete a gateway
    async fn delete_gateway(&self, gateway_id: &str) -> DomainResult<()>;

    /// List gateways by organization (excludes soft deleted)
    async fn list_gateways(&self, organization_id: &str) -> DomainResult<Vec<Gateway>>;

    /// List all non-deleted gateways across all organizations
    async fn list_all_gateways(&self) -> DomainResult<Vec<Gateway>>;
}

/// Domain representation of a gateway CDC event
#[derive(Debug, Clone)]
pub enum GatewayCdcEvent {
    Created { gateway: Gateway },
    Updated { gateway: Gateway },
    Deleted { gateway_id: String },
}

impl GatewayCdcEvent {
    pub fn gateway_id(&self) -> &str {
        match self {
            GatewayCdcEvent::Created { gateway } => &gateway.gateway_id,
            GatewayCdcEvent::Updated { gateway, .. } => &gateway.gateway_id,
            GatewayCdcEvent::Deleted { gateway_id } => gateway_id,
        }
    }
}

use crate::domain::result::DomainResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use garde::Validate;

/// Gateway entity representing a configured gateway for an organization
#[derive(Debug, Clone, PartialEq)]
pub struct Gateway {
    pub gateway_id: String,
    pub organization_id: String,
    pub name: String,
    pub gateway_type: String,
    pub gateway_config: GatewayConfig,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Repository input for creating a gateway (domain service -> repository)
#[derive(Debug, Clone, PartialEq)]
pub struct CreateGatewayRepoInput {
    pub gateway_id: String,
    pub organization_id: String,
    pub name: String,
    pub gateway_type: String,
    pub gateway_config: GatewayConfig,
}

/// Repository input for getting a gateway by ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGatewayRepoInput {
    pub gateway_id: String,
    pub organization_id: String,
}

/// Repository input for updating a gateway
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateGatewayRepoInput {
    pub gateway_id: String,
    pub organization_id: String,
    pub name: Option<String>,
    pub gateway_type: Option<String>,
    pub gateway_config: Option<GatewayConfig>,
}

/// Repository input for deleting a gateway
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteGatewayRepoInput {
    pub gateway_id: String,
    pub organization_id: String,
}

/// Repository input for listing gateways by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListGatewaysRepoInput {
    pub organization_id: String,
}

/// Gateway configuration types
/// Mirrors the protobuf Gateway config structure but as a domain type
#[derive(Debug, Clone, PartialEq, Validate)]
pub enum GatewayConfig {
    /// EMQX gateway configuration
    Emqx(#[garde(dive)] EmqxGatewayConfig),
}

/// EMQX gateway configuration
#[derive(Debug, Clone, PartialEq, Validate)]
pub struct EmqxGatewayConfig {
    #[garde(length(min = 1))]
    pub broker_url: String,
}

/// Repository trait for gateway persistence operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait GatewayRepository: Send + Sync {
    /// Create a new gateway
    async fn create_gateway(&self, input: CreateGatewayRepoInput) -> DomainResult<Gateway>;

    /// Get a gateway by ID and organization (excludes soft deleted)
    async fn get_gateway(&self, input: GetGatewayRepoInput) -> DomainResult<Option<Gateway>>;

    /// Update a gateway
    async fn update_gateway(&self, input: UpdateGatewayRepoInput) -> DomainResult<Gateway>;

    /// Soft delete a gateway
    async fn delete_gateway(&self, input: DeleteGatewayRepoInput) -> DomainResult<()>;

    /// List gateways by organization (excludes soft deleted)
    async fn list_gateways(&self, input: ListGatewaysRepoInput) -> DomainResult<Vec<Gateway>>;

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

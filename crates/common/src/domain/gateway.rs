use crate::domain::result::DomainResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use garde::Validate;

/// MQTT credentials for connecting to a gateway's broker.
///
/// Stored in plaintext for now; encryption-at-rest is tracked in #208.
#[derive(Debug, Clone, PartialEq, Eq, Validate)]
pub struct MqttCredentials {
    #[garde(length(min = 1))]
    pub username: String,
    #[garde(length(min = 1))]
    pub password: String,
}

/// Gateway entity representing a LoRaWAN network server MQTT connection.
#[derive(Debug, Clone, PartialEq)]
pub struct Gateway {
    pub gateway_id: String,
    pub organization_id: String,
    pub name: String,
    pub broker_url: String,
    pub credentials: Option<MqttCredentials>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Repository input for creating a gateway (domain service -> repository)
#[derive(Debug, Clone, PartialEq, Validate)]
pub struct CreateGatewayRepoInput {
    #[garde(skip)]
    pub gateway_id: String,
    #[garde(skip)]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub name: String,
    #[garde(length(min = 1))]
    pub broker_url: String,
    #[garde(dive)]
    pub credentials: Option<MqttCredentials>,
}

/// Repository input for getting a gateway by ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGatewayRepoInput {
    pub gateway_id: String,
    pub organization_id: String,
}

/// Repository input for updating a gateway
#[derive(Debug, Clone, PartialEq, Validate)]
pub struct UpdateGatewayRepoInput {
    #[garde(skip)]
    pub gateway_id: String,
    #[garde(skip)]
    pub organization_id: String,
    #[garde(inner(length(min = 1)))]
    pub name: Option<String>,
    #[garde(inner(length(min = 1)))]
    pub broker_url: Option<String>,
    #[garde(dive)]
    pub credentials: Option<MqttCredentials>,
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

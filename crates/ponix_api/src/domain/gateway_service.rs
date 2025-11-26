use common::domain::{
    CreateGatewayInput, CreateGatewayInputWithId, DeleteGatewayInput, DomainError, DomainResult,
    Gateway, GatewayRepository, GetGatewayInput, GetOrganizationInput, ListGatewaysInput,
    OrganizationRepository, UpdateGatewayInput,
};
use std::sync::Arc;
use tracing::{debug, info};

/// Service for gateway business logic
pub struct GatewayService {
    gateway_repository: Arc<dyn GatewayRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
}

impl GatewayService {
    pub fn new(
        gateway_repository: Arc<dyn GatewayRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
    ) -> Self {
        Self {
            gateway_repository,
            organization_repository,
        }
    }

    /// Create a new gateway for an organization
    pub async fn create_gateway(&self, input: CreateGatewayInput) -> DomainResult<Gateway> {
        debug!(organization_id = %input.organization_id, gateway_type = %input.gateway_type, "Creating gateway");

        // Validate organization exists and is not deleted
        let org_input = GetOrganizationInput {
            organization_id: input.organization_id.clone(),
        };

        match self
            .organization_repository
            .get_organization(org_input)
            .await?
        {
            Some(org) => {
                if org.deleted_at.is_some() {
                    return Err(DomainError::OrganizationDeleted(format!(
                        "Cannot create gateway for deleted organization: {}",
                        input.organization_id
                    )));
                }
            }
            None => {
                return Err(DomainError::OrganizationNotFound(format!(
                    "Organization not found: {}",
                    input.organization_id
                )));
            }
        }

        // Validate gateway_type is not empty
        if input.gateway_type.trim().is_empty() {
            return Err(DomainError::InvalidGatewayType(
                "Gateway type cannot be empty".to_string(),
            ));
        }

        // Generate gateway ID
        let gateway_id = xid::new().to_string();

        let repo_input = CreateGatewayInputWithId {
            gateway_id: gateway_id.clone(),
            organization_id: input.organization_id,
            gateway_type: input.gateway_type,
            gateway_config: input.gateway_config,
        };

        let gateway = self.gateway_repository.create_gateway(repo_input).await?;

        info!(gateway_id = %gateway.gateway_id, "Gateway created successfully");
        Ok(gateway)
    }

    /// Get a gateway by ID
    pub async fn get_gateway(&self, input: GetGatewayInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "Getting gateway");

        if input.gateway_id.is_empty() {
            return Err(DomainError::InvalidGatewayId(
                "Gateway ID cannot be empty".to_string(),
            ));
        }

        let gateway = self
            .gateway_repository
            .get_gateway(&input.gateway_id)
            .await?
            .ok_or_else(|| DomainError::GatewayNotFound(input.gateway_id.clone()))?;

        Ok(gateway)
    }

    /// Update a gateway
    pub async fn update_gateway(&self, input: UpdateGatewayInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "Updating gateway");

        if input.gateway_id.is_empty() {
            return Err(DomainError::InvalidGatewayId(
                "Gateway ID cannot be empty".to_string(),
            ));
        }

        // Validate gateway_type if provided
        if let Some(ref gateway_type) = input.gateway_type {
            if gateway_type.trim().is_empty() {
                return Err(DomainError::InvalidGatewayType(
                    "Gateway type cannot be empty".to_string(),
                ));
            }
        }

        let gateway = self.gateway_repository.update_gateway(input).await?;

        info!(gateway_id = %gateway.gateway_id, "Gateway updated successfully");
        Ok(gateway)
    }

    /// Soft delete a gateway
    pub async fn delete_gateway(&self, input: DeleteGatewayInput) -> DomainResult<()> {
        debug!(gateway_id = %input.gateway_id, "Deleting gateway");

        if input.gateway_id.is_empty() {
            return Err(DomainError::InvalidGatewayId(
                "Gateway ID cannot be empty".to_string(),
            ));
        }

        self.gateway_repository
            .delete_gateway(&input.gateway_id)
            .await?;

        info!("Gateway soft deleted successfully");
        Ok(())
    }

    /// List gateways by organization
    pub async fn list_gateways(&self, input: ListGatewaysInput) -> DomainResult<Vec<Gateway>> {
        debug!(organization_id = %input.organization_id, "Listing gateways");

        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId(
                "Organization ID cannot be empty".to_string(),
            ));
        }

        let gateways = self
            .gateway_repository
            .list_gateways(&input.organization_id)
            .await?;

        info!(count = gateways.len(), "Listed gateways");
        Ok(gateways)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use common::domain::{
        EmqxGatewayConfig, GatewayConfig, MockGatewayRepository, MockOrganizationRepository,
        Organization,
    };

    #[tokio::test]
    async fn test_create_gateway_success() {
        let mut mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        let test_config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
        });

        mock_org_repo
            .expect_get_organization()
            .withf(|input| input.organization_id == "org-001")
            .times(1)
            .return_once(|_| {
                Ok(Some(Organization {
                    id: "org-001".to_string(),
                    name: "Test Org".to_string(),
                    deleted_at: None,
                    created_at: Some(Utc::now()),
                    updated_at: Some(Utc::now()),
                }))
            });

        mock_gateway_repo
            .expect_create_gateway()
            .withf(|input| {
                input.organization_id == "org-001"
                    && input.gateway_type == "emqx"
                    && !input.gateway_id.is_empty()
            })
            .times(1)
            .return_once(|input| {
                Ok(Gateway {
                    gateway_id: input.gateway_id,
                    organization_id: input.organization_id,
                    gateway_type: input.gateway_type,
                    gateway_config: input.gateway_config,
                    deleted_at: None,
                    created_at: Some(Utc::now()),
                    updated_at: Some(Utc::now()),
                })
            });

        let service = GatewayService::new(Arc::new(mock_gateway_repo), Arc::new(mock_org_repo));

        let input = CreateGatewayInput {
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: test_config,
        };

        let result = service.create_gateway(input).await;
        assert!(result.is_ok());
        let gateway = result.unwrap();
        assert_eq!(gateway.organization_id, "org-001");
        assert_eq!(gateway.gateway_type, "emqx");
    }

    #[tokio::test]
    async fn test_create_gateway_org_not_found() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = GatewayService::new(Arc::new(mock_gateway_repo), Arc::new(mock_org_repo));

        let input = CreateGatewayInput {
            organization_id: "org-999".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: String::new(),
            }),
        };

        let result = service.create_gateway(input).await;
        assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
    }

    #[tokio::test]
    async fn test_create_gateway_org_deleted() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| {
                Ok(Some(Organization {
                    id: "org-001".to_string(),
                    name: "Test Org".to_string(),
                    deleted_at: Some(Utc::now()),
                    created_at: Some(Utc::now()),
                    updated_at: Some(Utc::now()),
                }))
            });

        let service = GatewayService::new(Arc::new(mock_gateway_repo), Arc::new(mock_org_repo));

        let input = CreateGatewayInput {
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: String::new(),
            }),
        };

        let result = service.create_gateway(input).await;
        assert!(matches!(result, Err(DomainError::OrganizationDeleted(_))));
    }
}

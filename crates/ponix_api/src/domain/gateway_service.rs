use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateGatewayRepoInput, DeleteGatewayRepoInput, DomainError, DomainResult, Gateway,
    GatewayConfig, GatewayRepository, GetGatewayRepoInput, GetOrganizationRepoInput,
    ListGatewaysRepoInput, OrganizationRepository, UpdateGatewayRepoInput,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Service request for creating a gateway
#[derive(Debug, Clone, Validate)]
pub struct CreateGatewayRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub gateway_type: String,
    #[garde(dive)]
    pub gateway_config: GatewayConfig,
}

/// Service request for getting a gateway
#[derive(Debug, Clone, Validate)]
pub struct GetGatewayRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub gateway_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for updating a gateway
#[derive(Debug, Clone, Validate)]
pub struct UpdateGatewayRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub gateway_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(inner(length(min = 1)))]
    pub gateway_type: Option<String>,
    #[garde(dive)]
    pub gateway_config: Option<GatewayConfig>,
}

/// Service request for deleting a gateway
#[derive(Debug, Clone, Validate)]
pub struct DeleteGatewayRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub gateway_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for listing gateways
#[derive(Debug, Clone, Validate)]
pub struct ListGatewaysRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service for gateway business logic
pub struct GatewayService {
    gateway_repository: Arc<dyn GatewayRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl GatewayService {
    pub fn new(
        gateway_repository: Arc<dyn GatewayRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            gateway_repository,
            organization_repository,
            authorization_provider,
        }
    }

    /// Create a new gateway for an organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, gateway_type = %request.gateway_type))]
    pub async fn create_gateway(&self, request: CreateGatewayRequest) -> DomainResult<Gateway> {
        // Validate request using garde
        common::validation::validate(&request)?;

        debug!(organization_id = %request.organization_id, gateway_type = %request.gateway_type, "Creating gateway");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Gateway,
                Action::Create,
            )
            .await?;

        // Validate organization exists and is not deleted
        let org_input = GetOrganizationRepoInput {
            organization_id: request.organization_id.clone(),
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
                        request.organization_id
                    )));
                }
            }
            None => {
                return Err(DomainError::OrganizationNotFound(format!(
                    "Organization not found: {}",
                    request.organization_id
                )));
            }
        }

        // Generate gateway ID
        let gateway_id = xid::new().to_string();

        let repo_input = CreateGatewayRepoInput {
            gateway_id: gateway_id.clone(),
            organization_id: request.organization_id,
            gateway_type: request.gateway_type,
            gateway_config: request.gateway_config,
        };

        let gateway = self.gateway_repository.create_gateway(repo_input).await?;

        debug!(gateway_id = %gateway.gateway_id, "Gateway created successfully");
        Ok(gateway)
    }

    /// Get a gateway by ID and organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, gateway_id = %request.gateway_id, organization_id = %request.organization_id))]
    pub async fn get_gateway(&self, request: GetGatewayRequest) -> DomainResult<Gateway> {
        // Validate request using garde
        common::validation::validate(&request)?;

        debug!(gateway_id = %request.gateway_id, organization_id = %request.organization_id, "Getting gateway");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Gateway,
                Action::Read,
            )
            .await?;

        let repo_input = GetGatewayRepoInput {
            gateway_id: request.gateway_id.clone(),
            organization_id: request.organization_id,
        };

        let gateway = self
            .gateway_repository
            .get_gateway(repo_input)
            .await?
            .ok_or_else(|| DomainError::GatewayNotFound(request.gateway_id))?;

        Ok(gateway)
    }

    /// Update a gateway
    #[instrument(skip(self, request), fields(user_id = %request.user_id, gateway_id = %request.gateway_id, organization_id = %request.organization_id))]
    pub async fn update_gateway(&self, request: UpdateGatewayRequest) -> DomainResult<Gateway> {
        // Validate request using garde
        common::validation::validate(&request)?;

        debug!(gateway_id = %request.gateway_id, organization_id = %request.organization_id, "Updating gateway");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Gateway,
                Action::Update,
            )
            .await?;

        let repo_input = UpdateGatewayRepoInput {
            gateway_id: request.gateway_id,
            organization_id: request.organization_id,
            gateway_type: request.gateway_type,
            gateway_config: request.gateway_config,
        };

        let gateway = self.gateway_repository.update_gateway(repo_input).await?;

        debug!(gateway_id = %gateway.gateway_id, "Gateway updated successfully");
        Ok(gateway)
    }

    /// Soft delete a gateway
    #[instrument(skip(self, request), fields(user_id = %request.user_id, gateway_id = %request.gateway_id, organization_id = %request.organization_id))]
    pub async fn delete_gateway(&self, request: DeleteGatewayRequest) -> DomainResult<()> {
        // Validate request using garde
        common::validation::validate(&request)?;

        debug!(gateway_id = %request.gateway_id, organization_id = %request.organization_id, "Deleting gateway");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Gateway,
                Action::Delete,
            )
            .await?;

        let repo_input = DeleteGatewayRepoInput {
            gateway_id: request.gateway_id,
            organization_id: request.organization_id,
        };

        self.gateway_repository.delete_gateway(repo_input).await?;

        debug!("Gateway soft deleted successfully");
        Ok(())
    }

    /// List gateways by organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn list_gateways(&self, request: ListGatewaysRequest) -> DomainResult<Vec<Gateway>> {
        // Validate request using garde
        common::validation::validate(&request)?;

        debug!(organization_id = %request.organization_id, "Listing gateways");

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Gateway,
                Action::Read,
            )
            .await?;

        let repo_input = ListGatewaysRepoInput {
            organization_id: request.organization_id,
        };

        let gateways = self.gateway_repository.list_gateways(repo_input).await?;

        debug!(count = gateways.len(), "Listed gateways");
        Ok(gateways)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{
        EmqxGatewayConfig, GatewayConfig, MockGatewayRepository, MockOrganizationRepository,
        Organization,
    };

    const TEST_USER_ID: &str = "user-123";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    #[tokio::test]
    async fn test_create_gateway_success() {
        let mut mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        let test_config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            subscription_group: "ponix".to_string(),
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

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: test_config,
        };

        let result = service.create_gateway(request).await;
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

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-999".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "mqtt://localhost:1883".to_string(),
                subscription_group: "ponix".to_string(),
            }),
        };

        let result = service.create_gateway(request).await;
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

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "mqtt://localhost:1883".to_string(),
                subscription_group: "ponix".to_string(),
            }),
        };

        let result = service.create_gateway(request).await;
        assert!(matches!(result, Err(DomainError::OrganizationDeleted(_))));
    }

    #[tokio::test]
    async fn test_create_gateway_empty_broker_url() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "".to_string(),
                subscription_group: "ponix".to_string(),
            }),
        };

        let result = service.create_gateway(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
        if let Err(DomainError::ValidationError(msg)) = result {
            assert!(msg.contains("broker_url"));
        }
    }

    #[tokio::test]
    async fn test_create_gateway_empty_subscription_group() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-001".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "mqtt://localhost:1883".to_string(),
                subscription_group: "".to_string(),
            }),
        };

        let result = service.create_gateway(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
        if let Err(DomainError::ValidationError(msg)) = result {
            assert!(msg.contains("subscription_group"));
        }
    }

    #[tokio::test]
    async fn test_get_gateway_empty_organization_id_fails() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = GetGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            gateway_id: "gw-123".to_string(),
            organization_id: "".to_string(),
        };

        let result = service.get_gateway(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_update_gateway_empty_organization_id_fails() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = UpdateGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            gateway_id: "gw-123".to_string(),
            organization_id: "".to_string(),
            gateway_type: None,
            gateway_config: None,
        };

        let result = service.update_gateway(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_delete_gateway_empty_organization_id_fails() {
        let mock_gateway_repo = MockGatewayRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = DeleteGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            gateway_id: "gw-123".to_string(),
            organization_id: "".to_string(),
        };

        let result = service.delete_gateway(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }
}

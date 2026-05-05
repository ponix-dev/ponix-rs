use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateGatewayRepoInput, DeleteGatewayRepoInput, DomainError, DomainResult, Gateway,
    GatewayRepository, GetGatewayRepoInput, GetOrganizationRepoInput, ListGatewaysRepoInput,
    MqttCredentials, OrganizationRepository, UpdateGatewayRepoInput,
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
    pub name: String,
    #[garde(length(min = 1))]
    pub broker_url: String,
    #[garde(dive)]
    pub credentials: Option<MqttCredentials>,
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
    pub name: Option<String>,
    #[garde(inner(length(min = 1)))]
    pub broker_url: Option<String>,
    #[garde(dive)]
    pub credentials: Option<MqttCredentials>,
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
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn create_gateway(&self, request: CreateGatewayRequest) -> DomainResult<Gateway> {
        common::garde::validate_struct(&request)?;

        debug!(organization_id = %request.organization_id, "Creating gateway");

        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Gateway,
                Action::Create,
            )
            .await?;

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

        let gateway_id = xid::new().to_string();

        let repo_input = CreateGatewayRepoInput {
            gateway_id: gateway_id.clone(),
            organization_id: request.organization_id,
            name: request.name,
            broker_url: request.broker_url,
            credentials: request.credentials,
        };

        let gateway = self.gateway_repository.create_gateway(repo_input).await?;

        debug!(gateway_id = %gateway.gateway_id, "Gateway created successfully");
        Ok(gateway)
    }

    /// Get a gateway by ID and organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, gateway_id = %request.gateway_id, organization_id = %request.organization_id))]
    pub async fn get_gateway(&self, request: GetGatewayRequest) -> DomainResult<Gateway> {
        common::garde::validate_struct(&request)?;

        debug!(gateway_id = %request.gateway_id, organization_id = %request.organization_id, "Getting gateway");

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
        common::garde::validate_struct(&request)?;

        debug!(gateway_id = %request.gateway_id, organization_id = %request.organization_id, "Updating gateway");

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
            name: request.name,
            broker_url: request.broker_url,
            credentials: request.credentials,
        };

        let gateway = self.gateway_repository.update_gateway(repo_input).await?;

        debug!(gateway_id = %gateway.gateway_id, "Gateway updated successfully");
        Ok(gateway)
    }

    /// Soft delete a gateway
    #[instrument(skip(self, request), fields(user_id = %request.user_id, gateway_id = %request.gateway_id, organization_id = %request.organization_id))]
    pub async fn delete_gateway(&self, request: DeleteGatewayRequest) -> DomainResult<()> {
        common::garde::validate_struct(&request)?;

        debug!(gateway_id = %request.gateway_id, organization_id = %request.organization_id, "Deleting gateway");

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
        common::garde::validate_struct(&request)?;

        debug!(organization_id = %request.organization_id, "Listing gateways");

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
    use common::domain::{MockGatewayRepository, MockOrganizationRepository, Organization};

    const TEST_USER_ID: &str = "user-123";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    fn make_org(id: &str, deleted: bool) -> Organization {
        Organization {
            id: id.to_string(),
            name: "Test Org".to_string(),
            deleted_at: deleted.then(Utc::now),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        }
    }

    #[tokio::test]
    async fn test_create_gateway_success() {
        let mut mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .withf(|input| input.organization_id == "org-001")
            .times(1)
            .return_once(|_| Ok(Some(make_org("org-001", false))));

        mock_gateway_repo
            .expect_create_gateway()
            .withf(|input| {
                input.organization_id == "org-001"
                    && input.broker_url == "mqtt://mqtt.example.com:1883"
                    && !input.gateway_id.is_empty()
                    && input.name == "Test Gateway"
                    && input.credentials.is_none()
            })
            .times(1)
            .return_once(|input| {
                Ok(Gateway {
                    gateway_id: input.gateway_id,
                    organization_id: input.organization_id,
                    name: input.name,
                    broker_url: input.broker_url,
                    credentials: input.credentials,
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
            name: "Test Gateway".to_string(),
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            credentials: None,
        };

        let gateway = service.create_gateway(request).await.unwrap();
        assert_eq!(gateway.organization_id, "org-001");
        assert_eq!(gateway.broker_url, "mqtt://mqtt.example.com:1883");
        assert_eq!(gateway.name, "Test Gateway");
        assert!(gateway.credentials.is_none());
    }

    #[tokio::test]
    async fn test_create_gateway_with_credentials() {
        let mut mock_gateway_repo = MockGatewayRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(Some(make_org("org-001", false))));

        mock_gateway_repo
            .expect_create_gateway()
            .withf(|input| {
                input
                    .credentials
                    .as_ref()
                    .map(|c| c.username == "alice" && c.password == "s3cret")
                    .unwrap_or(false)
            })
            .times(1)
            .return_once(|input| {
                Ok(Gateway {
                    gateway_id: input.gateway_id,
                    organization_id: input.organization_id,
                    name: input.name,
                    broker_url: input.broker_url,
                    credentials: input.credentials,
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
            name: "Test Gateway".to_string(),
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            credentials: Some(MqttCredentials {
                username: "alice".to_string(),
                password: "s3cret".to_string(),
            }),
        };

        let gateway = service.create_gateway(request).await.unwrap();
        assert!(gateway.credentials.is_some());
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
            name: "Test Gateway".to_string(),
            broker_url: "mqtt://localhost:1883".to_string(),
            credentials: None,
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
            .return_once(|_| Ok(Some(make_org("org-001", true))));

        let service = GatewayService::new(
            Arc::new(mock_gateway_repo),
            Arc::new(mock_org_repo),
            create_mock_auth_provider(),
        );

        let request = CreateGatewayRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-001".to_string(),
            name: "Test Gateway".to_string(),
            broker_url: "mqtt://localhost:1883".to_string(),
            credentials: None,
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
            name: "Test Gateway".to_string(),
            broker_url: "".to_string(),
            credentials: None,
        };

        let result = service.create_gateway(request).await;
        let err = result.unwrap_err();
        match err {
            DomainError::ValidationError(msg) => assert!(msg.contains("broker_url")),
            other => panic!("expected ValidationError, got {other:?}"),
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

        let err = service.get_gateway(request).await.unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
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
            name: None,
            broker_url: None,
            credentials: None,
        };

        let err = service.update_gateway(request).await.unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
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

        let err = service.delete_gateway(request).await.unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }
}

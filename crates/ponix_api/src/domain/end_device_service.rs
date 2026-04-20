use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateEndDeviceRepoInput, DomainError, DomainResult, EndDevice, EndDeviceDefinitionRepository,
    EndDeviceRepository, GatewayRepository, GetEndDeviceDefinitionRepoInput, GetEndDeviceRepoInput,
    GetGatewayRepoInput, GetOrganizationRepoInput, ListEndDevicesByGatewayRepoInput,
    ListEndDevicesRepoInput, OrganizationRepository,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Service request for creating an end device
#[derive(Debug, Clone, Validate)]
pub struct CreateEndDeviceRequest {
    #[garde(skip)] // user_id validated by auth layer
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub definition_id: String,
    #[garde(length(min = 1))]
    pub gateway_id: String,
    #[garde(length(min = 1))]
    pub name: String,
}

/// Service request for getting an end device
#[derive(Debug, Clone, Validate)]
pub struct GetEndDeviceRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub end_device_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for listing end devices
#[derive(Debug, Clone, Validate)]
pub struct ListEndDevicesRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for listing end devices by gateway
#[derive(Debug, Clone, Validate)]
pub struct ListEndDevicesByGatewayRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub gateway_id: String,
}

/// Domain service for end device management business logic
/// This is the orchestration layer that handlers call
pub struct EndDeviceService {
    end_device_repository: Arc<dyn EndDeviceRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    definition_repository: Arc<dyn EndDeviceDefinitionRepository>,
    gateway_repository: Arc<dyn GatewayRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl EndDeviceService {
    pub fn new(
        end_device_repository: Arc<dyn EndDeviceRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        definition_repository: Arc<dyn EndDeviceDefinitionRepository>,
        gateway_repository: Arc<dyn GatewayRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            end_device_repository,
            organization_repository,
            definition_repository,
            gateway_repository,
            authorization_provider,
        }
    }

    /// Create a new end device with business logic validation
    /// Generates a unique end_device_id using xid
    /// Validates that the organization exists and is not deleted
    /// Validates that the definition exists and belongs to the same organization
    /// Validates that the gateway exists and belongs to the same organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, gateway_id = %request.gateway_id, end_device_name = %request.name))]
    pub async fn create_end_device(
        &self,
        request: CreateEndDeviceRequest,
    ) -> DomainResult<EndDevice> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::EndDevice,
                Action::Create,
            )
            .await?;

        // Validate organization exists and is not deleted
        debug!(organization_id = %request.organization_id, "validating organization exists");
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
                        "Cannot create end device for deleted organization: {}",
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

        // Validate definition exists and belongs to the same organization
        debug!(definition_id = %request.definition_id, "validating definition exists");
        let def_input = GetEndDeviceDefinitionRepoInput {
            id: request.definition_id.clone(),
            organization_id: request.organization_id.clone(),
        };

        self.definition_repository
            .get_definition(def_input)
            .await?
            .ok_or_else(|| {
                DomainError::EndDeviceDefinitionNotFound(request.definition_id.clone())
            })?;

        // Validate gateway exists and belongs to the same organization
        debug!(gateway_id = %request.gateway_id, "validating gateway exists");
        let gateway_input = GetGatewayRepoInput {
            gateway_id: request.gateway_id.clone(),
            organization_id: request.organization_id.clone(),
        };

        self.gateway_repository
            .get_gateway(gateway_input)
            .await?
            .ok_or_else(|| {
                DomainError::GatewayNotFound(format!(
                    "Gateway {} not found in organization {}",
                    request.gateway_id, request.organization_id
                ))
            })?;

        // Generate unique end device ID
        let end_device_id = xid::new().to_string();

        debug!(end_device_id = %end_device_id, organization_id = %request.organization_id, gateway_id = %request.gateway_id, "creating end device");

        // Create input with generated ID for repository
        let repo_input = CreateEndDeviceRepoInput {
            end_device_id,
            organization_id: request.organization_id,
            definition_id: request.definition_id,
            gateway_id: request.gateway_id,
            name: request.name,
        };

        let end_device = self
            .end_device_repository
            .create_end_device(repo_input)
            .await?;

        Ok(end_device)
    }

    /// Get an end device by ID and organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, end_device_id = %request.end_device_id, organization_id = %request.organization_id))]
    pub async fn get_end_device(&self, request: GetEndDeviceRequest) -> DomainResult<EndDevice> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::EndDevice,
                Action::Read,
            )
            .await?;

        debug!(end_device_id = %request.end_device_id, organization_id = %request.organization_id, "getting end device");

        let repo_input = GetEndDeviceRepoInput {
            end_device_id: request.end_device_id,
            organization_id: request.organization_id,
        };

        let end_device = self
            .end_device_repository
            .get_end_device(repo_input)
            .await?
            .ok_or_else(|| DomainError::EndDeviceNotFound("End device not found".to_string()))?;

        Ok(end_device)
    }

    /// List end devices for an organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn list_end_devices(
        &self,
        request: ListEndDevicesRequest,
    ) -> DomainResult<Vec<EndDevice>> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        // Check authorization (at org level)
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::EndDevice,
                Action::Read,
            )
            .await?;

        debug!(organization_id = %request.organization_id, "listing end devices for organization");

        let repo_input = ListEndDevicesRepoInput {
            organization_id: request.organization_id,
        };

        let end_devices = self
            .end_device_repository
            .list_end_devices(repo_input)
            .await?;

        debug!(
            count = end_devices.len(),
            "listed end devices for organization"
        );
        Ok(end_devices)
    }

    /// List end devices for a gateway
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, gateway_id = %request.gateway_id))]
    pub async fn list_end_devices_by_gateway(
        &self,
        request: ListEndDevicesByGatewayRequest,
    ) -> DomainResult<Vec<EndDevice>> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        // Check authorization (at org level for end device read)
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::EndDevice,
                Action::Read,
            )
            .await?;

        // Validate gateway exists and belongs to the organization
        debug!(gateway_id = %request.gateway_id, "validating gateway exists");
        let gateway_input = GetGatewayRepoInput {
            gateway_id: request.gateway_id.clone(),
            organization_id: request.organization_id.clone(),
        };

        self.gateway_repository
            .get_gateway(gateway_input)
            .await?
            .ok_or_else(|| {
                DomainError::GatewayNotFound(format!(
                    "Gateway {} not found in organization {}",
                    request.gateway_id, request.organization_id
                ))
            })?;

        debug!(organization_id = %request.organization_id, gateway_id = %request.gateway_id, "listing end devices for gateway");

        let repo_input = ListEndDevicesByGatewayRepoInput {
            organization_id: request.organization_id,
            gateway_id: request.gateway_id,
        };

        let end_devices = self
            .end_device_repository
            .list_end_devices_by_gateway(repo_input)
            .await?;

        debug!(count = end_devices.len(), "listed end devices for gateway");
        Ok(end_devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{
        EndDeviceDefinition, Gateway, GatewayConfig, MockEndDeviceDefinitionRepository,
        MockEndDeviceRepository, MockGatewayRepository, MockOrganizationRepository, Organization,
    };

    const TEST_USER_ID: &str = "user-123";
    const TEST_GATEWAY_ID: &str = "gw-001";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    fn create_mock_gateway_repo() -> MockGatewayRepository {
        let mut mock = MockGatewayRepository::new();
        mock.expect_get_gateway().returning(|input| {
            Ok(Some(Gateway {
                gateway_id: input.gateway_id.clone(),
                organization_id: input.organization_id.clone(),
                name: "Test Gateway".to_string(),
                gateway_type: "emqx".to_string(),
                gateway_config: GatewayConfig::Emqx(common::domain::EmqxGatewayConfig {
                    broker_url: "mqtt://localhost:1883".to_string(),
                }),
                deleted_at: None,
                created_at: None,
                updated_at: None,
            }))
        });
        mock
    }

    #[tokio::test]
    async fn test_create_end_device_success() {
        let mut mock_end_device_repo = MockEndDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = create_mock_gateway_repo();

        // Mock organization exists and is active
        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_org_repo
            .expect_get_organization()
            .withf(|input: &GetOrganizationRepoInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        // Mock definition exists
        let def = EndDeviceDefinition {
            id: "def-789".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![],
            created_at: None,
            updated_at: None,
        };

        mock_def_repo
            .expect_get_definition()
            .withf(|input: &GetEndDeviceDefinitionRepoInput| {
                input.id == "def-789" && input.organization_id == "org-456"
            })
            .times(1)
            .return_once(move |_| Ok(Some(def)));

        let expected_end_device = EndDevice {
            end_device_id: "ed-123".to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test End Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_end_device_repo
            .expect_create_end_device()
            .withf(|input: &CreateEndDeviceRepoInput| {
                !input.end_device_id.is_empty() // ID is generated
                    && input.organization_id == "org-456"
                    && input.definition_id == "def-789"
                    && input.gateway_id == TEST_GATEWAY_ID
                    && input.name == "Test End Device"
            })
            .times(1)
            .return_once(move |_| Ok(expected_end_device.clone()));

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test End Device".to_string(),
        };

        let result = service.create_end_device(request).await;
        assert!(result.is_ok());

        let end_device = result.unwrap();
        assert!(!end_device.end_device_id.is_empty()); // ID was generated
        assert_eq!(end_device.name, "Test End Device");
        assert_eq!(end_device.definition_id, "def-789");
        assert_eq!(end_device.gateway_id, TEST_GATEWAY_ID);
    }

    #[tokio::test]
    async fn test_create_end_device_empty_name() {
        let mock_end_device_repo = MockEndDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "".to_string(),
        };

        let result = service.create_end_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_create_end_device_organization_not_found() {
        let mock_end_device_repo = MockEndDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        // Mock organization not found
        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "nonexistent-org".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test End Device".to_string(),
        };

        let result = service.create_end_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::OrganizationNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_create_end_device_organization_deleted() {
        let mock_end_device_repo = MockEndDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        // Mock deleted organization
        let deleted_org = Organization {
            id: "org-deleted".to_string(),
            name: "Deleted Org".to_string(),
            deleted_at: Some(chrono::Utc::now()),
            created_at: None,
            updated_at: None,
        };

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(deleted_org)));

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-deleted".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test End Device".to_string(),
        };

        let result = service.create_end_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::OrganizationDeleted(_)
        ));
    }

    #[tokio::test]
    async fn test_create_end_device_definition_not_found() {
        let mock_end_device_repo = MockEndDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        // Mock organization exists
        let org = Organization {
            id: "org-456".to_string(),
            name: "Test Org".to_string(),
            deleted_at: None,
            created_at: None,
            updated_at: None,
        };

        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        // Mock definition not found
        mock_def_repo
            .expect_get_definition()
            .times(1)
            .return_once(|_| Ok(None));

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = CreateEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "nonexistent-def".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test End Device".to_string(),
        };

        let result = service.create_end_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::EndDeviceDefinitionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_end_device_success() {
        let mut mock_end_device_repo = MockEndDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let expected_end_device = EndDevice {
            end_device_id: "ed-123".to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test End Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_end_device_repo
            .expect_get_end_device()
            .withf(|input: &GetEndDeviceRepoInput| {
                input.end_device_id == "ed-123" && input.organization_id == "org-456"
            })
            .times(1)
            .return_once(move |_| Ok(Some(expected_end_device)));

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = GetEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            end_device_id: "ed-123".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_end_device(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_end_device_not_found() {
        let mut mock_end_device_repo = MockEndDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        mock_end_device_repo
            .expect_get_end_device()
            .times(1)
            .return_once(|_| Ok(None));

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = GetEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            end_device_id: "nonexistent".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_end_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::EndDeviceNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_end_device_empty_organization_id_fails() {
        let mock_end_device_repo = MockEndDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = GetEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            end_device_id: "ed-123".to_string(),
            organization_id: "".to_string(),
        };

        let result = service.get_end_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_list_end_devices_success() {
        let mut mock_end_device_repo = MockEndDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let end_devices = vec![
            EndDevice {
                end_device_id: "ed-1".to_string(),
                organization_id: "org-456".to_string(),
                definition_id: "def-789".to_string(),
                gateway_id: TEST_GATEWAY_ID.to_string(),
                name: "End Device 1".to_string(),
                created_at: None,
                updated_at: None,
            },
            EndDevice {
                end_device_id: "ed-2".to_string(),
                organization_id: "org-456".to_string(),
                definition_id: "def-789".to_string(),
                gateway_id: TEST_GATEWAY_ID.to_string(),
                name: "End Device 2".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        mock_end_device_repo
            .expect_list_end_devices()
            .withf(|input: &ListEndDevicesRepoInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(end_devices));

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            create_mock_auth_provider(),
        );

        let request = ListEndDevicesRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.list_end_devices(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_create_end_device_permission_denied() {
        let mock_end_device_repo = MockEndDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();
        let mock_gateway_repo = MockGatewayRepository::new();

        let mut mock_auth = MockAuthorizationProvider::new();
        mock_auth
            .expect_require_permission()
            .returning(|_, _, _, _| {
                Box::pin(async {
                    Err(DomainError::PermissionDenied(
                        "User does not have permission".to_string(),
                    ))
                })
            });

        let service = EndDeviceService::new(
            Arc::new(mock_end_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_gateway_repo),
            Arc::new(mock_auth),
        );

        let request = CreateEndDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            gateway_id: TEST_GATEWAY_ID.to_string(),
            name: "Test End Device".to_string(),
        };

        let result = service.create_end_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::PermissionDenied(_)
        ));
    }
}

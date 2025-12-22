use common::auth::{Action, AuthorizationProvider, Resource};
use common::domain::{
    CreateDeviceRepoInput, Device, DeviceRepository, DomainError, DomainResult,
    EndDeviceDefinitionRepository, GetDeviceRepoInput, GetEndDeviceDefinitionRepoInput,
    GetOrganizationRepoInput, ListDevicesRepoInput, OrganizationRepository,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

/// Service request for creating a device
#[derive(Debug, Clone, Validate)]
pub struct CreateDeviceRequest {
    #[garde(skip)] // user_id validated by auth layer
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
    #[garde(length(min = 1))]
    pub definition_id: String,
    #[garde(length(min = 1))]
    pub name: String,
}

/// Service request for getting a device
#[derive(Debug, Clone, Validate)]
pub struct GetDeviceRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub device_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Service request for listing devices
#[derive(Debug, Clone, Validate)]
pub struct ListDevicesRequest {
    #[garde(skip)]
    pub user_id: String,
    #[garde(length(min = 1))]
    pub organization_id: String,
}

/// Domain service for device management business logic
/// This is the orchestration layer that handlers call
pub struct DeviceService {
    device_repository: Arc<dyn DeviceRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
    definition_repository: Arc<dyn EndDeviceDefinitionRepository>,
    authorization_provider: Arc<dyn AuthorizationProvider>,
}

impl DeviceService {
    pub fn new(
        device_repository: Arc<dyn DeviceRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
        definition_repository: Arc<dyn EndDeviceDefinitionRepository>,
        authorization_provider: Arc<dyn AuthorizationProvider>,
    ) -> Self {
        Self {
            device_repository,
            organization_repository,
            definition_repository,
            authorization_provider,
        }
    }

    /// Create a new device with business logic validation
    /// Generates a unique device_id using xid
    /// Validates that the organization exists and is not deleted
    /// Validates that the definition exists and belongs to the same organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id, device_name = %request.name))]
    pub async fn create_device(&self, request: CreateDeviceRequest) -> DomainResult<Device> {
        // Validate request using garde
        common::garde::validate(&request)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device,
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
                        "Cannot create device for deleted organization: {}",
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

        // Generate unique device ID
        let device_id = xid::new().to_string();

        debug!(device_id = %device_id, organization_id = %request.organization_id, "creating device");

        // Create input with generated ID for repository
        let repo_input = CreateDeviceRepoInput {
            device_id,
            organization_id: request.organization_id,
            definition_id: request.definition_id,
            name: request.name,
        };

        let device = self.device_repository.create_device(repo_input).await?;

        Ok(device)
    }

    /// Get a device by ID and organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, device_id = %request.device_id, organization_id = %request.organization_id))]
    pub async fn get_device(&self, request: GetDeviceRequest) -> DomainResult<Device> {
        // Validate request using garde
        common::garde::validate(&request)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device,
                Action::Read,
            )
            .await?;

        debug!(device_id = %request.device_id, organization_id = %request.organization_id, "getting device");

        let repo_input = GetDeviceRepoInput {
            device_id: request.device_id,
            organization_id: request.organization_id,
        };

        let device = self
            .device_repository
            .get_device(repo_input)
            .await?
            .ok_or_else(|| DomainError::DeviceNotFound("Device not found".to_string()))?;

        Ok(device)
    }

    /// List devices for an organization
    #[instrument(skip(self, request), fields(user_id = %request.user_id, organization_id = %request.organization_id))]
    pub async fn list_devices(&self, request: ListDevicesRequest) -> DomainResult<Vec<Device>> {
        // Validate request using garde
        common::garde::validate(&request)?;

        // Check authorization
        self.authorization_provider
            .require_permission(
                &request.user_id,
                &request.organization_id,
                Resource::Device,
                Action::Read,
            )
            .await?;

        debug!(organization_id = %request.organization_id, "listing devices");

        let repo_input = ListDevicesRepoInput {
            organization_id: request.organization_id,
        };

        let devices = self.device_repository.list_devices(repo_input).await?;

        debug!(count = devices.len(), "listed devices");
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::MockAuthorizationProvider;
    use common::domain::{
        EndDeviceDefinition, MockDeviceRepository, MockEndDeviceDefinitionRepository,
        MockOrganizationRepository, Organization,
    };

    const TEST_USER_ID: &str = "user-123";

    fn create_mock_auth_provider() -> Arc<MockAuthorizationProvider> {
        let mut mock = MockAuthorizationProvider::new();
        mock.expect_require_permission()
            .returning(|_, _, _, _| Box::pin(async { Ok(()) }));
        Arc::new(mock)
    }

    #[tokio::test]
    async fn test_create_device_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();

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
            json_schema: "{}".to_string(),
            payload_conversion: "cayenne_lpp.decode(payload)".to_string(),
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

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            name: "Test Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_create_device()
            .withf(|input: &CreateDeviceRepoInput| {
                !input.device_id.is_empty() // ID is generated
                    && input.organization_id == "org-456"
                    && input.definition_id == "def-789"
                    && input.name == "Test Device"
            })
            .times(1)
            .return_once(move |_| Ok(expected_device.clone()));

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(request).await;
        assert!(result.is_ok());

        let device = result.unwrap();
        assert!(!device.device_id.is_empty()); // ID was generated
        assert_eq!(device.name, "Test Device");
        assert_eq!(device.definition_id, "def-789");
    }

    #[tokio::test]
    async fn test_create_device_empty_name() {
        let mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            name: "".to_string(),
        };

        let result = service.create_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_create_device_organization_not_found() {
        let mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

        // Mock organization not found
        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "nonexistent-org".to_string(),
            definition_id: "def-789".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::OrganizationNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_create_device_organization_deleted() {
        let mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

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

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-deleted".to_string(),
            definition_id: "def-789".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::OrganizationDeleted(_)
        ));
    }

    #[tokio::test]
    async fn test_create_device_definition_not_found() {
        let mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();
        let mut mock_def_repo = MockEndDeviceDefinitionRepository::new();

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

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = CreateDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "nonexistent-def".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::EndDeviceDefinitionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_device_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            name: "Test Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device()
            .withf(|input: &GetDeviceRepoInput| {
                input.device_id == "device-123" && input.organization_id == "org-456"
            })
            .times(1)
            .return_once(move |_| Ok(Some(expected_device)));

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = GetDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_device(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_device_not_found() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

        mock_device_repo
            .expect_get_device()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = GetDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            device_id: "nonexistent".to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.get_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::DeviceNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_get_device_empty_organization_id_fails() {
        let mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = GetDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            device_id: "device-123".to_string(),
            organization_id: "".to_string(),
        };

        let result = service.get_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[tokio::test]
    async fn test_list_devices_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

        let devices = vec![
            Device {
                device_id: "device-1".to_string(),
                organization_id: "org-456".to_string(),
                definition_id: "def-789".to_string(),
                name: "Device 1".to_string(),
                created_at: None,
                updated_at: None,
            },
            Device {
                device_id: "device-2".to_string(),
                organization_id: "org-456".to_string(),
                definition_id: "def-789".to_string(),
                name: "Device 2".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        mock_device_repo
            .expect_list_devices()
            .withf(|input: &ListDevicesRepoInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(devices));

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            create_mock_auth_provider(),
        );

        let request = ListDevicesRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
        };

        let result = service.list_devices(request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_create_device_permission_denied() {
        let mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let mock_def_repo = MockEndDeviceDefinitionRepository::new();

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

        let service = DeviceService::new(
            Arc::new(mock_device_repo),
            Arc::new(mock_org_repo),
            Arc::new(mock_def_repo),
            Arc::new(mock_auth),
        );

        let request = CreateDeviceRequest {
            user_id: TEST_USER_ID.to_string(),
            organization_id: "org-456".to_string(),
            definition_id: "def-789".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::PermissionDenied(_)
        ));
    }
}

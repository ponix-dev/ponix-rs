use std::sync::Arc;
use tracing::{debug, info};

use crate::end_device::*;
use crate::error::{DomainError, DomainResult};
use crate::organization::GetOrganizationInput;
use crate::repository::{DeviceRepository, OrganizationRepository};

/// Domain service for device management business logic
/// This is the orchestration layer that handlers call
pub struct DeviceService {
    device_repository: Arc<dyn DeviceRepository>,
    organization_repository: Arc<dyn OrganizationRepository>,
}

impl DeviceService {
    pub fn new(
        device_repository: Arc<dyn DeviceRepository>,
        organization_repository: Arc<dyn OrganizationRepository>,
    ) -> Self {
        Self {
            device_repository,
            organization_repository,
        }
    }

    /// Create a new device with business logic validation
    /// Generates a unique device_id using xid
    /// Validates that the organization exists and is not deleted
    pub async fn create_device(&self, input: CreateDeviceInput) -> DomainResult<Device> {
        // Business logic: validate inputs
        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId(
                "Organization ID cannot be empty".to_string(),
            ));
        }

        if input.name.is_empty() {
            return Err(DomainError::InvalidDeviceName(
                "Device name cannot be empty".to_string(),
            ));
        }

        // TODO: this may be more efficient to do at the db layer with a foreign key constraint
        // Validate organization exists and is not deleted
        debug!(organization_id = %input.organization_id, "Validating organization exists");
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
                        "Cannot create device for deleted organization: {}",
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

        // Generate unique device ID
        let device_id = xid::new().to_string();

        debug!(device_id = %device_id, organization_id = %input.organization_id, "Creating device");

        // Create input with generated ID for repository
        let repo_input = CreateDeviceInputWithId {
            device_id,
            organization_id: input.organization_id,
            name: input.name,
            payload_conversion: input.payload_conversion,
        };

        let device = self.device_repository.create_device(repo_input).await?;

        info!(device_id = %device.device_id, "Device created successfully");
        Ok(device)
    }

    /// Get a device by ID
    pub async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Device> {
        if input.device_id.is_empty() {
            return Err(DomainError::InvalidDeviceId(
                "Device ID cannot be empty".to_string(),
            ));
        }

        debug!(device_id = %input.device_id, "Getting device");

        let device = self
            .device_repository
            .get_device(input)
            .await?
            .ok_or_else(|| DomainError::DeviceNotFound("Device not found".to_string()))?;

        Ok(device)
    }

    /// List devices for an organization
    pub async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>> {
        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId(
                "Organization ID cannot be empty".to_string(),
            ));
        }

        debug!(organization_id = %input.organization_id, "Listing devices");

        let devices = self.device_repository.list_devices(input).await?;

        info!(count = devices.len(), "Listed devices");
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organization::Organization;
    use crate::repository::{MockDeviceRepository, MockOrganizationRepository};

    #[tokio::test]
    async fn test_create_device_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

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
            .withf(|input: &GetOrganizationInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(Some(org)));

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "test conversion".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_create_device()
            .withf(|input: &CreateDeviceInputWithId| {
                !input.device_id.is_empty() // ID is generated
                    && input.organization_id == "org-456"
                    && input.name == "Test Device"
                    && input.payload_conversion == "test conversion"
            })
            .times(1)
            .return_once(move |_| Ok(expected_device.clone()));

        let service = DeviceService::new(Arc::new(mock_device_repo), Arc::new(mock_org_repo));

        let input = CreateDeviceInput {
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "test conversion".to_string(),
        };

        let result = service.create_device(input).await;
        assert!(result.is_ok());

        let device = result.unwrap();
        assert!(!device.device_id.is_empty()); // ID was generated
        assert_eq!(device.name, "Test Device");
    }

    #[tokio::test]
    async fn test_create_device_empty_name() {
        let mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();
        let service = DeviceService::new(Arc::new(mock_device_repo), Arc::new(mock_org_repo));

        let input = CreateDeviceInput {
            organization_id: "org-456".to_string(),
            name: "".to_string(),
            payload_conversion: "test conversion".to_string(),
        };

        let result = service.create_device(input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::InvalidDeviceName(_)
        ));
    }

    #[tokio::test]
    async fn test_create_device_organization_not_found() {
        let mock_device_repo = MockDeviceRepository::new();
        let mut mock_org_repo = MockOrganizationRepository::new();

        // Mock organization not found
        mock_org_repo
            .expect_get_organization()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DeviceService::new(Arc::new(mock_device_repo), Arc::new(mock_org_repo));

        let input = CreateDeviceInput {
            organization_id: "nonexistent-org".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "test conversion".to_string(),
        };

        let result = service.create_device(input).await;
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

        let service = DeviceService::new(Arc::new(mock_device_repo), Arc::new(mock_org_repo));

        let input = CreateDeviceInput {
            organization_id: "org-deleted".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "test conversion".to_string(),
        };

        let result = service.create_device(input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::OrganizationDeleted(_)
        ));
    }

    #[tokio::test]
    async fn test_get_device_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "test conversion".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_device_repo
            .expect_get_device()
            .withf(|input: &GetDeviceInput| input.device_id == "device-123")
            .times(1)
            .return_once(move |_| Ok(Some(expected_device)));

        let service = DeviceService::new(Arc::new(mock_device_repo), Arc::new(mock_org_repo));

        let input = GetDeviceInput {
            device_id: "device-123".to_string(),
        };

        let result = service.get_device(input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_device_not_found() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        mock_device_repo
            .expect_get_device()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DeviceService::new(Arc::new(mock_device_repo), Arc::new(mock_org_repo));

        let input = GetDeviceInput {
            device_id: "nonexistent".to_string(),
        };

        let result = service.get_device(input).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DomainError::DeviceNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_list_devices_success() {
        let mut mock_device_repo = MockDeviceRepository::new();
        let mock_org_repo = MockOrganizationRepository::new();

        let devices = vec![
            Device {
                device_id: "device-1".to_string(),
                organization_id: "org-456".to_string(),
                name: "Device 1".to_string(),
                payload_conversion: "conversion 1".to_string(),
                created_at: None,
                updated_at: None,
            },
            Device {
                device_id: "device-2".to_string(),
                organization_id: "org-456".to_string(),
                name: "Device 2".to_string(),
                payload_conversion: "conversion 2".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        mock_device_repo
            .expect_list_devices()
            .withf(|input: &ListDevicesInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(devices));

        let service = DeviceService::new(Arc::new(mock_device_repo), Arc::new(mock_org_repo));

        let input = ListDevicesInput {
            organization_id: "org-456".to_string(),
        };

        let result = service.list_devices(input).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}

use std::sync::Arc;
use tracing::{debug, info};

use crate::error::{DomainError, DomainResult};
use crate::repository::DeviceRepository;
use crate::types::{CreateDeviceInput, Device, GetDeviceInput, ListDevicesInput};

/// Domain service for device management business logic
/// This is the orchestration layer that handlers call
pub struct DeviceService {
    repository: Arc<dyn DeviceRepository>,
}

impl DeviceService {
    pub fn new(repository: Arc<dyn DeviceRepository>) -> Self {
        Self { repository }
    }

    /// Create a new device with business logic validation
    /// Generates a unique device_id using xid
    pub async fn create_device(&self, input: CreateDeviceInput) -> DomainResult<Device> {
        // Business logic: validate inputs
        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId("Organization ID cannot be empty".to_string()));
        }

        if input.name.is_empty() {
            return Err(DomainError::InvalidDeviceName("Device name cannot be empty".to_string()));
        }

        // Generate unique device ID
        let device_id = xid::new().to_string();

        debug!(device_id = %device_id, organization_id = %input.organization_id, "Creating device");

        // Create input with generated ID for repository
        let repo_input = crate::types::CreateDeviceInputWithId {
            device_id,
            organization_id: input.organization_id,
            name: input.name,
        };

        let device = self.repository.create_device(repo_input).await?;

        info!(device_id = %device.device_id, "Device created successfully");
        Ok(device)
    }

    /// Get a device by ID
    pub async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Device> {
        if input.device_id.is_empty() {
            return Err(DomainError::InvalidDeviceId("Device ID cannot be empty".to_string()));
        }

        debug!(device_id = %input.device_id, "Getting device");

        let device = self.repository.get_device(input).await?
            .ok_or_else(|| DomainError::DeviceNotFound("Device not found".to_string()))?;

        Ok(device)
    }

    /// List devices for an organization
    pub async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>> {
        if input.organization_id.is_empty() {
            return Err(DomainError::InvalidOrganizationId("Organization ID cannot be empty".to_string()));
        }

        debug!(organization_id = %input.organization_id, "Listing devices");

        let devices = self.repository.list_devices(input).await?;

        info!(count = devices.len(), "Listed devices");
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::MockDeviceRepository;

    #[tokio::test]
    async fn test_create_device_success() {
        let mut mock_repo = MockDeviceRepository::new();

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_repo
            .expect_create_device()
            .withf(|input: &crate::types::CreateDeviceInputWithId| {
                !input.device_id.is_empty() // ID is generated
                    && input.organization_id == "org-456"
                    && input.name == "Test Device"
            })
            .times(1)
            .return_once(move |_| Ok(expected_device.clone()));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = CreateDeviceInput {
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
        };

        let result = service.create_device(input).await;
        assert!(result.is_ok());

        let device = result.unwrap();
        assert!(!device.device_id.is_empty()); // ID was generated
        assert_eq!(device.name, "Test Device");
    }

    #[tokio::test]
    async fn test_create_device_empty_name() {
        let mock_repo = MockDeviceRepository::new();
        let service = DeviceService::new(Arc::new(mock_repo));

        let input = CreateDeviceInput {
            organization_id: "org-456".to_string(),
            name: "".to_string(),
        };

        let result = service.create_device(input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DomainError::InvalidDeviceName(_)));
    }

    #[tokio::test]
    async fn test_get_device_success() {
        let mut mock_repo = MockDeviceRepository::new();

        let expected_device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            created_at: None,
            updated_at: None,
        };

        mock_repo
            .expect_get_device()
            .withf(|input: &GetDeviceInput| input.device_id == "device-123")
            .times(1)
            .return_once(move |_| Ok(Some(expected_device)));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = GetDeviceInput {
            device_id: "device-123".to_string(),
        };

        let result = service.get_device(input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_device_not_found() {
        let mut mock_repo = MockDeviceRepository::new();

        mock_repo
            .expect_get_device()
            .times(1)
            .return_once(|_| Ok(None));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = GetDeviceInput {
            device_id: "nonexistent".to_string(),
        };

        let result = service.get_device(input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DomainError::DeviceNotFound(_)));
    }

    #[tokio::test]
    async fn test_list_devices_success() {
        let mut mock_repo = MockDeviceRepository::new();

        let devices = vec![
            Device {
                device_id: "device-1".to_string(),
                organization_id: "org-456".to_string(),
                name: "Device 1".to_string(),
                created_at: None,
                updated_at: None,
            },
            Device {
                device_id: "device-2".to_string(),
                organization_id: "org-456".to_string(),
                name: "Device 2".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];

        mock_repo
            .expect_list_devices()
            .withf(|input: &ListDevicesInput| input.organization_id == "org-456")
            .times(1)
            .return_once(move |_| Ok(devices));

        let service = DeviceService::new(Arc::new(mock_repo));

        let input = ListDevicesInput {
            organization_id: "org-456".to_string(),
        };

        let result = service.list_devices(input).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}

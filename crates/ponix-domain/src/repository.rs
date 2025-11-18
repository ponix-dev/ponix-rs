use async_trait::async_trait;
use crate::error::DomainResult;
use crate::types::{CreateDeviceInput, Device, GetDeviceInput, ListDevicesInput};

/// Repository trait for device storage operations
/// Infrastructure layer (e.g., ponix-postgres) implements this trait
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Create a new device
    async fn create_device(&self, input: CreateDeviceInput) -> DomainResult<Device>;

    /// Get a device by ID
    async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Option<Device>>;

    /// List all devices for an organization
    async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>>;
}

use ponix_domain::{CreateDeviceInput, Device as DomainDevice};
use crate::models::{Device as DbDevice, DeviceRow};

/// Convert domain CreateDeviceInput to database Device (for insert)
impl From<&CreateDeviceInput> for DbDevice {
    fn from(input: &CreateDeviceInput) -> Self {
        DbDevice {
            device_id: input.device_id.clone(),
            organization_id: input.organization_id.clone(),
            device_name: input.name.clone(), // Map name -> device_name
        }
    }
}

/// Convert database DeviceRow to domain Device
impl From<DeviceRow> for DomainDevice {
    fn from(row: DeviceRow) -> Self {
        DomainDevice {
            device_id: row.device_id,
            organization_id: row.organization_id,
            name: row.device_name, // Map device_name -> name
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// Convert database Device (without timestamps) to domain Device
impl From<DbDevice> for DomainDevice {
    fn from(device: DbDevice) -> Self {
        DomainDevice {
            device_id: device.device_id,
            organization_id: device.organization_id,
            name: device.device_name, // Map device_name -> name
            created_at: None,
            updated_at: None,
        }
    }
}

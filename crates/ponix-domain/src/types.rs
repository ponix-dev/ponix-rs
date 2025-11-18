/// Domain representation of a Device
/// Simple String types for now - can evolve to newtypes later
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub device_id: String,
    pub organization_id: String,
    pub name: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Input for creating a new device
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDeviceInput {
    pub device_id: String,
    pub organization_id: String,
    pub name: String,
}

/// Input for retrieving a device
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetDeviceInput {
    pub device_id: String,
}

/// Input for listing devices by organization
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDevicesInput {
    pub organization_id: String,
}

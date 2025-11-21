use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Device representation for API and storage operations
/// NOTE: This will be replaced with ponix_proto::device::v1::Device once available in BSR
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    pub device_id: String,
    pub organization_id: String,
    pub device_name: String,
    pub payload_conversion: String,
}

/// Device row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRow {
    pub device_id: String,
    pub organization_id: String,
    pub device_name: String,
    pub payload_conversion: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

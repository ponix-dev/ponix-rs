use crate::domain::Device;
use chrono::{DateTime, Utc};
use ponix_proto_prost::end_device::v1::EndDevice;
use prost_types::Timestamp;

/// Convert chrono DateTime to protobuf Timestamp
fn datetime_to_timestamp(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(|d| Timestamp {
        seconds: d.timestamp(),
        nanos: d.timestamp_subsec_nanos() as i32,
    })
}

/// Convert domain Device to protobuf EndDevice
pub fn to_proto_device(device: Device) -> EndDevice {
    EndDevice {
        device_id: device.device_id,
        organization_id: device.organization_id,
        name: device.name,
        payload_conversion: device.payload_conversion,
        created_at: datetime_to_timestamp(device.created_at),
        updated_at: datetime_to_timestamp(device.updated_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_device_to_proto() {
        let now = Utc::now();
        let device = Device {
            device_id: "device-123".to_string(),
            organization_id: "org-456".to_string(),
            name: "Test Device".to_string(),
            payload_conversion: "test conversion".to_string(),
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_device(device);

        assert_eq!(proto.device_id, "device-123");
        assert_eq!(proto.organization_id, "org-456");
        assert_eq!(proto.name, "Test Device");
        assert_eq!(proto.payload_conversion, "test conversion");
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
    }
}

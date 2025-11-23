use anyhow::{anyhow, Result};
use ponix_domain::RawEnvelope as DomainRawEnvelope;
use ponix_proto::envelope::v1::RawEnvelope as ProtoRawEnvelope;
use prost_types::Timestamp;

/// Convert protobuf RawEnvelope to domain RawEnvelope
#[cfg(feature = "raw-envelope")]
pub fn proto_to_domain(proto: ProtoRawEnvelope) -> Result<DomainRawEnvelope> {
    let occurred_at = timestamp_to_datetime(
        proto
            .received_at
            .ok_or_else(|| anyhow!("Missing received_at timestamp"))?,
    )?;

    Ok(DomainRawEnvelope {
        organization_id: proto.organization_id,
        end_device_id: proto.device_id,
        occurred_at,
        payload: proto.payload.to_vec(),
    })
}

/// Convert domain RawEnvelope to protobuf RawEnvelope
#[cfg(feature = "raw-envelope")]
pub fn domain_to_proto(envelope: &DomainRawEnvelope) -> ProtoRawEnvelope {
    ProtoRawEnvelope {
        organization_id: envelope.organization_id.clone(),
        device_id: envelope.end_device_id.clone(),
        received_at: Some(Timestamp {
            seconds: envelope.occurred_at.timestamp(),
            nanos: envelope.occurred_at.timestamp_subsec_nanos() as i32,
        }),
        payload: envelope.payload.clone().into(),
    }
}

/// Convert protobuf Timestamp to chrono DateTime
fn timestamp_to_datetime(ts: Timestamp) -> Result<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;

    chrono::Utc
        .timestamp_opt(ts.seconds, ts.nanos as u32)
        .single()
        .ok_or_else(|| {
            anyhow!(
                "Invalid timestamp: {} seconds, {} nanos",
                ts.seconds,
                ts.nanos
            )
        })
}

#[cfg(all(test, feature = "raw-envelope"))]
mod tests {
    use super::*;

    #[test]
    fn test_proto_to_domain_success() {
        let now = chrono::Utc::now();
        let proto = ProtoRawEnvelope {
            organization_id: "org-123".to_string(),
            device_id: "device-456".to_string(),
            received_at: Some(Timestamp {
                seconds: now.timestamp(),
                nanos: now.timestamp_subsec_nanos() as i32,
            }),
            payload: vec![0x01, 0x02, 0x03].into(),
        };

        let result = proto_to_domain(proto.clone());
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, "org-123");
        assert_eq!(domain.end_device_id, "device-456");
        assert_eq!(domain.payload, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_proto_to_domain_missing_timestamp() {
        let proto = ProtoRawEnvelope {
            organization_id: "org-123".to_string(),
            device_id: "device-456".to_string(),
            received_at: None,
            payload: vec![0x01, 0x02, 0x03].into(),
        };

        let result = proto_to_domain(proto);
        assert!(result.is_err());
    }

    #[test]
    fn test_domain_to_proto_success() {
        let now = chrono::Utc::now();
        let domain = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: now,
            payload: vec![0x01, 0x02, 0x03],
        };

        let proto = domain_to_proto(&domain);
        assert_eq!(proto.organization_id, "org-123");
        assert_eq!(proto.device_id, "device-456");
        assert_eq!(proto.payload.to_vec(), vec![0x01, 0x02, 0x03]);
        assert!(proto.received_at.is_some());
    }

    #[test]
    fn test_round_trip_conversion() {
        let now = chrono::Utc::now();
        let original = DomainRawEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: now,
            payload: vec![0x01, 0x02, 0x03],
        };

        let proto = domain_to_proto(&original);
        let result = proto_to_domain(proto);
        assert!(result.is_ok());

        let domain = result.unwrap();
        assert_eq!(domain.organization_id, original.organization_id);
        assert_eq!(domain.end_device_id, original.end_device_id);
        assert_eq!(domain.payload, original.payload);
        // Note: timestamp precision may differ slightly due to conversion
        assert_eq!(
            domain.occurred_at.timestamp(),
            original.occurred_at.timestamp()
        );
    }
}

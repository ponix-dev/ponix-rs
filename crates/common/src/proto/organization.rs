use crate::domain::Organization;
use chrono::{DateTime, Utc};
use ponix_proto_prost::organization::v1::Organization as ProtoOrganization;
use prost_types::Timestamp;

/// Convert chrono DateTime to protobuf Timestamp
pub fn datetime_to_timestamp(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(|d| Timestamp {
        seconds: d.timestamp(),
        nanos: d.timestamp_subsec_nanos() as i32,
    })
}

/// Convert domain Organization to protobuf Organization
/// Note: status field is set based on deleted_at
pub fn to_proto_organization(org: Organization) -> ProtoOrganization {
    ProtoOrganization {
        id: org.id,
        name: org.name,
        status: if org.deleted_at.is_some() { 1 } else { 0 }, // 0 = active, 1 = deleted
        deleted_at: datetime_to_timestamp(org.deleted_at),
        created_at: datetime_to_timestamp(org.created_at),
        updated_at: datetime_to_timestamp(org.updated_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_organization_to_proto() {
        let now = Utc::now();
        let org = Organization {
            id: "org-123".to_string(),
            name: "Test Organization".to_string(),
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_organization(org);

        assert_eq!(proto.id, "org-123");
        assert_eq!(proto.name, "Test Organization");
        assert_eq!(proto.status, 0); // Active
        assert!(proto.deleted_at.is_none());
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
    }
}

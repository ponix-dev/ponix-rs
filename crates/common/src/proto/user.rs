use crate::domain::User;
use crate::proto::organization::datetime_to_timestamp;
use ponix_proto_prost::user::v1::User as ProtoUser;

/// Convert domain User to protobuf User
/// Note: password_hash is NOT included in proto response for security
pub fn to_proto_user(user: User) -> ProtoUser {
    ProtoUser {
        id: user.id,
        email: user.email,
        name: user.name,
        created_at: datetime_to_timestamp(user.created_at),
        updated_at: datetime_to_timestamp(user.updated_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_user_to_proto() {
        let now = Utc::now();
        let user = User {
            id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            password_hash: "argon2hash".to_string(), // Should not appear in proto
            name: "John Doe".to_string(),
            created_at: Some(now),
            updated_at: Some(now),
        };

        let proto = to_proto_user(user);

        assert_eq!(proto.id, "user-123");
        assert_eq!(proto.email, "test@example.com");
        assert_eq!(proto.name, "John Doe");
        assert!(proto.created_at.is_some());
        assert!(proto.updated_at.is_some());
        // Note: password_hash is intentionally not in ProtoUser
    }
}

use crate::auth::LoginUserInput;
use crate::domain::{GetUserInput, RegisterUserInput, User};
use crate::proto::organization::datetime_to_timestamp;
use ponix_proto_prost::user::v1::{
    GetUserRequest, LoginRequest, RegisterUserRequest, User as ProtoUser,
};

/// Convert protobuf RegisterUserRequest to domain RegisterUserInput
pub fn to_register_user_input(req: RegisterUserRequest) -> RegisterUserInput {
    RegisterUserInput {
        email: req.email,
        password: req.password,
        name: req.name,
    }
}

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

/// Convert protobuf GetUserRequest to domain GetUserInput
pub fn to_get_user_input(req: GetUserRequest) -> GetUserInput {
    GetUserInput {
        user_id: req.user_id,
    }
}

/// Convert protobuf LoginRequest to domain LoginUserInput
pub fn to_login_user_input(req: LoginRequest) -> LoginUserInput {
    LoginUserInput {
        email: req.email,
        password: req.password,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_register_request_to_domain() {
        let req = RegisterUserRequest {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
            name: "John Doe".to_string(),
        };

        let input = to_register_user_input(req);

        assert_eq!(input.email, "test@example.com");
        assert_eq!(input.password, "securepassword123");
        assert_eq!(input.name, "John Doe");
    }

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

    #[test]
    fn test_get_request_to_domain() {
        let req = GetUserRequest {
            user_id: "user-456".to_string(),
        };

        let input = to_get_user_input(req);
        assert_eq!(input.user_id, "user-456");
    }

    #[test]
    fn test_login_request_to_domain() {
        let req = LoginRequest {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
        };

        let input = to_login_user_input(req);

        assert_eq!(input.email, "test@example.com");
        assert_eq!(input.password, "securepassword123");
    }
}

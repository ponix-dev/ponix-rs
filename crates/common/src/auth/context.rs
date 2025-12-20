use tonic::{Request, Status};

use super::traits::AuthTokenProvider;
use crate::grpc::domain_error_to_status;

/// User context extracted from authenticated requests
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: String,
}

/// Extract user context from a gRPC request's authorization header
///
/// Expects a Bearer token in the Authorization header and validates it
/// using the provided AuthTokenProvider.
///
/// # Arguments
/// * `request` - The gRPC request containing metadata with authorization header
/// * `auth_token_provider` - Provider to validate the token and extract user ID
///
/// # Returns
/// * `Ok(UserContext)` - Successfully extracted user context with user_id
/// * `Err(Status)` - Authentication failed (missing header, invalid format, invalid token)
pub fn extract_user_context<T>(
    request: &Request<T>,
    auth_token_provider: &dyn AuthTokenProvider,
) -> Result<UserContext, Status> {
    let auth_header = request
        .metadata()
        .get("authorization")
        .ok_or_else(|| Status::unauthenticated("Missing authorization header"))?
        .to_str()
        .map_err(|_| Status::unauthenticated("Invalid authorization header"))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .ok_or_else(|| {
            Status::unauthenticated("Invalid authorization format, expected 'Bearer <token>'")
        })?;

    let user_id = auth_token_provider
        .validate_token(token)
        .map_err(domain_error_to_status)?;

    Ok(UserContext { user_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::MockAuthTokenProvider;
    use crate::domain::DomainError;

    #[test]
    fn test_extract_user_context_success() {
        let mut mock_provider = MockAuthTokenProvider::new();
        mock_provider
            .expect_validate_token()
            .with(mockall::predicate::eq("valid_token"))
            .returning(|_| Ok("user123".to_string()));

        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert("authorization", "Bearer valid_token".parse().unwrap());

        let result = extract_user_context(&request, &mock_provider);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().user_id, "user123");
    }

    #[test]
    fn test_extract_user_context_missing_header() {
        let mock_provider = MockAuthTokenProvider::new();
        let request = Request::new(());

        let result = extract_user_context(&request, &mock_provider);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_extract_user_context_invalid_format() {
        let mock_provider = MockAuthTokenProvider::new();
        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert("authorization", "Basic abc123".parse().unwrap());

        let result = extract_user_context(&request, &mock_provider);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_extract_user_context_invalid_token() {
        let mut mock_provider = MockAuthTokenProvider::new();
        mock_provider
            .expect_validate_token()
            .with(mockall::predicate::eq("invalid_token"))
            .returning(|_| Err(DomainError::InvalidToken("expired".to_string())));

        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert("authorization", "Bearer invalid_token".parse().unwrap());

        let result = extract_user_context(&request, &mock_provider);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_extract_user_context_lowercase_bearer() {
        let mut mock_provider = MockAuthTokenProvider::new();
        mock_provider
            .expect_validate_token()
            .with(mockall::predicate::eq("valid_token"))
            .returning(|_| Ok("user456".to_string()));

        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert("authorization", "bearer valid_token".parse().unwrap());

        let result = extract_user_context(&request, &mock_provider);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().user_id, "user456");
    }
}

use crate::domain::DomainResult;

/// Trait for authentication token operations (JWT access tokens)
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait AuthTokenProvider: Send + Sync {
    /// Generate an access token (JWT) for a user
    fn generate_token(&self, user_id: &str, email: &str) -> DomainResult<String>;

    /// Validate an access token and extract the user ID
    fn validate_token(&self, token: &str) -> DomainResult<String>;

    /// Extract user ID from token without full validation (for logging, etc.)
    fn extract_user_id(&self, token: &str) -> Option<String>;
}

/// Trait for password hashing and verification
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait PasswordService: Send + Sync {
    /// Hash a plaintext password
    fn hash_password(&self, password: &str) -> DomainResult<String>;

    /// Verify a password against a hash
    fn verify_password(&self, password: &str, hash: &str) -> DomainResult<bool>;
}

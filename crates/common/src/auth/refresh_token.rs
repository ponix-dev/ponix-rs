use crate::domain::DomainResult;
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

// =============================================================================
// Domain Types
// =============================================================================

/// Refresh token domain entity
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshToken {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Input for creating a refresh token
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRefreshTokenInput {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Input for finding a refresh token by hash
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetRefreshTokenByHashInput {
    pub token_hash: String,
}

/// Input for deleting a specific refresh token
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRefreshTokenInput {
    pub id: String,
}

/// Input for deleting all refresh tokens for a user
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRefreshTokensByUserInput {
    pub user_id: String,
}

/// Input for atomically rotating a refresh token (delete old + create new)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotateRefreshTokenInput {
    /// ID of the old token to delete
    pub old_token_id: String,
    /// New token to create
    pub new_token: CreateRefreshTokenInput,
}

/// Output from generating a refresh token
#[derive(Debug, Clone)]
pub struct GenerateRefreshTokenOutput {
    /// The raw token to send to the client (before hashing)
    pub raw_token: String,
    /// The hash of the token (for storage)
    pub token_hash: String,
}

// =============================================================================
// Traits
// =============================================================================

/// Repository trait for refresh token storage operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    /// Create a new refresh token
    async fn create_refresh_token(
        &self,
        input: CreateRefreshTokenInput,
    ) -> DomainResult<RefreshToken>;

    /// Get a refresh token by its hash
    async fn get_refresh_token_by_hash(
        &self,
        input: GetRefreshTokenByHashInput,
    ) -> DomainResult<Option<RefreshToken>>;

    /// Delete a specific refresh token
    async fn delete_refresh_token(&self, input: DeleteRefreshTokenInput) -> DomainResult<()>;

    /// Delete all refresh tokens for a user (for logout-all functionality)
    async fn delete_refresh_tokens_by_user(
        &self,
        input: DeleteRefreshTokensByUserInput,
    ) -> DomainResult<()>;

    /// Atomically rotate a refresh token (delete old + create new in a transaction)
    /// This ensures that if the new token creation fails, the old token is not deleted.
    async fn rotate_refresh_token(
        &self,
        input: RotateRefreshTokenInput,
    ) -> DomainResult<RefreshToken>;
}

/// Trait for refresh token generation and hashing
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait RefreshTokenProvider: Send + Sync {
    /// Generate a refresh token (returns raw token for client and hash for storage)
    fn generate_refresh_token(&self) -> GenerateRefreshTokenOutput;

    /// Hash a raw refresh token (for lookup/validation)
    fn hash_refresh_token(&self, raw_token: &str) -> String;
}

// =============================================================================
// Implementation
// =============================================================================

/// Cryptographic implementation of RefreshTokenProvider
///
/// Generates opaque refresh tokens using cryptographically secure random bytes
/// and hashes them using SHA-256 for secure storage.
pub struct CryptoRefreshTokenProvider;

impl CryptoRefreshTokenProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CryptoRefreshTokenProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl RefreshTokenProvider for CryptoRefreshTokenProvider {
    fn generate_refresh_token(&self) -> GenerateRefreshTokenOutput {
        // Generate 32 bytes of cryptographically secure random data
        let mut random_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut random_bytes);

        // Encode as URL-safe base64 (no padding)
        let raw_token = URL_SAFE_NO_PAD.encode(random_bytes);

        // Hash the token for storage
        let token_hash = self.hash_refresh_token(&raw_token);

        GenerateRefreshTokenOutput {
            raw_token,
            token_hash,
        }
    }

    fn hash_refresh_token(&self, raw_token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(raw_token.as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_refresh_token_success() {
        let provider = CryptoRefreshTokenProvider::new();
        let output = provider.generate_refresh_token();

        // Raw token should be base64-encoded 32 bytes (43 chars without padding)
        assert!(!output.raw_token.is_empty());
        assert!(output.raw_token.len() >= 40);

        // Token hash should be hex-encoded SHA256 (64 chars)
        assert_eq!(output.token_hash.len(), 64);
    }

    #[test]
    fn test_generate_refresh_token_unique() {
        let provider = CryptoRefreshTokenProvider::new();

        let token1 = provider.generate_refresh_token();
        let token2 = provider.generate_refresh_token();

        // Each token should be unique
        assert_ne!(token1.raw_token, token2.raw_token);
        assert_ne!(token1.token_hash, token2.token_hash);
    }

    #[test]
    fn test_hash_refresh_token_consistent() {
        let provider = CryptoRefreshTokenProvider::new();
        let raw_token = "test-token-12345";

        let hash1 = provider.hash_refresh_token(raw_token);
        let hash2 = provider.hash_refresh_token(raw_token);

        // Same token should produce same hash
        assert_eq!(hash1, hash2);

        // Hash should be 64 hex characters (SHA256)
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_hash_matches_generated_token() {
        let provider = CryptoRefreshTokenProvider::new();
        let output = provider.generate_refresh_token();

        // Re-hashing the raw token should produce the same hash
        let rehashed = provider.hash_refresh_token(&output.raw_token);
        assert_eq!(rehashed, output.token_hash);
    }

    #[test]
    fn test_default_implementation() {
        let provider = CryptoRefreshTokenProvider::default();
        let output = provider.generate_refresh_token();
        assert!(!output.raw_token.is_empty());
    }
}

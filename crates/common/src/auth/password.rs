use crate::auth::PasswordService;
use crate::domain::{DomainError, DomainResult};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

/// Argon2-based implementation of PasswordService
#[derive(Default)]
pub struct Argon2PasswordService;

impl Argon2PasswordService {
    pub fn new() -> Self {
        Self
    }
}

impl PasswordService for Argon2PasswordService {
    fn hash_password(&self, password: &str) -> DomainResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| DomainError::PasswordHashingError(e.to_string()))
    }

    fn verify_password(&self, password: &str, hash: &str) -> DomainResult<bool> {
        let parsed_hash = argon2::PasswordHash::new(hash)
            .map_err(|e| DomainError::PasswordHashingError(e.to_string()))?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_password_success() {
        let service = Argon2PasswordService::new();
        let hash = service.hash_password("secure-password-123");
        assert!(hash.is_ok());

        let hash_str = hash.unwrap();
        assert!(!hash_str.is_empty());
        assert!(hash_str.starts_with("$argon2"));
    }

    #[test]
    fn test_hash_password_different_each_time() {
        let service = Argon2PasswordService::new();
        let hash1 = service.hash_password("same-password").unwrap();
        let hash2 = service.hash_password("same-password").unwrap();

        // Different salts should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_verify_password_correct() {
        let service = Argon2PasswordService::new();
        let hash = service.hash_password("secure-password-123").unwrap();

        let result = service.verify_password("secure-password-123", &hash);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_verify_password_incorrect() {
        let service = Argon2PasswordService::new();
        let hash = service.hash_password("correct-password").unwrap();

        let result = service.verify_password("wrong-password", &hash);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_verify_password_invalid_hash() {
        let service = Argon2PasswordService::new();
        let result = service.verify_password("any-password", "invalid-hash");
        assert!(matches!(result, Err(DomainError::PasswordHashingError(_))));
    }
}

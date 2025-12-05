use crate::auth::{AuthTokenProvider, JwtConfig};
use crate::domain::{DomainError, DomainResult};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,  // user_id
    pub email: String,
    pub exp: usize,   // expiration timestamp
    pub iat: usize,   // issued at timestamp
}

/// JWT-based implementation of AuthTokenProvider
pub struct JwtAuthTokenProvider {
    config: JwtConfig,
}

impl JwtAuthTokenProvider {
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }
}

impl AuthTokenProvider for JwtAuthTokenProvider {
    fn generate_token(&self, user_id: &str, email: &str) -> DomainResult<String> {
        let now = chrono::Utc::now();
        let exp = now + chrono::Duration::hours(self.config.expiration_hours as i64);

        let claims = JwtClaims {
            sub: user_id.to_string(),
            email: email.to_string(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.config.secret.as_bytes()),
        )
        .map_err(|e| DomainError::RepositoryError(anyhow::anyhow!("JWT encoding error: {}", e)))
    }

    fn validate_token(&self, token: &str) -> DomainResult<String> {
        let token_data = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.config.secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| DomainError::InvalidToken(e.to_string()))?;

        Ok(token_data.claims.sub)
    }

    fn extract_user_id(&self, token: &str) -> Option<String> {
        // Decode without validation for logging purposes
        let mut validation = Validation::default();
        validation.insecure_disable_signature_validation();
        validation.validate_exp = false;

        decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(&[]),
            &validation,
        )
        .ok()
        .map(|data| data.claims.sub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        JwtConfig::new("test-secret-key".to_string(), 24)
    }

    #[test]
    fn test_generate_token_success() {
        let provider = JwtAuthTokenProvider::new(test_config());
        let token = provider.generate_token("user-123", "test@example.com");
        assert!(token.is_ok());
        assert!(!token.unwrap().is_empty());
    }

    #[test]
    fn test_validate_token_success() {
        let provider = JwtAuthTokenProvider::new(test_config());
        let token = provider.generate_token("user-123", "test@example.com").unwrap();

        let user_id = provider.validate_token(&token);
        assert!(user_id.is_ok());
        assert_eq!(user_id.unwrap(), "user-123");
    }

    #[test]
    fn test_validate_token_invalid() {
        let provider = JwtAuthTokenProvider::new(test_config());
        let result = provider.validate_token("invalid-token");
        assert!(matches!(result, Err(DomainError::InvalidToken(_))));
    }

    #[test]
    fn test_validate_token_wrong_secret() {
        let provider1 = JwtAuthTokenProvider::new(test_config());
        let provider2 = JwtAuthTokenProvider::new(JwtConfig::new("different-secret".to_string(), 24));

        let token = provider1.generate_token("user-123", "test@example.com").unwrap();
        let result = provider2.validate_token(&token);
        assert!(matches!(result, Err(DomainError::InvalidToken(_))));
    }

    #[test]
    fn test_extract_user_id_success() {
        let provider = JwtAuthTokenProvider::new(test_config());
        let token = provider.generate_token("user-123", "test@example.com").unwrap();

        let user_id = provider.extract_user_id(&token);
        assert_eq!(user_id, Some("user-123".to_string()));
    }

    #[test]
    fn test_extract_user_id_invalid_token() {
        let provider = JwtAuthTokenProvider::new(test_config());
        let user_id = provider.extract_user_id("invalid-token");
        assert_eq!(user_id, None);
    }
}

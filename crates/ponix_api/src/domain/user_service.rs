use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use common::domain::{
    DomainError, DomainResult, GetUserByEmailInput, GetUserInput, LoginUserInput, LoginUserOutput,
    RegisterUserInput, RegisterUserInputWithId, User, UserRepository,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, instrument};

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,  // user_id
    pub email: String,
    pub exp: usize,   // expiration timestamp
    pub iat: usize,   // issued at timestamp
}

/// Configuration for JWT token generation
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub expiration_hours: u64,
}

/// Domain service for user registration and management
pub struct UserService {
    user_repository: Arc<dyn UserRepository>,
    jwt_config: JwtConfig,
}

impl UserService {
    pub fn new(user_repository: Arc<dyn UserRepository>, jwt_config: JwtConfig) -> Self {
        Self {
            user_repository,
            jwt_config,
        }
    }

    /// Register a new user with hashed password
    #[instrument(skip(self, input), fields(email = %input.email))]
    pub async fn register_user(&self, input: RegisterUserInput) -> DomainResult<User> {
        debug!(email = %input.email, "registering new user");

        // Validate email format (basic validation)
        if !Self::is_valid_email(&input.email) {
            return Err(DomainError::InvalidEmail(
                "Invalid email format".to_string(),
            ));
        }

        // Validate password (minimum length)
        if input.password.len() < 8 {
            return Err(DomainError::InvalidPassword(
                "Password must be at least 8 characters".to_string(),
            ));
        }

        // Validate name is not empty
        if input.name.trim().is_empty() {
            return Err(DomainError::InvalidUserName(
                "Name cannot be empty".to_string(),
            ));
        }

        // Hash the password using Argon2
        let password_hash = Self::hash_password(&input.password)?;

        // Generate unique user ID using xid
        let user_id = xid::new().to_string();

        debug!(user_id = %user_id, email = %input.email, "registering user with hashed password");

        let repo_input = RegisterUserInputWithId {
            id: user_id,
            email: input.email,
            password_hash,
            name: input.name,
        };

        let user = self.user_repository.register_user(repo_input).await?;

        debug!(user_id = %user.id, "user registered successfully");
        Ok(user)
    }

    /// Get user by ID
    #[instrument(skip(self, input), fields(user_id = %input.user_id))]
    pub async fn get_user(&self, input: GetUserInput) -> DomainResult<User> {
        debug!(user_id = %input.user_id, "getting user");

        if input.user_id.is_empty() {
            return Err(DomainError::InvalidUserId(
                "User ID cannot be empty".to_string(),
            ));
        }

        let user = self
            .user_repository
            .get_user(input.clone())
            .await?
            .ok_or_else(|| DomainError::UserNotFound(input.user_id.clone()))?;

        Ok(user)
    }

    /// Hash a password using Argon2
    fn hash_password(password: &str) -> DomainResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| DomainError::PasswordHashingError(e.to_string()))
    }

    /// Basic email validation
    fn is_valid_email(email: &str) -> bool {
        // Simple validation: contains @ and at least one . after @
        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() != 2 {
            return false;
        }
        let domain = parts[1];
        domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
    }

    /// Login user and generate JWT token
    #[instrument(skip(self, input), fields(email = %input.email))]
    pub async fn login_user(&self, input: LoginUserInput) -> DomainResult<LoginUserOutput> {
        debug!(email = %input.email, "attempting user login");

        // Validate email format
        if !Self::is_valid_email(&input.email) {
            return Err(DomainError::InvalidCredentials);
        }

        // Look up user by email
        let user = self
            .user_repository
            .get_user_by_email(GetUserByEmailInput {
                email: input.email.clone(),
            })
            .await?
            .ok_or(DomainError::InvalidCredentials)?;

        // Verify password
        if !Self::verify_password(&input.password, &user.password_hash)? {
            return Err(DomainError::InvalidCredentials);
        }

        // Generate JWT token
        let token = self.generate_jwt(&user)?;

        debug!(user_id = %user.id, "user login successful");

        Ok(LoginUserOutput { token })
    }

    /// Verify a password against its hash using Argon2
    fn verify_password(password: &str, hash: &str) -> DomainResult<bool> {
        let parsed_hash = argon2::PasswordHash::new(hash)
            .map_err(|e| DomainError::PasswordHashingError(e.to_string()))?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Generate a JWT token for a user
    fn generate_jwt(&self, user: &User) -> DomainResult<String> {
        let now = chrono::Utc::now();
        let exp = now + chrono::Duration::hours(self.jwt_config.expiration_hours as i64);

        let claims = JwtClaims {
            sub: user.id.clone(),
            email: user.email.clone(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_config.secret.as_bytes()),
        )
        .map_err(|e| DomainError::RepositoryError(anyhow::anyhow!("JWT encoding error: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::MockUserRepository;

    fn test_jwt_config() -> JwtConfig {
        JwtConfig {
            secret: "test-secret-key".to_string(),
            expiration_hours: 24,
        }
    }

    #[tokio::test]
    async fn test_register_user_success() {
        let mut mock_repo = MockUserRepository::new();

        mock_repo
            .expect_register_user()
            .withf(|input: &RegisterUserInputWithId| {
                !input.id.is_empty()
                    && input.email == "test@example.com"
                    && !input.password_hash.is_empty()
                    && input.name == "John Doe"
            })
            .times(1)
            .return_once(move |input| {
                Ok(User {
                    id: input.id,
                    email: input.email,
                    password_hash: input.password_hash,
                    name: input.name,
                    created_at: Some(chrono::Utc::now()),
                    updated_at: Some(chrono::Utc::now()),
                })
            });

        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());
        let input = RegisterUserInput {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
            name: "John Doe".to_string(),
        };

        let result = service.register_user(input).await;
        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.name, "John Doe");
    }

    #[tokio::test]
    async fn test_register_user_invalid_email() {
        let mock_repo = MockUserRepository::new();
        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());

        let input = RegisterUserInput {
            email: "invalid-email".to_string(),
            password: "securepassword123".to_string(),
            name: "John Doe".to_string(),
        };

        let result = service.register_user(input).await;
        assert!(matches!(result, Err(DomainError::InvalidEmail(_))));
    }

    #[tokio::test]
    async fn test_register_user_short_password() {
        let mock_repo = MockUserRepository::new();
        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());

        let input = RegisterUserInput {
            email: "test@example.com".to_string(),
            password: "short".to_string(),
            name: "John Doe".to_string(),
        };

        let result = service.register_user(input).await;
        assert!(matches!(result, Err(DomainError::InvalidPassword(_))));
    }

    #[tokio::test]
    async fn test_register_user_empty_name() {
        let mock_repo = MockUserRepository::new();
        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());

        let input = RegisterUserInput {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
            name: "".to_string(),
        };

        let result = service.register_user(input).await;
        assert!(matches!(result, Err(DomainError::InvalidUserName(_))));
    }

    #[tokio::test]
    async fn test_get_user_empty_id() {
        let mock_repo = MockUserRepository::new();
        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());

        let input = GetUserInput {
            user_id: "".to_string(),
        };

        let result = service.get_user(input).await;
        assert!(matches!(result, Err(DomainError::InvalidUserId(_))));
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        let mut mock_repo = MockUserRepository::new();

        mock_repo
            .expect_get_user()
            .times(1)
            .return_once(|_| Ok(None));

        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());
        let input = GetUserInput {
            user_id: "nonexistent".to_string(),
        };

        let result = service.get_user(input).await;
        assert!(matches!(result, Err(DomainError::UserNotFound(_))));
    }

    #[tokio::test]
    async fn test_login_user_success() {
        let mut mock_repo = MockUserRepository::new();

        // Create a valid password hash for "securepassword123"
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(b"securepassword123", &salt)
            .unwrap()
            .to_string();

        let stored_user = User {
            id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            password_hash: password_hash.clone(),
            name: "John Doe".to_string(),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        mock_repo
            .expect_get_user_by_email()
            .withf(|input: &GetUserByEmailInput| input.email == "test@example.com")
            .times(1)
            .return_once(move |_| Ok(Some(stored_user)));

        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());
        let input = LoginUserInput {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
        };

        let result = service.login_user(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.token.is_empty());
    }

    #[tokio::test]
    async fn test_login_user_not_found() {
        let mut mock_repo = MockUserRepository::new();

        mock_repo
            .expect_get_user_by_email()
            .times(1)
            .return_once(|_| Ok(None));

        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());
        let input = LoginUserInput {
            email: "nonexistent@example.com".to_string(),
            password: "somepassword123".to_string(),
        };

        let result = service.login_user(input).await;
        assert!(matches!(result, Err(DomainError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn test_login_user_wrong_password() {
        let mut mock_repo = MockUserRepository::new();

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(b"correctpassword", &salt)
            .unwrap()
            .to_string();

        let stored_user = User {
            id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            password_hash,
            name: "John Doe".to_string(),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        mock_repo
            .expect_get_user_by_email()
            .times(1)
            .return_once(move |_| Ok(Some(stored_user)));

        let service = UserService::new(Arc::new(mock_repo), test_jwt_config());
        let input = LoginUserInput {
            email: "test@example.com".to_string(),
            password: "wrongpassword123".to_string(),
        };

        let result = service.login_user(input).await;
        assert!(matches!(result, Err(DomainError::InvalidCredentials)));
    }
}

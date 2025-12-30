use common::auth::{
    AuthTokenProvider, CreateRefreshTokenInput, DeleteRefreshTokenInput,
    GetRefreshTokenByHashInput, LoginUserInput, LoginUserOutput, LogoutInput, PasswordService,
    RefreshTokenInput, RefreshTokenOutput, RefreshTokenProvider, RefreshTokenRepository,
    RotateRefreshTokenInput,
};
use common::domain::{
    DomainError, DomainResult, GetUserByEmailRepoInput, GetUserRepoInput,
    RegisterUserRepoInputWithId, User, UserRepository,
};
use garde::Validate;
use std::sync::Arc;
use tracing::{debug, instrument};

// Service Request Types
// Note: RegisterUserRequest doesn't have user_id because the user is being created
#[derive(Debug, Clone, Validate)]
pub struct RegisterUserRequest {
    #[garde(email)]
    pub email: String,
    #[garde(length(min = 8))]
    pub password: String,
    #[garde(length(min = 1))]
    pub name: String,
}

#[derive(Debug, Clone, Validate)]
pub struct GetUserRequest {
    /// The ID of the user making the request (from JWT)
    #[garde(length(min = 1))]
    pub requesting_user_id: String,
    /// The ID of the user to fetch
    #[garde(length(min = 1))]
    pub user_id: String,
}

/// Domain service for user registration and management
pub struct UserService {
    user_repository: Arc<dyn UserRepository>,
    refresh_token_repository: Arc<dyn RefreshTokenRepository>,
    auth_token_provider: Arc<dyn AuthTokenProvider>,
    refresh_token_provider: Arc<dyn RefreshTokenProvider>,
    password_service: Arc<dyn PasswordService>,
    refresh_token_expiration_days: u64,
}

impl UserService {
    pub fn new(
        user_repository: Arc<dyn UserRepository>,
        refresh_token_repository: Arc<dyn RefreshTokenRepository>,
        auth_token_provider: Arc<dyn AuthTokenProvider>,
        refresh_token_provider: Arc<dyn RefreshTokenProvider>,
        password_service: Arc<dyn PasswordService>,
        refresh_token_expiration_days: u64,
    ) -> Self {
        Self {
            user_repository,
            refresh_token_repository,
            auth_token_provider,
            refresh_token_provider,
            password_service,
            refresh_token_expiration_days,
        }
    }

    /// Register a new user with hashed password
    #[instrument(skip(self, request), fields(email = %request.email))]
    pub async fn register_user(&self, request: RegisterUserRequest) -> DomainResult<User> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        debug!(email = %request.email, "registering new user");

        // Hash the password using injected password service
        let password_hash = self.password_service.hash_password(&request.password)?;

        // Generate unique user ID using xid
        let user_id = xid::new().to_string();

        debug!(user_id = %user_id, email = %request.email, "registering user with hashed password");

        let repo_input = RegisterUserRepoInputWithId {
            id: user_id,
            email: request.email,
            password_hash,
            name: request.name,
        };

        let user = self.user_repository.register_user(repo_input).await?;

        debug!(user_id = %user.id, "user registered successfully");
        Ok(user)
    }

    /// Get user by ID
    #[instrument(skip(self, request), fields(requesting_user_id = %request.requesting_user_id, user_id = %request.user_id))]
    pub async fn get_user(&self, request: GetUserRequest) -> DomainResult<User> {
        // Validate request using garde
        common::garde::validate_struct(&request)?;

        debug!(requesting_user_id = %request.requesting_user_id, user_id = %request.user_id, "getting user");

        // Users can only access their own profile
        if request.requesting_user_id != request.user_id {
            return Err(DomainError::PermissionDenied(
                "You can only access your own user profile".to_string(),
            ));
        }

        let repo_input = GetUserRepoInput {
            user_id: request.user_id.clone(),
        };

        let user = self
            .user_repository
            .get_user(repo_input)
            .await?
            .ok_or_else(|| DomainError::UserNotFound(request.user_id))?;

        Ok(user)
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

    /// Login user and generate access + refresh tokens
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
            .get_user_by_email(GetUserByEmailRepoInput {
                email: input.email.clone(),
            })
            .await?
            .ok_or(DomainError::InvalidCredentials)?;

        // Verify password using injected password service
        if !self
            .password_service
            .verify_password(&input.password, &user.password_hash)?
        {
            return Err(DomainError::InvalidCredentials);
        }

        // Generate access token
        let access_token = self
            .auth_token_provider
            .generate_token(&user.id, &user.email)?;

        // Generate refresh token
        let refresh_output = self.refresh_token_provider.generate_refresh_token();

        // Calculate expiration
        let expires_at =
            chrono::Utc::now() + chrono::Duration::days(self.refresh_token_expiration_days as i64);

        // Store the hashed refresh token (multiple sessions allowed)
        let token_id = xid::new().to_string();
        self.refresh_token_repository
            .create_refresh_token(CreateRefreshTokenInput {
                id: token_id,
                user_id: user.id.clone(),
                token_hash: refresh_output.token_hash,
                expires_at,
            })
            .await?;

        debug!(user_id = %user.id, "user login successful with refresh token");

        Ok(LoginUserOutput {
            access_token,
            refresh_token: refresh_output.raw_token,
        })
    }

    /// Refresh access token using a valid refresh token
    #[instrument(skip(self, input))]
    pub async fn refresh_token(
        &self,
        input: RefreshTokenInput,
    ) -> DomainResult<RefreshTokenOutput> {
        debug!("attempting token refresh");

        // Hash the incoming token to look up in database
        let token_hash = self
            .refresh_token_provider
            .hash_refresh_token(&input.refresh_token);

        // Look up the refresh token
        let stored_token = self
            .refresh_token_repository
            .get_refresh_token_by_hash(GetRefreshTokenByHashInput { token_hash })
            .await?
            .ok_or(DomainError::RefreshTokenNotFound)?;

        // Check if expired
        if stored_token.expires_at < chrono::Utc::now() {
            // Delete the expired token
            self.refresh_token_repository
                .delete_refresh_token(DeleteRefreshTokenInput {
                    id: stored_token.id,
                })
                .await?;
            return Err(DomainError::RefreshTokenExpired);
        }

        // Get the user to generate new access token
        let user = self
            .user_repository
            .get_user(GetUserRepoInput {
                user_id: stored_token.user_id.clone(),
            })
            .await?
            .ok_or(DomainError::UserNotFound(stored_token.user_id.clone()))?;

        // Generate new access token
        let access_token = self
            .auth_token_provider
            .generate_token(&user.id, &user.email)?;

        // Generate new refresh token
        let refresh_output = self.refresh_token_provider.generate_refresh_token();

        // Calculate expiration
        let expires_at =
            chrono::Utc::now() + chrono::Duration::days(self.refresh_token_expiration_days as i64);

        // Atomically rotate the refresh token (delete old + create new in a transaction)
        // This ensures that if the new token creation fails, the old token is not deleted
        let token_id = xid::new().to_string();
        self.refresh_token_repository
            .rotate_refresh_token(RotateRefreshTokenInput {
                old_token_id: stored_token.id,
                new_token: CreateRefreshTokenInput {
                    id: token_id,
                    user_id: user.id.clone(),
                    token_hash: refresh_output.token_hash,
                    expires_at,
                },
            })
            .await?;

        debug!(user_id = %user.id, "token refresh successful");

        Ok(RefreshTokenOutput {
            access_token,
            refresh_token: refresh_output.raw_token,
        })
    }

    /// Logout user by invalidating refresh token
    #[instrument(skip(self, input))]
    pub async fn logout(&self, input: LogoutInput) -> DomainResult<()> {
        debug!("attempting logout");

        // Hash the incoming token to look up in database
        let token_hash = self
            .refresh_token_provider
            .hash_refresh_token(&input.refresh_token);

        // Look up the refresh token
        let stored_token = self
            .refresh_token_repository
            .get_refresh_token_by_hash(GetRefreshTokenByHashInput { token_hash })
            .await?;

        // Delete if found (don't error if not found - idempotent logout)
        if let Some(token) = stored_token {
            self.refresh_token_repository
                .delete_refresh_token(DeleteRefreshTokenInput { id: token.id })
                .await?;
            debug!(user_id = %token.user_id, "logout successful");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::auth::{
        GenerateRefreshTokenOutput, MockAuthTokenProvider, MockPasswordService,
        MockRefreshTokenProvider, MockRefreshTokenRepository, RefreshToken,
    };
    use common::domain::MockUserRepository;

    fn create_test_service(
        mock_user_repo: MockUserRepository,
        mock_refresh_repo: MockRefreshTokenRepository,
        mock_auth: MockAuthTokenProvider,
        mock_refresh_provider: MockRefreshTokenProvider,
        mock_password: MockPasswordService,
    ) -> UserService {
        UserService::new(
            Arc::new(mock_user_repo),
            Arc::new(mock_refresh_repo),
            Arc::new(mock_auth),
            Arc::new(mock_refresh_provider),
            Arc::new(mock_password),
            7, // 7 days expiration
        )
    }

    #[tokio::test]
    async fn test_register_user_success() {
        let mut mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mut mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        mock_password
            .expect_hash_password()
            .withf(|password: &str| password == "securepassword123")
            .times(1)
            .return_once(|_| Ok("hashed-password".to_string()));

        mock_repo
            .expect_register_user()
            .withf(|input: &RegisterUserRepoInputWithId| {
                !input.id.is_empty()
                    && input.email == "test@example.com"
                    && input.password_hash == "hashed-password"
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

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let request = RegisterUserRequest {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
            name: "John Doe".to_string(),
        };

        let result = service.register_user(request).await;
        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.name, "John Doe");
    }

    #[tokio::test]
    async fn test_register_user_invalid_email() {
        let mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );

        let request = RegisterUserRequest {
            email: "invalid-email".to_string(),
            password: "securepassword123".to_string(),
            name: "John Doe".to_string(),
        };

        let result = service.register_user(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_register_user_short_password() {
        let mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );

        let request = RegisterUserRequest {
            email: "test@example.com".to_string(),
            password: "short".to_string(),
            name: "John Doe".to_string(),
        };

        let result = service.register_user(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_register_user_empty_name() {
        let mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );

        let request = RegisterUserRequest {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
            name: "".to_string(),
        };

        let result = service.register_user(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_get_user_empty_id() {
        let mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );

        let request = GetUserRequest {
            requesting_user_id: "user-123".to_string(),
            user_id: "".to_string(),
        };

        let result = service.get_user(request).await;
        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        let mut mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        mock_repo
            .expect_get_user()
            .times(1)
            .return_once(|_| Ok(None));

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let request = GetUserRequest {
            requesting_user_id: "nonexistent".to_string(),
            user_id: "nonexistent".to_string(),
        };

        let result = service.get_user(request).await;
        assert!(matches!(result, Err(DomainError::UserNotFound(_))));
    }

    #[tokio::test]
    async fn test_get_user_permission_denied() {
        let mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let request = GetUserRequest {
            requesting_user_id: "user-123".to_string(),
            user_id: "different-user".to_string(),
        };

        let result = service.get_user(request).await;
        assert!(matches!(result, Err(DomainError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn test_login_user_success() {
        let mut mock_repo = MockUserRepository::new();
        let mut mock_refresh_repo = MockRefreshTokenRepository::new();
        let mut mock_password = MockPasswordService::new();
        let mut mock_auth = MockAuthTokenProvider::new();
        let mut mock_refresh_provider = MockRefreshTokenProvider::new();

        let stored_user = User {
            id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            password_hash: "hashed-password".to_string(),
            name: "John Doe".to_string(),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        mock_repo
            .expect_get_user_by_email()
            .withf(|input: &GetUserByEmailRepoInput| input.email == "test@example.com")
            .times(1)
            .return_once(move |_| Ok(Some(stored_user)));

        mock_password
            .expect_verify_password()
            .withf(|password: &str, hash: &str| {
                password == "securepassword123" && hash == "hashed-password"
            })
            .times(1)
            .return_once(|_, _| Ok(true));

        mock_auth
            .expect_generate_token()
            .withf(|user_id: &str, email: &str| {
                user_id == "user-123" && email == "test@example.com"
            })
            .times(1)
            .return_once(|_, _| Ok("jwt-token-123".to_string()));

        mock_refresh_provider
            .expect_generate_refresh_token()
            .times(1)
            .return_once(|| GenerateRefreshTokenOutput {
                raw_token: "raw-refresh-token".to_string(),
                token_hash: "hashed-refresh-token".to_string(),
            });

        mock_refresh_repo
            .expect_create_refresh_token()
            .times(1)
            .return_once(|input| {
                Ok(RefreshToken {
                    id: input.id,
                    user_id: input.user_id,
                    token_hash: input.token_hash,
                    expires_at: input.expires_at,
                    created_at: Some(chrono::Utc::now()),
                })
            });

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let input = LoginUserInput {
            email: "test@example.com".to_string(),
            password: "securepassword123".to_string(),
        };

        let result = service.login_user(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.access_token, "jwt-token-123");
        assert_eq!(output.refresh_token, "raw-refresh-token");
    }

    #[tokio::test]
    async fn test_login_user_not_found() {
        let mut mock_repo = MockUserRepository::new();
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        mock_repo
            .expect_get_user_by_email()
            .times(1)
            .return_once(|_| Ok(None));

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
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
        let mock_refresh_repo = MockRefreshTokenRepository::new();
        let mut mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mock_refresh_provider = MockRefreshTokenProvider::new();

        let stored_user = User {
            id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            password_hash: "hashed-password".to_string(),
            name: "John Doe".to_string(),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        mock_repo
            .expect_get_user_by_email()
            .times(1)
            .return_once(move |_| Ok(Some(stored_user)));

        mock_password
            .expect_verify_password()
            .times(1)
            .return_once(|_, _| Ok(false));

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let input = LoginUserInput {
            email: "test@example.com".to_string(),
            password: "wrongpassword123".to_string(),
        };

        let result = service.login_user(input).await;
        assert!(matches!(result, Err(DomainError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn test_refresh_token_success() {
        let mut mock_repo = MockUserRepository::new();
        let mut mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mut mock_auth = MockAuthTokenProvider::new();
        let mut mock_refresh_provider = MockRefreshTokenProvider::new();

        let stored_user = User {
            id: "user-123".to_string(),
            email: "test@example.com".to_string(),
            password_hash: "hashed-password".to_string(),
            name: "John Doe".to_string(),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        mock_refresh_provider
            .expect_hash_refresh_token()
            .withf(|token: &str| token == "old-refresh-token")
            .times(1)
            .return_once(|_| "hashed-old-token".to_string());

        mock_refresh_repo
            .expect_get_refresh_token_by_hash()
            .times(1)
            .return_once(|_| {
                Ok(Some(RefreshToken {
                    id: "token-id".to_string(),
                    user_id: "user-123".to_string(),
                    token_hash: "hashed-old-token".to_string(),
                    expires_at: chrono::Utc::now() + chrono::Duration::days(1),
                    created_at: Some(chrono::Utc::now()),
                }))
            });

        mock_repo
            .expect_get_user()
            .times(1)
            .return_once(move |_| Ok(Some(stored_user)));

        mock_auth
            .expect_generate_token()
            .times(1)
            .return_once(|_, _| Ok("new-jwt-token".to_string()));

        mock_refresh_provider
            .expect_generate_refresh_token()
            .times(1)
            .return_once(|| GenerateRefreshTokenOutput {
                raw_token: "new-refresh-token".to_string(),
                token_hash: "hashed-new-token".to_string(),
            });

        mock_refresh_repo
            .expect_rotate_refresh_token()
            .withf(|input| input.old_token_id == "token-id")
            .times(1)
            .return_once(|input| {
                Ok(RefreshToken {
                    id: input.new_token.id,
                    user_id: input.new_token.user_id,
                    token_hash: input.new_token.token_hash,
                    expires_at: input.new_token.expires_at,
                    created_at: Some(chrono::Utc::now()),
                })
            });

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let input = RefreshTokenInput {
            refresh_token: "old-refresh-token".to_string(),
        };

        let result = service.refresh_token(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.access_token, "new-jwt-token");
        assert_eq!(output.refresh_token, "new-refresh-token");
    }

    #[tokio::test]
    async fn test_refresh_token_not_found() {
        let mock_repo = MockUserRepository::new();
        let mut mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mut mock_refresh_provider = MockRefreshTokenProvider::new();

        mock_refresh_provider
            .expect_hash_refresh_token()
            .times(1)
            .return_once(|_| "hashed-token".to_string());

        mock_refresh_repo
            .expect_get_refresh_token_by_hash()
            .times(1)
            .return_once(|_| Ok(None));

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let input = RefreshTokenInput {
            refresh_token: "invalid-token".to_string(),
        };

        let result = service.refresh_token(input).await;
        assert!(matches!(result, Err(DomainError::RefreshTokenNotFound)));
    }

    #[tokio::test]
    async fn test_refresh_token_expired() {
        let mock_repo = MockUserRepository::new();
        let mut mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mut mock_refresh_provider = MockRefreshTokenProvider::new();

        mock_refresh_provider
            .expect_hash_refresh_token()
            .times(1)
            .return_once(|_| "hashed-token".to_string());

        mock_refresh_repo
            .expect_get_refresh_token_by_hash()
            .times(1)
            .return_once(|_| {
                Ok(Some(RefreshToken {
                    id: "token-id".to_string(),
                    user_id: "user-123".to_string(),
                    token_hash: "hashed-token".to_string(),
                    expires_at: chrono::Utc::now() - chrono::Duration::days(1), // Expired
                    created_at: Some(chrono::Utc::now()),
                }))
            });

        mock_refresh_repo
            .expect_delete_refresh_token()
            .times(1)
            .return_once(|_| Ok(()));

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let input = RefreshTokenInput {
            refresh_token: "expired-token".to_string(),
        };

        let result = service.refresh_token(input).await;
        assert!(matches!(result, Err(DomainError::RefreshTokenExpired)));
    }

    #[tokio::test]
    async fn test_logout_success() {
        let mock_repo = MockUserRepository::new();
        let mut mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mut mock_refresh_provider = MockRefreshTokenProvider::new();

        mock_refresh_provider
            .expect_hash_refresh_token()
            .times(1)
            .return_once(|_| "hashed-token".to_string());

        mock_refresh_repo
            .expect_get_refresh_token_by_hash()
            .times(1)
            .return_once(|_| {
                Ok(Some(RefreshToken {
                    id: "token-id".to_string(),
                    user_id: "user-123".to_string(),
                    token_hash: "hashed-token".to_string(),
                    expires_at: chrono::Utc::now() + chrono::Duration::days(1),
                    created_at: Some(chrono::Utc::now()),
                }))
            });

        mock_refresh_repo
            .expect_delete_refresh_token()
            .times(1)
            .return_once(|_| Ok(()));

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let input = LogoutInput {
            refresh_token: "valid-token".to_string(),
        };

        let result = service.logout(input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logout_token_not_found_is_ok() {
        let mock_repo = MockUserRepository::new();
        let mut mock_refresh_repo = MockRefreshTokenRepository::new();
        let mock_password = MockPasswordService::new();
        let mock_auth = MockAuthTokenProvider::new();
        let mut mock_refresh_provider = MockRefreshTokenProvider::new();

        mock_refresh_provider
            .expect_hash_refresh_token()
            .times(1)
            .return_once(|_| "hashed-token".to_string());

        mock_refresh_repo
            .expect_get_refresh_token_by_hash()
            .times(1)
            .return_once(|_| Ok(None));

        let service = create_test_service(
            mock_repo,
            mock_refresh_repo,
            mock_auth,
            mock_refresh_provider,
            mock_password,
        );
        let input = LogoutInput {
            refresh_token: "invalid-token".to_string(),
        };

        // Logout should be idempotent - not finding a token is OK
        let result = service.logout(input).await;
        assert!(result.is_ok());
    }
}

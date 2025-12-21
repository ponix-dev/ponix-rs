use crate::domain::result::DomainResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// User domain entity
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Repository input for registering a user (no ID, plaintext password)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterUserRepoInput {
    pub email: String,
    pub password: String, // Plaintext - will be hashed by domain service
    pub name: String,
}

/// Repository input with generated ID and hashed password
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterUserRepoInputWithId {
    pub id: String,
    pub email: String,
    pub password_hash: String, // Already hashed
    pub name: String,
}

/// Repository input for getting a user by ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetUserRepoInput {
    pub user_id: String,
}

/// Repository input for getting a user by email
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetUserByEmailRepoInput {
    pub email: String,
}

/// Repository trait for user storage operations
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Register a new user (id and password_hash already generated/hashed by domain service)
    async fn register_user(&self, input: RegisterUserRepoInputWithId) -> DomainResult<User>;

    /// Get a user by ID
    async fn get_user(&self, input: GetUserRepoInput) -> DomainResult<Option<User>>;

    /// Get a user by email
    async fn get_user_by_email(&self, input: GetUserByEmailRepoInput)
        -> DomainResult<Option<User>>;
}

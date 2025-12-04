use crate::domain::{
    DomainError, DomainResult, GetUserByEmailInput, GetUserInput, RegisterUserInputWithId, User,
    UserRepository,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// User row for PostgreSQL storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            id: row.id,
            email: row.email,
            password_hash: row.password_hash,
            name: row.name,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// PostgreSQL implementation of UserRepository trait
#[derive(Clone)]
pub struct PostgresUserRepository {
    client: PostgresClient,
}

impl PostgresUserRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    #[instrument(skip(self, input), fields(user_id = %input.id, email = %input.email))]
    async fn register_user(&self, input: RegisterUserInputWithId) -> DomainResult<User> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let result = conn
            .execute(
                "INSERT INTO users (id, email, password_hash, name, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &input.id,
                    &input.email,
                    &input.password_hash,
                    &input.name,
                    &now,
                    &now,
                ],
            )
            .await;

        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                // PostgreSQL error code 23505 is unique_violation
                if db_err.code().code() == "23505" {
                    return Err(DomainError::UserAlreadyExists(input.email.clone()));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!(user_id = %input.id, "user registered in database");

        Ok(User {
            id: input.id,
            email: input.email,
            password_hash: input.password_hash,
            name: input.name,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(user_id = %input.user_id))]
    async fn get_user(&self, input: GetUserInput) -> DomainResult<Option<User>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!(user_id = %input.user_id, "fetching user from database");

        let row = conn
            .query_opt(
                "SELECT id, email, password_hash, name, created_at, updated_at
                 FROM users
                 WHERE id = $1",
                &[&input.user_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let user_row = UserRow {
                    id: row.get("id"),
                    email: row.get("email"),
                    password_hash: row.get("password_hash"),
                    name: row.get("name"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                Ok(Some(user_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(email = %input.email))]
    async fn get_user_by_email(&self, input: GetUserByEmailInput) -> DomainResult<Option<User>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        debug!(email = %input.email, "fetching user by email from database");

        let row = conn
            .query_opt(
                "SELECT id, email, password_hash, name, created_at, updated_at
                 FROM users
                 WHERE email = $1",
                &[&input.email],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let user_row = UserRow {
                    id: row.get("id"),
                    email: row.get("email"),
                    password_hash: row.get("password_hash"),
                    name: row.get("name"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };
                Ok(Some(user_row.into()))
            }
            None => Ok(None),
        }
    }
}

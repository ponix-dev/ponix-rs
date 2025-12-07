use crate::auth::{
    CreateRefreshTokenInput, DeleteRefreshTokenInput, DeleteRefreshTokensByUserInput,
    GetRefreshTokenByHashInput, RefreshToken, RefreshTokenRepository, RotateRefreshTokenInput,
};
use crate::domain::{DomainError, DomainResult};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::{debug, instrument};

/// Refresh token row for PostgreSQL storage
#[derive(Debug, Clone)]
pub struct RefreshTokenRow {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl From<RefreshTokenRow> for RefreshToken {
    fn from(row: RefreshTokenRow) -> Self {
        RefreshToken {
            id: row.id,
            user_id: row.user_id,
            token_hash: row.token_hash,
            expires_at: row.expires_at,
            created_at: Some(row.created_at),
        }
    }
}

/// PostgreSQL implementation of RefreshTokenRepository trait
#[derive(Clone)]
pub struct PostgresRefreshTokenRepository {
    client: PostgresClient,
}

impl PostgresRefreshTokenRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl RefreshTokenRepository for PostgresRefreshTokenRepository {
    #[instrument(skip(self, input), fields(user_id = %input.user_id))]
    async fn create_refresh_token(
        &self,
        input: CreateRefreshTokenInput,
    ) -> DomainResult<RefreshToken> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        conn.execute(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &input.id,
                &input.user_id,
                &input.token_hash,
                &input.expires_at,
                &now,
            ],
        )
        .await
        .map_err(|e| DomainError::RepositoryError(e.into()))?;

        debug!(token_id = %input.id, user_id = %input.user_id, "refresh token created in database");

        Ok(RefreshToken {
            id: input.id,
            user_id: input.user_id,
            token_hash: input.token_hash,
            expires_at: input.expires_at,
            created_at: Some(now),
        })
    }

    #[instrument(skip(self, input))]
    async fn get_refresh_token_by_hash(
        &self,
        input: GetRefreshTokenByHashInput,
    ) -> DomainResult<Option<RefreshToken>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT id, user_id, token_hash, expires_at, created_at
                 FROM refresh_tokens
                 WHERE token_hash = $1",
                &[&input.token_hash],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let token_row = RefreshTokenRow {
                    id: row.get("id"),
                    user_id: row.get("user_id"),
                    token_hash: row.get("token_hash"),
                    expires_at: row.get("expires_at"),
                    created_at: row.get("created_at"),
                };
                Ok(Some(token_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(token_id = %input.id))]
    async fn delete_refresh_token(&self, input: DeleteRefreshTokenInput) -> DomainResult<()> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        conn.execute("DELETE FROM refresh_tokens WHERE id = $1", &[&input.id])
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        debug!(token_id = %input.id, "refresh token deleted from database");

        Ok(())
    }

    #[instrument(skip(self, input), fields(user_id = %input.user_id))]
    async fn delete_refresh_tokens_by_user(
        &self,
        input: DeleteRefreshTokensByUserInput,
    ) -> DomainResult<()> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let result = conn
            .execute(
                "DELETE FROM refresh_tokens WHERE user_id = $1",
                &[&input.user_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        debug!(user_id = %input.user_id, deleted_count = %result, "refresh tokens deleted for user");

        Ok(())
    }

    #[instrument(skip(self, input), fields(old_token_id = %input.old_token_id, new_token_id = %input.new_token.id))]
    async fn rotate_refresh_token(
        &self,
        input: RotateRefreshTokenInput,
    ) -> DomainResult<RefreshToken> {
        let mut conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        // Start a transaction to ensure atomicity
        let transaction = conn
            .transaction()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        // Delete the old token
        transaction
            .execute(
                "DELETE FROM refresh_tokens WHERE id = $1",
                &[&input.old_token_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let now = Utc::now();

        // Create the new token
        transaction
            .execute(
                "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    &input.new_token.id,
                    &input.new_token.user_id,
                    &input.new_token.token_hash,
                    &input.new_token.expires_at,
                    &now,
                ],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        // Commit the transaction - if this fails, both operations are rolled back
        transaction
            .commit()
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        debug!(
            old_token_id = %input.old_token_id,
            new_token_id = %input.new_token.id,
            user_id = %input.new_token.user_id,
            "refresh token rotated atomically"
        );

        Ok(RefreshToken {
            id: input.new_token.id,
            user_id: input.new_token.user_id,
            token_hash: input.new_token.token_hash,
            expires_at: input.new_token.expires_at,
            created_at: Some(now),
        })
    }
}

use tokio_postgres_adapter::TokioPostgresAdapter;
use tracing::instrument;

use crate::domain::{DomainError, DomainResult};
use crate::postgres::PostgresClient;

/// Create a Casbin adapter backed by PostgreSQL.
///
/// This adapter can be passed to `CasbinAuthorizationService::new()` to create
/// an authorization service with PostgreSQL-backed policy storage.
#[instrument(skip(postgres_client))]
pub async fn create_postgres_authorization_adapter(
    postgres_client: &PostgresClient,
) -> DomainResult<TokioPostgresAdapter> {
    let pool = postgres_client.get_pool();
    let adapter = TokioPostgresAdapter::new_with_pool(pool)
        .await
        .map_err(|e| {
            DomainError::AuthorizationError(format!("Failed to create PostgreSQL adapter: {}", e))
        })?;
    Ok(adapter)
}

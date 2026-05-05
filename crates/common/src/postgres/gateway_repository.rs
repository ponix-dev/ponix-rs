use crate::domain::{
    CreateGatewayRepoInput, DeleteGatewayRepoInput, DomainError, DomainResult, Gateway,
    GatewayRepository, GetGatewayRepoInput, ListGatewaysRepoInput, MqttCredentials,
    UpdateGatewayRepoInput,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Gateway row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRow {
    pub gateway_id: String,
    pub organization_id: String,
    pub name: String,
    pub broker_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<GatewayRow> for Gateway {
    fn from(row: GatewayRow) -> Self {
        let credentials = match (row.username, row.password) {
            (Some(username), Some(password)) => Some(MqttCredentials { username, password }),
            _ => None,
        };

        Gateway {
            gateway_id: row.gateway_id,
            organization_id: row.organization_id,
            name: row.name,
            broker_url: row.broker_url,
            credentials,
            deleted_at: row.deleted_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

#[derive(Clone)]
pub struct PostgresGatewayRepository {
    client: PostgresClient,
}

impl PostgresGatewayRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl GatewayRepository for PostgresGatewayRepository {
    #[instrument(skip(self, input), fields(gateway_id = %input.gateway_id, organization_id = %input.organization_id))]
    async fn create_gateway(&self, input: CreateGatewayRepoInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "creating gateway in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();
        let username = input.credentials.as_ref().map(|c| c.username.clone());
        let password = input.credentials.as_ref().map(|c| c.password.clone());

        let result = conn
            .execute(
                "INSERT INTO gateways (gateway_id, organization_id, name, broker_url, username, password, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    &input.gateway_id,
                    &input.organization_id,
                    &input.name,
                    &input.broker_url,
                    &username,
                    &password,
                    &now,
                    &now,
                ],
            )
            .await;

        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                if db_err.code().code() == "23505" {
                    return Err(DomainError::GatewayAlreadyExists(input.gateway_id));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!(gateway_id = %input.gateway_id, "gateway created in database");

        Ok(Gateway {
            gateway_id: input.gateway_id,
            organization_id: input.organization_id,
            name: input.name,
            broker_url: input.broker_url,
            credentials: input.credentials,
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(gateway_id = %input.gateway_id, organization_id = %input.organization_id))]
    async fn get_gateway(&self, input: GetGatewayRepoInput) -> DomainResult<Option<Gateway>> {
        debug!(gateway_id = %input.gateway_id, organization_id = %input.organization_id, "getting gateway from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT gateway_id, organization_id, name, broker_url, username, password, deleted_at, created_at, updated_at
                 FROM gateways
                 WHERE gateway_id = $1 AND organization_id = $2 AND deleted_at IS NULL",
                &[&input.gateway_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let gateway = row.map(|row| {
            let gateway_row = GatewayRow {
                gateway_id: row.get(0),
                organization_id: row.get(1),
                name: row.get(2),
                broker_url: row.get(3),
                username: row.get(4),
                password: row.get(5),
                deleted_at: row.get(6),
                created_at: row.get(7),
                updated_at: row.get(8),
            };
            gateway_row.into()
        });

        Ok(gateway)
    }

    #[instrument(skip(self, input), fields(gateway_id = %input.gateway_id, organization_id = %input.organization_id))]
    async fn update_gateway(&self, input: UpdateGatewayRepoInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, organization_id = %input.organization_id, "updating gateway in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let username = input.credentials.as_ref().map(|c| c.username.clone());
        let password = input.credentials.as_ref().map(|c| c.password.clone());

        let mut query = String::from("UPDATE gateways SET updated_at = $1");
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&now];
        let mut param_idx = 2;

        if let Some(ref name) = input.name {
            query.push_str(&format!(", name = ${}", param_idx));
            params.push(name);
            param_idx += 1;
        }

        if let Some(ref broker_url) = input.broker_url {
            query.push_str(&format!(", broker_url = ${}", param_idx));
            params.push(broker_url);
            param_idx += 1;
        }

        if input.credentials.is_some() {
            query.push_str(&format!(
                ", username = ${}, password = ${}",
                param_idx,
                param_idx + 1
            ));
            params.push(&username);
            params.push(&password);
            param_idx += 2;
        }

        query.push_str(&format!(
            " WHERE gateway_id = ${} AND organization_id = ${} AND deleted_at IS NULL
             RETURNING gateway_id, organization_id, name, broker_url, username, password, deleted_at, created_at, updated_at",
            param_idx,
            param_idx + 1
        ));
        params.push(&input.gateway_id);
        params.push(&input.organization_id);

        let row = conn
            .query_opt(&query, &params[..])
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let gateway_row = GatewayRow {
                    gateway_id: row.get(0),
                    organization_id: row.get(1),
                    name: row.get(2),
                    broker_url: row.get(3),
                    username: row.get(4),
                    password: row.get(5),
                    deleted_at: row.get(6),
                    created_at: row.get(7),
                    updated_at: row.get(8),
                };
                debug!(gateway_id = %gateway_row.gateway_id, "gateway updated in database");
                Ok(gateway_row.into())
            }
            None => Err(DomainError::GatewayNotFound(input.gateway_id)),
        }
    }

    #[instrument(skip(self, input), fields(gateway_id = %input.gateway_id, organization_id = %input.organization_id))]
    async fn delete_gateway(&self, input: DeleteGatewayRepoInput) -> DomainResult<()> {
        debug!(gateway_id = %input.gateway_id, organization_id = %input.organization_id, "soft deleting gateway");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let rows_affected = conn
            .execute(
                "UPDATE gateways
                 SET deleted_at = $1, updated_at = $1
                 WHERE gateway_id = $2 AND organization_id = $3 AND deleted_at IS NULL",
                &[&now, &input.gateway_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        if rows_affected == 0 {
            return Err(DomainError::GatewayNotFound(input.gateway_id));
        }

        debug!(gateway_id = %input.gateway_id, "gateway soft deleted");
        Ok(())
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn list_gateways(&self, input: ListGatewaysRepoInput) -> DomainResult<Vec<Gateway>> {
        debug!(organization_id = %input.organization_id, "listing gateways from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT gateway_id, organization_id, name, broker_url, username, password, deleted_at, created_at, updated_at
                 FROM gateways
                 WHERE organization_id = $1 AND deleted_at IS NULL
                 ORDER BY created_at DESC",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let gateways: Vec<Gateway> = rows
            .into_iter()
            .map(|row| {
                let gateway_row = GatewayRow {
                    gateway_id: row.get(0),
                    organization_id: row.get(1),
                    name: row.get(2),
                    broker_url: row.get(3),
                    username: row.get(4),
                    password: row.get(5),
                    deleted_at: row.get(6),
                    created_at: row.get(7),
                    updated_at: row.get(8),
                };
                gateway_row.into()
            })
            .collect();

        debug!(count = gateways.len(), "listed gateways from database");
        Ok(gateways)
    }

    #[instrument(skip(self))]
    async fn list_all_gateways(&self) -> DomainResult<Vec<Gateway>> {
        debug!("listing all non-deleted gateways from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT gateway_id, organization_id, name, broker_url, username, password, deleted_at, created_at, updated_at
                 FROM gateways
                 WHERE deleted_at IS NULL
                 ORDER BY created_at DESC",
                &[],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let gateways: Vec<Gateway> = rows
            .into_iter()
            .map(|row| {
                let gateway_row = GatewayRow {
                    gateway_id: row.get(0),
                    organization_id: row.get(1),
                    name: row.get(2),
                    broker_url: row.get(3),
                    username: row.get(4),
                    password: row.get(5),
                    deleted_at: row.get(6),
                    created_at: row.get(7),
                    updated_at: row.get(8),
                };
                gateway_row.into()
            })
            .collect();

        debug!(count = gateways.len(), "listed all gateways from database");
        Ok(gateways)
    }
}

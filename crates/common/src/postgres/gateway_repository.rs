use crate::domain::{
    CreateGatewayInputWithId, DomainError, DomainResult, EmqxGatewayConfig, Gateway, GatewayConfig,
    GatewayRepository, UpdateGatewayInput,
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
    pub gateway_type: String,
    pub gateway_config: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database GatewayRow to domain Gateway
impl From<GatewayRow> for Gateway {
    fn from(row: GatewayRow) -> Self {
        // Convert JSON config to domain GatewayConfig enum
        let gateway_config = json_to_gateway_config(&row.gateway_config);

        Gateway {
            gateway_id: row.gateway_id,
            organization_id: row.organization_id,
            gateway_type: row.gateway_type,
            gateway_config,
            deleted_at: row.deleted_at,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// Convert serde_json::Value to domain GatewayConfig
fn json_to_gateway_config(json: &serde_json::Value) -> GatewayConfig {
    let broker_url = json
        .get("broker_url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let subscription_group = json
        .get("subscription_group")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    GatewayConfig::Emqx(EmqxGatewayConfig {
        broker_url,
        subscription_group,
    })
}

/// Convert domain GatewayConfig to serde_json::Value
pub fn gateway_config_to_json(config: &GatewayConfig) -> serde_json::Value {
    match config {
        GatewayConfig::Emqx(emqx) => {
            serde_json::json!({
                "broker_url": emqx.broker_url,
                "subscription_group": emqx.subscription_group
            })
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
    async fn create_gateway(&self, input: CreateGatewayInputWithId) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "creating gateway in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Convert domain GatewayConfig to JSON for storage
        let gateway_config_json = gateway_config_to_json(&input.gateway_config);

        let result = conn
            .execute(
                "INSERT INTO gateways (gateway_id, organization_id, gateway_type, gateway_config, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &input.gateway_id,
                    &input.organization_id,
                    &input.gateway_type,
                    &gateway_config_json,
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
            gateway_type: input.gateway_type,
            gateway_config: input.gateway_config,
            deleted_at: None,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self), fields(gateway_id = %gateway_id))]
    async fn get_gateway(&self, gateway_id: &str) -> DomainResult<Option<Gateway>> {
        debug!(gateway_id = %gateway_id, "getting gateway from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT gateway_id, organization_id, gateway_type, gateway_config, deleted_at, created_at, updated_at
                 FROM gateways
                 WHERE gateway_id = $1 AND deleted_at IS NULL",
                &[&gateway_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let gateway = row.map(|row| {
            let gateway_row = GatewayRow {
                gateway_id: row.get(0),
                organization_id: row.get(1),
                gateway_type: row.get(2),
                gateway_config: row.get(3),
                deleted_at: row.get(4),
                created_at: row.get(5),
                updated_at: row.get(6),
            };
            gateway_row.into()
        });

        Ok(gateway)
    }

    #[instrument(skip(self, input), fields(gateway_id = %input.gateway_id))]
    async fn update_gateway(&self, input: UpdateGatewayInput) -> DomainResult<Gateway> {
        debug!(gateway_id = %input.gateway_id, "updating gateway in database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Convert domain GatewayConfig to JSON if provided
        let gateway_config_json = input.gateway_config.as_ref().map(gateway_config_to_json);

        // Build dynamic UPDATE query based on provided fields
        let mut query = String::from("UPDATE gateways SET updated_at = $1");
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&now];
        let mut param_idx = 2;

        if let Some(ref gateway_type) = input.gateway_type {
            query.push_str(&format!(", gateway_type = ${}", param_idx));
            params.push(gateway_type);
            param_idx += 1;
        }

        if let Some(ref config_json) = gateway_config_json {
            query.push_str(&format!(", gateway_config = ${}", param_idx));
            params.push(config_json);
            param_idx += 1;
        }

        query.push_str(&format!(
            " WHERE gateway_id = ${} AND deleted_at IS NULL
             RETURNING gateway_id, organization_id, gateway_type, gateway_config, deleted_at, created_at, updated_at",
            param_idx
        ));
        params.push(&input.gateway_id);

        let row = conn
            .query_opt(&query, &params[..])
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let gateway_row = GatewayRow {
                    gateway_id: row.get(0),
                    organization_id: row.get(1),
                    gateway_type: row.get(2),
                    gateway_config: row.get(3),
                    deleted_at: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
                };
                debug!(gateway_id = %gateway_row.gateway_id, "gateway updated in database");
                Ok(gateway_row.into())
            }
            None => Err(DomainError::GatewayNotFound(input.gateway_id)),
        }
    }

    #[instrument(skip(self), fields(gateway_id = %gateway_id))]
    async fn delete_gateway(&self, gateway_id: &str) -> DomainResult<()> {
        debug!(gateway_id = %gateway_id, "soft deleting gateway");

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
                 WHERE gateway_id = $2 AND deleted_at IS NULL",
                &[&now, &gateway_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        if rows_affected == 0 {
            return Err(DomainError::GatewayNotFound(gateway_id.to_string()));
        }

        debug!(gateway_id = %gateway_id, "gateway soft deleted");
        Ok(())
    }

    #[instrument(skip(self), fields(organization_id = %organization_id))]
    async fn list_gateways(&self, organization_id: &str) -> DomainResult<Vec<Gateway>> {
        debug!(organization_id = %organization_id, "listing gateways from database");

        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT gateway_id, organization_id, gateway_type, gateway_config, deleted_at, created_at, updated_at
                 FROM gateways
                 WHERE organization_id = $1 AND deleted_at IS NULL
                 ORDER BY created_at DESC",
                &[&organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let gateways: Vec<Gateway> = rows
            .into_iter()
            .map(|row| {
                let gateway_row = GatewayRow {
                    gateway_id: row.get(0),
                    organization_id: row.get(1),
                    gateway_type: row.get(2),
                    gateway_config: row.get(3),
                    deleted_at: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
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
                "SELECT gateway_id, organization_id, gateway_type, gateway_config, deleted_at, created_at, updated_at
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
                    gateway_type: row.get(2),
                    gateway_config: row.get(3),
                    deleted_at: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
                };
                gateway_row.into()
            })
            .collect();

        debug!(count = gateways.len(), "listed all gateways from database");
        Ok(gateways)
    }
}

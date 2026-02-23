use crate::domain::{
    CreateEndDeviceDefinitionRepoInput, DeleteEndDeviceDefinitionRepoInput, DomainError,
    DomainResult, EndDeviceDefinition, EndDeviceDefinitionRepository,
    GetEndDeviceDefinitionRepoInput, ListEndDeviceDefinitionsRepoInput, PayloadContract,
    UpdateEndDeviceDefinitionRepoInput,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Definition row for PostgreSQL storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndDeviceDefinitionRow {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub contracts: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<EndDeviceDefinitionRow> for EndDeviceDefinition {
    type Error = DomainError;

    fn try_from(row: EndDeviceDefinitionRow) -> Result<Self, Self::Error> {
        let contracts: Vec<PayloadContract> =
            serde_json::from_value(row.contracts).map_err(|e| {
                DomainError::RepositoryError(anyhow::anyhow!(
                    "Failed to deserialize contracts: {}",
                    e
                ))
            })?;

        Ok(EndDeviceDefinition {
            id: row.id,
            organization_id: row.organization_id,
            name: row.name,
            contracts,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        })
    }
}

/// PostgreSQL implementation of EndDeviceDefinitionRepository
#[derive(Clone)]
pub struct PostgresEndDeviceDefinitionRepository {
    client: PostgresClient,
}

impl PostgresEndDeviceDefinitionRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl EndDeviceDefinitionRepository for PostgresEndDeviceDefinitionRepository {
    #[instrument(skip(self, input), fields(id = %input.id, organization_id = %input.organization_id))]
    async fn create_definition(
        &self,
        input: CreateEndDeviceDefinitionRepoInput,
    ) -> DomainResult<EndDeviceDefinition> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let contracts_json = serde_json::to_value(&input.contracts).map_err(|e| {
            DomainError::RepositoryError(anyhow::anyhow!("Failed to serialize contracts: {}", e))
        })?;

        let result = conn
            .execute(
                "INSERT INTO end_device_definitions (id, organization_id, name, contracts, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &input.id,
                    &input.organization_id,
                    &input.name,
                    &contracts_json,
                    &now,
                    &now,
                ],
            )
            .await;

        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                if db_err.code().code() == "23505" {
                    return Err(DomainError::EndDeviceDefinitionAlreadyExists(input.id));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!("created end device definition: {}", input.id);

        Ok(EndDeviceDefinition {
            id: input.id,
            organization_id: input.organization_id,
            name: input.name,
            contracts: input.contracts,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(id = %input.id, organization_id = %input.organization_id))]
    async fn get_definition(
        &self,
        input: GetEndDeviceDefinitionRepoInput,
    ) -> DomainResult<Option<EndDeviceDefinition>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT id, organization_id, name, contracts, created_at, updated_at
                 FROM end_device_definitions
                 WHERE id = $1 AND organization_id = $2",
                &[&input.id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let def_row = EndDeviceDefinitionRow {
                    id: row.get(0),
                    organization_id: row.get(1),
                    name: row.get(2),
                    contracts: row.get(3),
                    created_at: row.get(4),
                    updated_at: row.get(5),
                };
                Ok(Some(def_row.try_into()?))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(id = %input.id, organization_id = %input.organization_id))]
    async fn update_definition(
        &self,
        input: UpdateEndDeviceDefinitionRepoInput,
    ) -> DomainResult<EndDeviceDefinition> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Build dynamic UPDATE query
        let mut query = String::from("UPDATE end_device_definitions SET updated_at = $1");
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&now];
        let mut param_idx = 2;

        if let Some(ref name) = input.name {
            query.push_str(&format!(", name = ${}", param_idx));
            params.push(name);
            param_idx += 1;
        }

        // Serialize contracts if provided
        let contracts_json = input
            .contracts
            .as_ref()
            .map(|c| {
                serde_json::to_value(c).map_err(|e| {
                    DomainError::RepositoryError(anyhow::anyhow!(
                        "Failed to serialize contracts: {}",
                        e
                    ))
                })
            })
            .transpose()?;

        if let Some(ref contracts) = contracts_json {
            query.push_str(&format!(", contracts = ${}", param_idx));
            params.push(contracts);
            param_idx += 1;
        }

        query.push_str(&format!(
            " WHERE id = ${} AND organization_id = ${}
             RETURNING id, organization_id, name, contracts, created_at, updated_at",
            param_idx,
            param_idx + 1
        ));
        params.push(&input.id);
        params.push(&input.organization_id);

        let row = conn
            .query_opt(&query, &params[..])
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let def_row = EndDeviceDefinitionRow {
                    id: row.get(0),
                    organization_id: row.get(1),
                    name: row.get(2),
                    contracts: row.get(3),
                    created_at: row.get(4),
                    updated_at: row.get(5),
                };
                debug!("updated end device definition: {}", input.id);
                def_row.try_into()
            }
            None => Err(DomainError::EndDeviceDefinitionNotFound(input.id)),
        }
    }

    #[instrument(skip(self, input), fields(id = %input.id, organization_id = %input.organization_id))]
    async fn delete_definition(
        &self,
        input: DeleteEndDeviceDefinitionRepoInput,
    ) -> DomainResult<()> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let result = conn
            .execute(
                "DELETE FROM end_device_definitions WHERE id = $1 AND organization_id = $2",
                &[&input.id, &input.organization_id],
            )
            .await;

        match result {
            Ok(rows_affected) => {
                if rows_affected == 0 {
                    return Err(DomainError::EndDeviceDefinitionNotFound(input.id));
                }
                debug!("deleted end device definition: {}", input.id);
                Ok(())
            }
            Err(e) => {
                if let Some(db_err) = e.as_db_error() {
                    if db_err.code().code() == "23503" {
                        return Err(DomainError::EndDeviceDefinitionInUse(input.id));
                    }
                }
                Err(DomainError::RepositoryError(e.into()))
            }
        }
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn list_definitions(
        &self,
        input: ListEndDeviceDefinitionsRepoInput,
    ) -> DomainResult<Vec<EndDeviceDefinition>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT id, organization_id, name, contracts, created_at, updated_at
                 FROM end_device_definitions
                 WHERE organization_id = $1
                 ORDER BY created_at DESC",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let mut definitions = Vec::with_capacity(rows.len());
        for row in &rows {
            let def_row = EndDeviceDefinitionRow {
                id: row.get(0),
                organization_id: row.get(1),
                name: row.get(2),
                contracts: row.get(3),
                created_at: row.get(4),
                updated_at: row.get(5),
            };
            definitions.push(def_row.try_into()?);
        }

        debug!(
            "found {} definitions for organization: {}",
            definitions.len(),
            input.organization_id
        );

        Ok(definitions)
    }
}

use crate::domain::{
    CreateDataStreamRepoInput, DataStream, DataStreamRepository, DataStreamWithDefinition,
    DomainError, DomainResult, GetDataStreamRepoInput, GetDataStreamWithDefinitionRepoInput,
    ListDataStreamsByGatewayRepoInput, ListDataStreamsRepoInput, PayloadContract,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// DataStream row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStreamRow {
    pub data_stream_id: String,
    pub organization_id: String,
    pub workspace_id: String,
    pub definition_id: String,
    pub gateway_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database DataStreamRow to domain DataStream
impl From<DataStreamRow> for DataStream {
    fn from(row: DataStreamRow) -> Self {
        DataStream {
            data_stream_id: row.data_stream_id,
            organization_id: row.organization_id,
            workspace_id: row.workspace_id,
            definition_id: row.definition_id,
            gateway_id: row.gateway_id,
            name: row.name,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// DataStream with definition row for joined query results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStreamWithDefinitionRow {
    pub data_stream_id: String,
    pub organization_id: String,
    pub workspace_id: String,
    pub definition_id: String,
    pub gateway_id: String,
    pub definition_name: String,
    pub name: String,
    pub contracts: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database DataStreamWithDefinitionRow to domain DataStreamWithDefinition
impl DataStreamWithDefinitionRow {
    fn into_domain(self) -> Result<DataStreamWithDefinition, DomainError> {
        let contracts: Vec<PayloadContract> =
            serde_json::from_value(self.contracts).map_err(|e| {
                DomainError::RepositoryError(anyhow::anyhow!(
                    "Failed to deserialize contracts: {}",
                    e
                ))
            })?;

        Ok(DataStreamWithDefinition {
            data_stream_id: self.data_stream_id,
            organization_id: self.organization_id,
            workspace_id: self.workspace_id,
            definition_id: self.definition_id,
            gateway_id: self.gateway_id,
            definition_name: self.definition_name,
            name: self.name,
            contracts,
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
        })
    }
}

/// PostgreSQL implementation of DataStreamRepository trait
#[derive(Clone)]
pub struct PostgresDataStreamRepository {
    client: PostgresClient,
}

impl PostgresDataStreamRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl DataStreamRepository for PostgresDataStreamRepository {
    #[instrument(skip(self, input), fields(data_stream_id = %input.data_stream_id, organization_id = %input.organization_id, gateway_id = %input.gateway_id))]
    async fn create_data_stream(
        &self,
        input: CreateDataStreamRepoInput,
    ) -> DomainResult<DataStream> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let result = conn
            .execute(
                "INSERT INTO data_streams (data_stream_id, organization_id, workspace_id, definition_id, gateway_id, name, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    &input.data_stream_id,
                    &input.organization_id,
                    &input.workspace_id,
                    &input.definition_id,
                    &input.gateway_id,
                    &input.name,
                    &now,
                    &now,
                ],
            )
            .await;

        if let Err(e) = result {
            if let Some(db_err) = e.as_db_error() {
                if db_err.code().code() == "23505" {
                    return Err(DomainError::DataStreamAlreadyExists(input.data_stream_id));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!("registered data stream: {}", input.data_stream_id);

        Ok(DataStream {
            data_stream_id: input.data_stream_id,
            organization_id: input.organization_id,
            workspace_id: input.workspace_id,
            definition_id: input.definition_id,
            gateway_id: input.gateway_id,
            name: input.name,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(data_stream_id = %input.data_stream_id, organization_id = %input.organization_id, workspace_id = %input.workspace_id))]
    async fn get_data_stream(
        &self,
        input: GetDataStreamRepoInput,
    ) -> DomainResult<Option<DataStream>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT data_stream_id, organization_id, workspace_id, definition_id, gateway_id, name, created_at, updated_at
                 FROM data_streams
                 WHERE data_stream_id = $1 AND organization_id = $2 AND workspace_id = $3",
                &[
                    &input.data_stream_id,
                    &input.organization_id,
                    &input.workspace_id,
                ],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let data_stream_row = DataStreamRow {
                    data_stream_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    name: row.get(5),
                    created_at: row.get(6),
                    updated_at: row.get(7),
                };
                Ok(Some(data_stream_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id, workspace_id = %input.workspace_id))]
    async fn list_data_streams(
        &self,
        input: ListDataStreamsRepoInput,
    ) -> DomainResult<Vec<DataStream>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT data_stream_id, organization_id, workspace_id, definition_id, gateway_id, name, created_at, updated_at
                 FROM data_streams
                 WHERE organization_id = $1 AND workspace_id = $2
                 ORDER BY created_at DESC",
                &[&input.organization_id, &input.workspace_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let data_streams = rows
            .iter()
            .map(|row| {
                let data_stream_row = DataStreamRow {
                    data_stream_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    name: row.get(5),
                    created_at: row.get(6),
                    updated_at: row.get(7),
                };
                data_stream_row.into()
            })
            .collect();

        debug!(
            "found {} data streams for workspace: {} in organization: {}",
            rows.len(),
            input.workspace_id,
            input.organization_id
        );

        Ok(data_streams)
    }

    #[instrument(skip(self, input), fields(data_stream_id = %input.data_stream_id, organization_id = %input.organization_id))]
    async fn get_data_stream_with_definition(
        &self,
        input: GetDataStreamWithDefinitionRepoInput,
    ) -> DomainResult<Option<DataStreamWithDefinition>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT d.data_stream_id, d.organization_id, d.workspace_id, d.definition_id, d.gateway_id,
                        def.name as definition_name, d.name,
                        def.contracts,
                        d.created_at, d.updated_at
                 FROM data_streams d
                 INNER JOIN data_stream_definitions def ON d.definition_id = def.id
                 WHERE d.data_stream_id = $1 AND d.organization_id = $2",
                &[&input.data_stream_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let joined_row = DataStreamWithDefinitionRow {
                    data_stream_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    definition_name: row.get(5),
                    name: row.get(6),
                    contracts: row.get(7),
                    created_at: row.get(8),
                    updated_at: row.get(9),
                };
                debug!(
                    "found data stream with definition: {}",
                    joined_row.data_stream_id
                );
                joined_row.into_domain().map(Some)
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id, gateway_id = %input.gateway_id))]
    async fn list_data_streams_by_gateway(
        &self,
        input: ListDataStreamsByGatewayRepoInput,
    ) -> DomainResult<Vec<DataStream>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT data_stream_id, organization_id, workspace_id, definition_id, gateway_id, name, created_at, updated_at
                 FROM data_streams
                 WHERE organization_id = $1 AND gateway_id = $2
                 ORDER BY created_at DESC",
                &[&input.organization_id, &input.gateway_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let data_streams = rows
            .iter()
            .map(|row| {
                let data_stream_row = DataStreamRow {
                    data_stream_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    name: row.get(5),
                    created_at: row.get(6),
                    updated_at: row.get(7),
                };
                data_stream_row.into()
            })
            .collect();

        debug!(
            "found {} data streams for gateway: {} in organization: {}",
            rows.len(),
            input.gateway_id,
            input.organization_id
        );

        Ok(data_streams)
    }
}

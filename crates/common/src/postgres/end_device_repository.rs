use crate::domain::{
    CreateEndDeviceRepoInput, DomainError, DomainResult, EndDevice, EndDeviceRepository,
    EndDeviceWithDefinition, GetEndDeviceRepoInput, GetEndDeviceWithDefinitionRepoInput,
    ListEndDevicesByGatewayRepoInput, ListEndDevicesRepoInput, PayloadContract,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// EndDevice row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndDeviceRow {
    pub end_device_id: String,
    pub organization_id: String,
    pub definition_id: String,
    pub gateway_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database EndDeviceRow to domain EndDevice
impl From<EndDeviceRow> for EndDevice {
    fn from(row: EndDeviceRow) -> Self {
        EndDevice {
            end_device_id: row.end_device_id,
            organization_id: row.organization_id,
            definition_id: row.definition_id,
            gateway_id: row.gateway_id,
            name: row.name,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// EndDevice with definition row for joined query results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndDeviceWithDefinitionRow {
    pub end_device_id: String,
    pub organization_id: String,
    pub definition_id: String,
    pub gateway_id: String,
    pub definition_name: String,
    pub name: String,
    pub contracts: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database EndDeviceWithDefinitionRow to domain EndDeviceWithDefinition
impl EndDeviceWithDefinitionRow {
    fn into_domain(self) -> Result<EndDeviceWithDefinition, DomainError> {
        let contracts: Vec<PayloadContract> =
            serde_json::from_value(self.contracts).map_err(|e| {
                DomainError::RepositoryError(anyhow::anyhow!(
                    "Failed to deserialize contracts: {}",
                    e
                ))
            })?;

        Ok(EndDeviceWithDefinition {
            end_device_id: self.end_device_id,
            organization_id: self.organization_id,
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

/// PostgreSQL implementation of EndDeviceRepository trait
#[derive(Clone)]
pub struct PostgresEndDeviceRepository {
    client: PostgresClient,
}

impl PostgresEndDeviceRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl EndDeviceRepository for PostgresEndDeviceRepository {
    #[instrument(skip(self, input), fields(end_device_id = %input.end_device_id, organization_id = %input.organization_id, gateway_id = %input.gateway_id))]
    async fn create_end_device(&self, input: CreateEndDeviceRepoInput) -> DomainResult<EndDevice> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        let result = conn
            .execute(
                "INSERT INTO end_devices (end_device_id, organization_id, definition_id, gateway_id, name, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &input.end_device_id,
                    &input.organization_id,
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
                    return Err(DomainError::EndDeviceAlreadyExists(input.end_device_id));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!("registered end device: {}", input.end_device_id);

        Ok(EndDevice {
            end_device_id: input.end_device_id,
            organization_id: input.organization_id,
            definition_id: input.definition_id,
            gateway_id: input.gateway_id,
            name: input.name,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(end_device_id = %input.end_device_id, organization_id = %input.organization_id))]
    async fn get_end_device(
        &self,
        input: GetEndDeviceRepoInput,
    ) -> DomainResult<Option<EndDevice>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT end_device_id, organization_id, definition_id, gateway_id, name, created_at, updated_at
                 FROM end_devices
                 WHERE end_device_id = $1 AND organization_id = $2",
                &[
                    &input.end_device_id,
                    &input.organization_id,
                ],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let end_device_row = EndDeviceRow {
                    end_device_id: row.get(0),
                    organization_id: row.get(1),
                    definition_id: row.get(2),
                    gateway_id: row.get(3),
                    name: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
                };
                Ok(Some(end_device_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id))]
    async fn list_end_devices(
        &self,
        input: ListEndDevicesRepoInput,
    ) -> DomainResult<Vec<EndDevice>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT end_device_id, organization_id, definition_id, gateway_id, name, created_at, updated_at
                 FROM end_devices
                 WHERE organization_id = $1
                 ORDER BY created_at DESC",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let end_devices = rows
            .iter()
            .map(|row| {
                let end_device_row = EndDeviceRow {
                    end_device_id: row.get(0),
                    organization_id: row.get(1),
                    definition_id: row.get(2),
                    gateway_id: row.get(3),
                    name: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
                };
                end_device_row.into()
            })
            .collect();

        debug!(
            "found {} end devices for organization: {}",
            rows.len(),
            input.organization_id
        );

        Ok(end_devices)
    }

    #[instrument(skip(self, input), fields(end_device_id = %input.end_device_id, organization_id = %input.organization_id))]
    async fn get_end_device_with_definition(
        &self,
        input: GetEndDeviceWithDefinitionRepoInput,
    ) -> DomainResult<Option<EndDeviceWithDefinition>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT d.end_device_id, d.organization_id, d.definition_id, d.gateway_id,
                        def.name as definition_name, d.name,
                        def.contracts,
                        d.created_at, d.updated_at
                 FROM end_devices d
                 INNER JOIN end_device_definitions def ON d.definition_id = def.id
                 WHERE d.end_device_id = $1 AND d.organization_id = $2",
                &[&input.end_device_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let joined_row = EndDeviceWithDefinitionRow {
                    end_device_id: row.get(0),
                    organization_id: row.get(1),
                    definition_id: row.get(2),
                    gateway_id: row.get(3),
                    definition_name: row.get(4),
                    name: row.get(5),
                    contracts: row.get(6),
                    created_at: row.get(7),
                    updated_at: row.get(8),
                };
                debug!(
                    "found end device with definition: {}",
                    joined_row.end_device_id
                );
                joined_row.into_domain().map(Some)
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id, gateway_id = %input.gateway_id))]
    async fn list_end_devices_by_gateway(
        &self,
        input: ListEndDevicesByGatewayRepoInput,
    ) -> DomainResult<Vec<EndDevice>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT end_device_id, organization_id, definition_id, gateway_id, name, created_at, updated_at
                 FROM end_devices
                 WHERE organization_id = $1 AND gateway_id = $2
                 ORDER BY created_at DESC",
                &[&input.organization_id, &input.gateway_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let end_devices = rows
            .iter()
            .map(|row| {
                let end_device_row = EndDeviceRow {
                    end_device_id: row.get(0),
                    organization_id: row.get(1),
                    definition_id: row.get(2),
                    gateway_id: row.get(3),
                    name: row.get(4),
                    created_at: row.get(5),
                    updated_at: row.get(6),
                };
                end_device_row.into()
            })
            .collect();

        debug!(
            "found {} end devices for gateway: {} in organization: {}",
            rows.len(),
            input.gateway_id,
            input.organization_id
        );

        Ok(end_devices)
    }
}

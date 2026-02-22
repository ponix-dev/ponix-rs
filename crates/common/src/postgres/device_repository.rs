use crate::domain::{
    CreateDeviceRepoInput, Device, DeviceRepository, DeviceWithDefinition, DomainError,
    DomainResult, GetDeviceRepoInput, GetDeviceWithDefinitionRepoInput,
    ListDevicesByGatewayRepoInput, ListDevicesRepoInput,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Device row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRow {
    pub device_id: String,
    pub organization_id: String,
    pub workspace_id: String,
    pub definition_id: String,
    pub gateway_id: String,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database DeviceRow to domain Device
impl From<DeviceRow> for Device {
    fn from(row: DeviceRow) -> Self {
        Device {
            device_id: row.device_id,
            organization_id: row.organization_id,
            workspace_id: row.workspace_id,
            definition_id: row.definition_id,
            gateway_id: row.gateway_id,
            name: row.device_name, // Map device_name -> name
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// Device with definition row for joined query results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceWithDefinitionRow {
    pub device_id: String,
    pub organization_id: String,
    pub workspace_id: String,
    pub definition_id: String,
    pub gateway_id: String,
    pub definition_name: String,
    pub device_name: String,
    pub payload_conversion: String,
    pub json_schema: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database DeviceWithDefinitionRow to domain DeviceWithDefinition
impl From<DeviceWithDefinitionRow> for DeviceWithDefinition {
    fn from(row: DeviceWithDefinitionRow) -> Self {
        DeviceWithDefinition {
            device_id: row.device_id,
            organization_id: row.organization_id,
            workspace_id: row.workspace_id,
            definition_id: row.definition_id,
            gateway_id: row.gateway_id,
            definition_name: row.definition_name,
            name: row.device_name,
            payload_conversion: row.payload_conversion,
            json_schema: row.json_schema,
            created_at: Some(row.created_at),
            updated_at: Some(row.updated_at),
        }
    }
}

/// PostgreSQL implementation of DeviceRepository trait
#[derive(Clone)]
pub struct PostgresDeviceRepository {
    client: PostgresClient,
}

impl PostgresDeviceRepository {
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl DeviceRepository for PostgresDeviceRepository {
    #[instrument(skip(self, input), fields(device_id = %input.device_id, organization_id = %input.organization_id, gateway_id = %input.gateway_id))]
    async fn create_device(&self, input: CreateDeviceRepoInput) -> DomainResult<Device> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Execute insert
        let result = conn
            .execute(
                "INSERT INTO devices (device_id, organization_id, workspace_id, definition_id, gateway_id, device_name, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    &input.device_id,
                    &input.organization_id,
                    &input.workspace_id,
                    &input.definition_id,
                    &input.gateway_id,
                    &input.name, // Map name -> device_name
                    &now,
                    &now,
                ],
            )
            .await;

        // Check for unique constraint violation
        if let Err(e) = result {
            // Check if it's a database error with a unique constraint violation
            if let Some(db_err) = e.as_db_error() {
                // PostgreSQL error code 23505 is unique_violation
                if db_err.code().code() == "23505" {
                    return Err(DomainError::DeviceAlreadyExists(input.device_id));
                }
            }
            return Err(DomainError::RepositoryError(e.into()));
        }

        debug!("registered device: {}", input.device_id);

        // Return created device
        Ok(Device {
            device_id: input.device_id,
            organization_id: input.organization_id,
            workspace_id: input.workspace_id,
            definition_id: input.definition_id,
            gateway_id: input.gateway_id,
            name: input.name,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    #[instrument(skip(self, input), fields(device_id = %input.device_id, organization_id = %input.organization_id, workspace_id = %input.workspace_id))]
    async fn get_device(&self, input: GetDeviceRepoInput) -> DomainResult<Option<Device>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT device_id, organization_id, workspace_id, definition_id, gateway_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE device_id = $1 AND organization_id = $2 AND workspace_id = $3",
                &[&input.device_id, &input.organization_id, &input.workspace_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let device_row = DeviceRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    device_name: row.get(5),
                    created_at: row.get(6),
                    updated_at: row.get(7),
                };
                Ok(Some(device_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id, workspace_id = %input.workspace_id))]
    async fn list_devices(&self, input: ListDevicesRepoInput) -> DomainResult<Vec<Device>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT device_id, organization_id, workspace_id, definition_id, gateway_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE organization_id = $1 AND workspace_id = $2
                 ORDER BY created_at DESC",
                &[&input.organization_id, &input.workspace_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let devices = rows
            .iter()
            .map(|row| {
                let device_row = DeviceRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    device_name: row.get(5),
                    created_at: row.get(6),
                    updated_at: row.get(7),
                };
                device_row.into()
            })
            .collect();

        debug!(
            "found {} devices for workspace: {} in organization: {}",
            rows.len(),
            input.workspace_id,
            input.organization_id
        );

        Ok(devices)
    }

    #[instrument(skip(self, input), fields(device_id = %input.device_id, organization_id = %input.organization_id))]
    async fn get_device_with_definition(
        &self,
        input: GetDeviceWithDefinitionRepoInput,
    ) -> DomainResult<Option<DeviceWithDefinition>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT d.device_id, d.organization_id, d.workspace_id, d.definition_id, d.gateway_id,
                        def.name as definition_name, d.device_name,
                        def.payload_conversion, def.json_schema,
                        d.created_at, d.updated_at
                 FROM devices d
                 INNER JOIN end_device_definitions def ON d.definition_id = def.id
                 WHERE d.device_id = $1 AND d.organization_id = $2",
                &[&input.device_id, &input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let joined_row = DeviceWithDefinitionRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    definition_name: row.get(5),
                    device_name: row.get(6),
                    payload_conversion: row.get(7),
                    json_schema: row.get(8),
                    created_at: row.get(9),
                    updated_at: row.get(10),
                };
                debug!("found device with definition: {}", joined_row.device_id);
                Ok(Some(joined_row.into()))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, input), fields(organization_id = %input.organization_id, gateway_id = %input.gateway_id))]
    async fn list_devices_by_gateway(
        &self,
        input: ListDevicesByGatewayRepoInput,
    ) -> DomainResult<Vec<Device>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT device_id, organization_id, workspace_id, definition_id, gateway_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE organization_id = $1 AND gateway_id = $2
                 ORDER BY created_at DESC",
                &[&input.organization_id, &input.gateway_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let devices = rows
            .iter()
            .map(|row| {
                let device_row = DeviceRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    workspace_id: row.get(2),
                    definition_id: row.get(3),
                    gateway_id: row.get(4),
                    device_name: row.get(5),
                    created_at: row.get(6),
                    updated_at: row.get(7),
                };
                device_row.into()
            })
            .collect();

        debug!(
            "found {} devices for gateway: {} in organization: {}",
            rows.len(),
            input.gateway_id,
            input.organization_id
        );

        Ok(devices)
    }
}

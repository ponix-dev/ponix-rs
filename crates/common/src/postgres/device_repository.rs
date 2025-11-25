use crate::domain::{
    CreateDeviceInputWithId, Device, DeviceRepository, DomainError, DomainResult, GetDeviceInput,
    ListDevicesInput,
};
use crate::postgres::PostgresClient;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Device row for PostgreSQL storage with timestamp metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRow {
    pub device_id: String,
    pub organization_id: String,
    pub device_name: String,
    pub payload_conversion: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Convert database DeviceRow to domain Device
impl From<DeviceRow> for Device {
    fn from(row: DeviceRow) -> Self {
        Device {
            device_id: row.device_id,
            organization_id: row.organization_id,
            name: row.device_name, // Map device_name -> name
            payload_conversion: row.payload_conversion,
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
    async fn create_device(&self, input: CreateDeviceInputWithId) -> DomainResult<Device> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Execute insert
        let result = conn.execute(
            "INSERT INTO devices (device_id, organization_id, device_name, payload_conversion, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &input.device_id,
                &input.organization_id,
                &input.name, // Map name -> device_name
                &input.payload_conversion,
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

        debug!("Registered device: {}", input.device_id);

        // Return created device
        Ok(Device {
            device_id: input.device_id,
            organization_id: input.organization_id,
            name: input.name,
            payload_conversion: input.payload_conversion,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Option<Device>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT device_id, organization_id, device_name, payload_conversion, created_at, updated_at
                 FROM devices
                 WHERE device_id = $1",
                &[&input.device_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        match row {
            Some(row) => {
                let device_row = DeviceRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    device_name: row.get(2),
                    payload_conversion: row.get(3),
                    created_at: row.get(4),
                    updated_at: row.get(5),
                };
                Ok(Some(device_row.into()))
            }
            None => Ok(None),
        }
    }

    async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>> {
        let conn = self
            .client
            .get_connection()
            .await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT device_id, organization_id, device_name, payload_conversion, created_at, updated_at
                 FROM devices
                 WHERE organization_id = $1
                 ORDER BY created_at DESC",
                &[&input.organization_id],
            )
            .await
            .map_err(|e| DomainError::RepositoryError(e.into()))?;

        let devices = rows
            .iter()
            .map(|row| {
                let device_row = DeviceRow {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    device_name: row.get(2),
                    payload_conversion: row.get(3),
                    created_at: row.get(4),
                    updated_at: row.get(5),
                };
                device_row.into()
            })
            .collect();

        debug!(
            "Found {} devices for organization: {}",
            rows.len(),
            input.organization_id
        );

        Ok(devices)
    }
}

use async_trait::async_trait;
use chrono::Utc;
use tracing::debug;

use ponix_domain::{
    CreateDeviceInputWithId, Device, DeviceRepository, DomainError, DomainResult,
    GetDeviceInput, ListDevicesInput,
};

use crate::client::PostgresClient;
use crate::models::DeviceRow;

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
        let conn = self.client.get_connection().await
            .map_err(DomainError::RepositoryError)?;

        let now = Utc::now();

        // Execute insert
        let result = conn.execute(
            "INSERT INTO devices (device_id, organization_id, device_name, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &input.device_id,
                &input.organization_id,
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

        debug!("Registered device: {}", input.device_id);

        // Return created device
        Ok(Device {
            device_id: input.device_id,
            organization_id: input.organization_id,
            name: input.name,
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    async fn get_device(&self, input: GetDeviceInput) -> DomainResult<Option<Device>> {
        let conn = self.client.get_connection().await
            .map_err(DomainError::RepositoryError)?;

        let row = conn
            .query_opt(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
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
                    created_at: row.get(3),
                    updated_at: row.get(4),
                };
                Ok(Some(device_row.into()))
            }
            None => Ok(None),
        }
    }

    async fn list_devices(&self, input: ListDevicesInput) -> DomainResult<Vec<Device>> {
        let conn = self.client.get_connection().await
            .map_err(DomainError::RepositoryError)?;

        let rows = conn
            .query(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
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
                    created_at: row.get(3),
                    updated_at: row.get(4),
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

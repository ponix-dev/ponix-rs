use anyhow::{Context, Result};
use chrono::Utc;
use tracing::debug;

use crate::client::PostgresClient;
use crate::models::Device;

/// Device storage operations for PostgreSQL
#[derive(Clone)]
pub struct DeviceStore {
    client: PostgresClient,
}

impl DeviceStore {
    /// Creates a new DeviceStore
    pub fn new(client: PostgresClient) -> Self {
        Self { client }
    }

    /// Registers a new device
    ///
    /// # Arguments
    /// * `device` - The Device protobuf message to store
    ///
    /// # Returns
    /// `Ok(())` if successful, error otherwise
    ///
    /// # Errors
    /// - Device ID already exists (unique constraint violation)
    /// - Database connection issues
    pub async fn register_device(&self, device: &Device) -> Result<()> {
        let conn = self.client.get_connection().await?;
        let now = Utc::now();

        conn.execute(
            "INSERT INTO devices (device_id, organization_id, device_name, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &device.device_id,
                &device.organization_id,
                &device.device_name,
                &now,
                &now,
            ],
        )
        .await
        .context("Failed to insert device")?;

        debug!("Registered device: {}", device.device_id);

        Ok(())
    }

    /// Queries a device by device ID
    ///
    /// # Arguments
    /// * `device_id` - The unique device identifier
    ///
    /// # Returns
    /// `Ok(Some(Device))` if found, `Ok(None)` if not found, error on database issues
    pub async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>> {
        let conn = self.client.get_connection().await?;

        let row = conn
            .query_opt(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE device_id = $1",
                &[&device_id],
            )
            .await
            .context("Failed to query device")?;

        match row {
            Some(row) => {
                let device = Device {
                    device_id: row.get(0),
                    organization_id: row.get(1),
                    device_name: row.get(2),
                };
                Ok(Some(device))
            }
            None => Ok(None),
        }
    }

    /// Lists all devices for an organization
    ///
    /// # Arguments
    /// * `organization_id` - The organization identifier
    ///
    /// # Returns
    /// Vector of Device messages for the organization
    pub async fn list_devices_by_organization(&self, organization_id: &str) -> Result<Vec<Device>> {
        let conn = self.client.get_connection().await?;

        let rows = conn
            .query(
                "SELECT device_id, organization_id, device_name, created_at, updated_at
                 FROM devices
                 WHERE organization_id = $1
                 ORDER BY created_at DESC",
                &[&organization_id],
            )
            .await
            .context("Failed to query devices by organization")?;

        let devices = rows
            .iter()
            .map(|row| Device {
                device_id: row.get(0),
                organization_id: row.get(1),
                device_name: row.get(2),
            })
            .collect();

        debug!(
            "Found {} devices for organization: {}",
            rows.len(),
            organization_id
        );

        Ok(devices)
    }

    /// Updates a device name
    ///
    /// # Arguments
    /// * `device_id` - The device to update
    /// * `new_name` - The new device name
    ///
    /// # Returns
    /// `Ok(())` if successful, error if device not found or database issues
    pub async fn update_device_name(&self, device_id: &str, new_name: &str) -> Result<()> {
        let conn = self.client.get_connection().await?;
        let now = Utc::now();

        let rows_affected = conn
            .execute(
                "UPDATE devices
                 SET device_name = $1, updated_at = $2
                 WHERE device_id = $3",
                &[&new_name, &now, &device_id],
            )
            .await
            .context("Failed to update device")?;

        if rows_affected == 0 {
            anyhow::bail!("Device not found: {}", device_id);
        }

        debug!("Updated device name: {}", device_id);

        Ok(())
    }
}

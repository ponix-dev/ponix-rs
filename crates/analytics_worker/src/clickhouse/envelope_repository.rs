use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clickhouse::Row;
use common::clickhouse::ClickHouseClient;
use common::domain::{
    DomainError, DomainResult, ProcessedEnvelope, ProcessedEnvelopeRepository, StoreEnvelopesInput,
};

use serde::{Deserialize, Serialize};
use tracing::{debug, error};

#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ProcessedEnvelopeRow {
    pub organization_id: String,
    pub end_device_id: String,
    // CRITICAL: This serde attribute is required when using JSON type alongside DateTime
    // Reference: https://github.com/ClickHouse/clickhouse-rs/blob/main/examples/data_types_derive_simple.rs
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub occurred_at: DateTime<Utc>,
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    pub processed_at: DateTime<Utc>,
    // JSON type in ClickHouse, mapped to String in Rust
    pub data: String,
}

/// Convert domain ProcessedEnvelope to database ProcessedEnvelopeRow
impl From<&ProcessedEnvelope> for ProcessedEnvelopeRow {
    fn from(envelope: &ProcessedEnvelope) -> Self {
        // Convert serde_json::Map to JSON string for ClickHouse storage
        let data_json = serde_json::to_string(&envelope.data).unwrap_or_else(|_| "{}".to_string());

        ProcessedEnvelopeRow {
            organization_id: envelope.organization_id.clone(),
            end_device_id: envelope.end_device_id.clone(),
            occurred_at: envelope.occurred_at,
            processed_at: envelope.processed_at,
            data: data_json,
        }
    }
}

/// ClickHouse implementation of ProcessedEnvelopeRepository
#[derive(Clone)]
pub struct ClickHouseEnvelopeRepository {
    client: ClickHouseClient,
    table: String,
}

impl ClickHouseEnvelopeRepository {
    pub fn new(client: ClickHouseClient, table: String) -> Self {
        Self { client, table }
    }
}

#[async_trait]
impl ProcessedEnvelopeRepository for ClickHouseEnvelopeRepository {
    async fn store_batch(&self, input: StoreEnvelopesInput) -> DomainResult<()> {
        if input.envelopes.is_empty() {
            debug!("no envelopes to store, skipping");
            return Ok(());
        }

        debug!(
            envelope_count = input.envelopes.len(),
            table = %self.table,
            "storing envelope batch to ClickHouse"
        );

        // Convert domain envelopes to database rows
        let rows: Vec<ProcessedEnvelopeRow> = input
            .envelopes
            .iter()
            .map(|envelope| envelope.into())
            .collect();

        // Insert batch using ClickHouse inserter API
        let mut insert = self
            .client
            .get_client()
            .insert::<ProcessedEnvelopeRow>(&self.table)
            .await
            .map_err(|e| {
                error!("failed to create ClickHouse inserter: {}", e);
                DomainError::RepositoryError(e.into())
            })?;

        for row in &rows {
            insert.write(row).await.map_err(|e| {
                error!("failed to write row to ClickHouse: {}", e);
                DomainError::RepositoryError(e.into())
            })?;
        }

        insert.end().await.map_err(|e| {
            error!("failed to finalize ClickHouse insert: {}", e);
            DomainError::RepositoryError(e.into())
        })?;

        debug!(
            rows_inserted = rows.len(),
            "successfully stored envelope batch"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_domain_to_row_conversion() {
        let mut data_map = serde_json::Map::new();
        data_map.insert("temperature".to_string(), serde_json::json!(23.5));
        data_map.insert("humidity".to_string(), serde_json::json!(65));

        let domain_envelope = ProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Utc::now(),
            processed_at: Utc::now(),
            data: data_map,
        };

        let row: ProcessedEnvelopeRow = (&domain_envelope).into();

        assert_eq!(row.organization_id, "org-123");
        assert_eq!(row.end_device_id, "device-456");
        assert!(row.data.contains("temperature"));
        assert!(row.data.contains("23.5"));
    }

    #[test]
    fn test_empty_data_conversion() {
        let domain_envelope = ProcessedEnvelope {
            organization_id: "org-123".to_string(),
            end_device_id: "device-456".to_string(),
            occurred_at: Utc::now(),
            processed_at: Utc::now(),
            data: serde_json::Map::new(),
        };

        let row: ProcessedEnvelopeRow = (&domain_envelope).into();

        assert_eq!(row.data, "{}");
    }
}

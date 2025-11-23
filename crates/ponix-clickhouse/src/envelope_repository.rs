use async_trait::async_trait;
use ponix_domain::types::StoreEnvelopesInput;
use ponix_domain::{DomainError, DomainResult, ProcessedEnvelopeRepository};
use tracing::{debug, error};

use crate::client::ClickHouseClient;
use crate::models::ProcessedEnvelopeRow;

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
            debug!("No envelopes to store, skipping");
            return Ok(());
        }

        debug!(
            envelope_count = input.envelopes.len(),
            table = %self.table,
            "Storing envelope batch to ClickHouse"
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
                error!("Failed to create ClickHouse inserter: {}", e);
                DomainError::RepositoryError(e.into())
            })?;

        for row in &rows {
            insert.write(row).await.map_err(|e| {
                error!("Failed to write row to ClickHouse: {}", e);
                DomainError::RepositoryError(e.into())
            })?;
        }

        insert.end().await.map_err(|e| {
            error!("Failed to finalize ClickHouse insert: {}", e);
            DomainError::RepositoryError(e.into())
        })?;

        debug!(
            rows_inserted = rows.len(),
            "Successfully stored envelope batch"
        );

        Ok(())
    }
}

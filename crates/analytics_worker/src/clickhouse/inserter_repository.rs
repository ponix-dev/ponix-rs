use crate::clickhouse::envelope_repository::ProcessedEnvelopeRow;
use anyhow::{Context, Result};
use clickhouse::inserter::Inserter;
use common::clickhouse::ClickHouseClient;
use common::domain::ProcessedEnvelope;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Configuration for the ClickHouse inserter
#[derive(Debug, Clone)]
pub struct InserterConfig {
    /// Maximum number of rows before auto-flush
    pub max_entries: u64,
    /// Maximum time before auto-flush
    pub period: Duration,
}

impl Default for InserterConfig {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            period: Duration::from_secs(1),
        }
    }
}

/// ClickHouse repository using the Inserter for background batching.
///
/// This repository buffers writes and flushes them to ClickHouse either:
/// - When `max_entries` rows have been buffered
/// - When `period` time has elapsed since the last flush
/// - When `commit()` is explicitly called
///
/// The inserter is wrapped in a Mutex because it requires `&mut self` for writes.
pub struct ClickHouseInserterRepository {
    inserter: Arc<Mutex<Inserter<ProcessedEnvelopeRow>>>,
}

impl ClickHouseInserterRepository {
    /// Create a new inserter repository
    pub fn new(client: &ClickHouseClient, table: &str, config: InserterConfig) -> Result<Self> {
        info!(
            table = %table,
            max_entries = config.max_entries,
            period_secs = config.period.as_secs(),
            "creating ClickHouse inserter repository"
        );

        let inserter = client
            .get_client()
            .inserter::<ProcessedEnvelopeRow>(table)
            .with_max_rows(config.max_entries)
            .with_period(Some(config.period));

        Ok(Self {
            inserter: Arc::new(Mutex::new(inserter)),
        })
    }

    /// Write a single envelope to the buffer.
    ///
    /// This operation is fast - it just buffers the row. The actual write
    /// to ClickHouse happens when the buffer is flushed (by time, count, or explicit commit).
    pub async fn write(&self, envelope: &ProcessedEnvelope) -> Result<()> {
        let row = ProcessedEnvelopeRow::from(envelope);

        let mut inserter = self.inserter.lock().await;
        inserter
            .write(&row)
            .await
            .context("failed to write row to inserter buffer")?;

        debug!(
            device_id = %envelope.end_device_id,
            "buffered envelope for ClickHouse insert"
        );

        Ok(())
    }

    /// Explicitly commit/flush the current buffer to ClickHouse.
    ///
    /// Call this periodically or on shutdown to ensure all buffered data is written.
    pub async fn commit(&self) -> Result<()> {
        let mut inserter = self.inserter.lock().await;
        let stats = inserter
            .commit()
            .await
            .context("failed to commit inserter buffer")?;

        if stats.rows > 0 {
            info!(
                rows = stats.rows,
                transactions = stats.transactions,
                "committed envelope batch to ClickHouse"
            );
        }

        Ok(())
    }

    /// Force an immediate flush and finalize the inserter.
    ///
    /// Call this on graceful shutdown to ensure all data is written.
    pub async fn end(self) -> Result<()> {
        // We need to take ownership of the inserter, but it's behind Arc<Mutex>
        // The caller should ensure this is only called once during shutdown
        let inserter = Arc::try_unwrap(self.inserter)
            .map_err(|_| anyhow::anyhow!("cannot end inserter: still has other references"))?
            .into_inner();

        let stats = inserter.end().await.context("failed to end inserter")?;

        info!(
            rows = stats.rows,
            transactions = stats.transactions,
            "finalized ClickHouse inserter"
        );

        Ok(())
    }
}

impl Clone for ClickHouseInserterRepository {
    fn clone(&self) -> Self {
        Self {
            inserter: Arc::clone(&self.inserter),
        }
    }
}

/// Handle for committing the inserter periodically.
///
/// This runs as a background task and commits the buffer on a regular interval.
pub struct InserterCommitHandle {
    repository: ClickHouseInserterRepository,
}

impl InserterCommitHandle {
    pub fn new(repository: ClickHouseInserterRepository) -> Self {
        Self { repository }
    }

    /// Run the commit loop until cancellation
    pub async fn run(
        &self,
        ctx: tokio_util::sync::CancellationToken,
        commit_interval: Duration,
    ) -> Result<()> {
        info!(
            interval_secs = commit_interval.as_secs(),
            "starting inserter commit loop"
        );

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    info!("inserter commit loop shutting down");
                    // Final commit on shutdown
                    if let Err(e) = self.repository.commit().await {
                        error!(error = %e, "failed to commit on shutdown");
                    }
                    break;
                }
                _ = tokio::time::sleep(commit_interval) => {
                    if let Err(e) = self.repository.commit().await {
                        warn!(error = %e, "failed to commit inserter buffer");
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = InserterConfig::default();
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.period, Duration::from_secs(1));
    }
}

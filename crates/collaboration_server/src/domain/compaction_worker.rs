use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::domain::snapshotter_service::SnapshotterService;

/// Periodic compaction worker that writes dirty documents to PostgreSQL.
pub struct CompactionWorker {
    service: Arc<SnapshotterService>,
    compaction_interval: Duration,
    idle_eviction_threshold: Duration,
}

impl CompactionWorker {
    pub fn new(
        service: Arc<SnapshotterService>,
        compaction_interval: Duration,
        idle_eviction_threshold: Duration,
    ) -> Self {
        Self {
            service,
            compaction_interval,
            idle_eviction_threshold,
        }
    }

    pub async fn run(self, ctx: CancellationToken) -> anyhow::Result<()> {
        info!(
            interval_secs = self.compaction_interval.as_secs(),
            idle_eviction_secs = self.idle_eviction_threshold.as_secs(),
            "starting compaction worker"
        );

        let mut interval = tokio::time::interval(self.compaction_interval);

        loop {
            tokio::select! {
                _ = ctx.cancelled() => {
                    info!("compaction worker shutdown, running final compaction");
                    if let Err(e) = self.service.compact_dirty_documents().await {
                        error!(error = %e, "final compaction failed");
                    }
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = self.service.compact_dirty_documents().await {
                        error!(error = %e, "compaction cycle failed");
                    }
                    self.service.evict_idle_documents(self.idle_eviction_threshold).await;
                }
            }
        }

        debug!("compaction worker stopped");
        Ok(())
    }
}

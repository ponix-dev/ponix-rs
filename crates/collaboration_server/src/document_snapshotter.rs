use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::domain::DocumentRepository;
use common::nats::NatsClient;
use ponix_runner::AppProcess;

use crate::domain::{CompactionWorker, DocumentCache, SnapshotterService};
use crate::nats::DocumentUpdateConsumer;

/// Configuration for the Document Snapshotter.
pub struct DocumentSnapshotterConfig {
    pub document_updates_stream: String,
    pub consumer_name: String,
    pub filter_subject: String,
    pub batch_size: usize,
    pub batch_wait_secs: u64,
    pub compaction_interval_secs: u64,
    pub idle_eviction_secs: u64,
}

/// Document Snapshotter: the single writer for Yrs CRDT state to PostgreSQL.
///
/// Consumes Yrs updates from the `document_sync` JetStream stream, maintains
/// in-memory `yrs::Doc` instances per active document, and periodically writes
/// compacted state + extracted content to PostgreSQL.
pub struct DocumentSnapshotter {
    consumer: DocumentUpdateConsumer,
    compaction_worker: CompactionWorker,
}

impl DocumentSnapshotter {
    pub async fn new(
        document_repo: Arc<dyn DocumentRepository>,
        nats_client: Arc<NatsClient>,
        config: DocumentSnapshotterConfig,
    ) -> anyhow::Result<Self> {
        let cache: DocumentCache = Arc::new(Mutex::new(HashMap::new()));
        let snapshotter_service = Arc::new(SnapshotterService::new(document_repo, cache));

        let consumer = DocumentUpdateConsumer::new(
            nats_client,
            &config.document_updates_stream,
            &config.consumer_name,
            &config.filter_subject,
            config.batch_size,
            config.batch_wait_secs,
            snapshotter_service.clone(),
        )
        .await?;

        let compaction_worker = CompactionWorker::new(
            snapshotter_service,
            Duration::from_secs(config.compaction_interval_secs),
            Duration::from_secs(config.idle_eviction_secs),
        );

        Ok(Self {
            consumer,
            compaction_worker,
        })
    }

    /// Returns two runner processes: the NATS consumer and the compaction loop.
    pub fn into_runner_processes(self) -> Vec<AppProcess> {
        let consumer = self.consumer;
        let compaction_worker = self.compaction_worker;
        vec![
            Box::new(move |ctx| Box::pin(async move { consumer.run(ctx).await })),
            Box::new(move |ctx| Box::pin(async move { compaction_worker.run(ctx).await })),
        ]
    }
}

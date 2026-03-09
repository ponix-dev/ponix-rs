use std::sync::Arc;

use anyhow::Result;
use common::nats::{
    NatsClient, NatsConsumeLoggingLayer, NatsConsumeLoggingService, NatsConsumeTracingConfig,
    NatsConsumeTracingLayer, NatsConsumeTracingService, TowerConsumer,
};
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tracing::debug;

use crate::domain::SnapshotterService;
use crate::nats::document_update_service::DocumentUpdateService;

/// Type alias for the layered document update consumer service
type DocumentUpdateLayeredService =
    NatsConsumeTracingService<NatsConsumeLoggingService<DocumentUpdateService>>;

pub struct DocumentUpdateConsumer {
    consumer: TowerConsumer<DocumentUpdateLayeredService>,
}

impl DocumentUpdateConsumer {
    pub async fn new(
        nats_client: Arc<NatsClient>,
        stream_name: &str,
        consumer_name: &str,
        filter_subject: &str,
        batch_size: usize,
        batch_wait_secs: u64,
        snapshotter_service: Arc<SnapshotterService>,
    ) -> Result<Self> {
        debug!(
            stream = %stream_name,
            consumer = %consumer_name,
            filter = %filter_subject,
            "initializing document update consumer with Tower middleware"
        );

        let inner_service = DocumentUpdateService::new(snapshotter_service);
        let layered_service = ServiceBuilder::new()
            .layer(NatsConsumeTracingLayer::new(NatsConsumeTracingConfig::new(
                "process_document_update",
            )))
            .layer(NatsConsumeLoggingLayer::new())
            .service(inner_service);

        let consumer_client = nats_client.create_consumer_client();
        let consumer = TowerConsumer::new(
            consumer_client,
            stream_name,
            consumer_name,
            filter_subject,
            batch_size,
            batch_wait_secs,
            layered_service,
        )
        .await?;

        Ok(Self { consumer })
    }

    pub async fn run(self, ctx: CancellationToken) -> Result<()> {
        debug!("starting document update consumer");
        self.consumer.run(ctx).await
    }
}

use crate::clickhouse::ClickHouseEnvelopeRepository;
use crate::domain::{CelPayloadConverter, ProcessedEnvelopeService, RawEnvelopeService};
use crate::nats::{
    create_processed_envelope_processor, create_raw_envelope_processor, ProcessedEnvelopeProducer,
};
use common::{ClickHouseClient, NatsClient, NatsConsumer};

use common::{PostgresDeviceRepository, PostgresOrganizationRepository};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct AnalyticsWorkerConfig {
    pub processed_envelopes_stream: String,
    pub processed_envelopes_subject: String,
    pub raw_stream: String,
    pub raw_subject: String,
    pub nats_batch_size: usize,
    pub nats_batch_wait_secs: u64,
}

pub struct AnalyticsWorker {
    processed_consumer: NatsConsumer,
    raw_consumer: NatsConsumer,
}

impl AnalyticsWorker {
    pub async fn new(
        device_repository: Arc<PostgresDeviceRepository>,
        organization_repository: Arc<PostgresOrganizationRepository>,
        clickhouse_client: ClickHouseClient,
        nats_client: Arc<NatsClient>,
        config: AnalyticsWorkerConfig,
    ) -> anyhow::Result<Self> {
        info!("Initializing Analytics Ingester module");

        // Initialize ProcessedEnvelope infrastructure
        let envelope_repository =
            ClickHouseEnvelopeRepository::new(clickhouse_client, "processed_envelopes".to_string());
        let envelope_service =
            Arc::new(ProcessedEnvelopeService::new(Arc::new(envelope_repository)));

        // Create processed envelope consumer
        let processor = create_processed_envelope_processor(envelope_service.clone());
        let consumer_client = nats_client.create_consumer_client();
        let processed_consumer = NatsConsumer::new(
            consumer_client,
            &config.processed_envelopes_stream,
            "ponix-all-in-one",
            &config.processed_envelopes_subject,
            config.nats_batch_size,
            config.nats_batch_wait_secs,
            processor,
        )
        .await?;

        // Initialize RawEnvelope infrastructure
        let publisher_client = nats_client.create_publisher_client();
        let payload_converter = Arc::new(CelPayloadConverter::new());
        let processed_envelope_producer = Arc::new(ProcessedEnvelopeProducer::new(
            publisher_client,
            config.processed_envelopes_stream.clone(),
        ));

        let raw_envelope_service = Arc::new(RawEnvelopeService::new(
            device_repository,
            organization_repository,
            payload_converter,
            processed_envelope_producer,
        ));

        // Create raw envelope consumer
        let raw_processor = create_raw_envelope_processor(raw_envelope_service);
        let raw_consumer_client = nats_client.create_consumer_client();
        let raw_consumer = NatsConsumer::new(
            raw_consumer_client,
            &config.raw_stream,
            "ponix-all-in-one-raw",
            &config.raw_subject,
            config.nats_batch_size,
            config.nats_batch_wait_secs,
            raw_processor,
        )
        .await?;

        info!("Analytics Ingester initialized");

        Ok(Self {
            processed_consumer,
            raw_consumer,
        })
    }

    pub fn into_runner_processes(
        self,
    ) -> Vec<
        Box<
            dyn FnOnce(
                    CancellationToken,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
                > + Send,
        >,
    > {
        vec![
            // Processed envelope consumer
            Box::new({
                let consumer = self.processed_consumer;
                move |ctx| Box::pin(async move { consumer.run(ctx).await })
            }),
            // Raw envelope consumer
            Box::new({
                let consumer = self.raw_consumer;
                move |ctx| Box::pin(async move { consumer.run(ctx).await })
            }),
        ]
    }
}

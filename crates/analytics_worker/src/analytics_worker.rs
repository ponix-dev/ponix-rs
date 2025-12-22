use crate::clickhouse::{ClickHouseInserterRepository, InserterCommitHandle, InserterConfig};
use crate::domain::{CelPayloadConverter, RawEnvelopeService};
use common::jsonschema::JsonSchemaValidator;
use crate::nats::{
    ProcessedEnvelopeConsumerService, ProcessedEnvelopeProducer, RawEnvelopeConsumerService,
};
use common::clickhouse::ClickHouseClient;
use common::nats::{
    NatsClient, NatsConsumeLoggingLayer, NatsConsumeTracingConfig, NatsConsumeTracingLayer,
    TowerConsumer,
};
use common::postgres::{PostgresDeviceRepository, PostgresOrganizationRepository};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tracing::info;

pub struct AnalyticsWorkerConfig {
    pub processed_envelopes_stream: String,
    pub processed_envelopes_subject: String,
    pub raw_stream: String,
    pub raw_subject: String,
    pub nats_batch_size: usize,
    pub nats_batch_wait_secs: u64,
    /// Maximum entries in ClickHouse inserter buffer before auto-flush
    pub clickhouse_inserter_max_entries: u64,
    /// Period for ClickHouse inserter auto-flush (seconds)
    pub clickhouse_inserter_period_secs: u64,
}

impl Default for AnalyticsWorkerConfig {
    fn default() -> Self {
        Self {
            processed_envelopes_stream: "processed_envelopes".to_string(),
            processed_envelopes_subject: "processed_envelopes.*".to_string(),
            raw_stream: "raw_envelopes".to_string(),
            raw_subject: "raw_envelopes.*".to_string(),
            nats_batch_size: 30,
            nats_batch_wait_secs: 5,
            clickhouse_inserter_max_entries: 10_000,
            clickhouse_inserter_period_secs: 1,
        }
    }
}

/// Type alias for the layered processed envelope consumer service
type ProcessedEnvelopeLayeredService = common::nats::NatsConsumeTracingService<
    common::nats::NatsConsumeLoggingService<ProcessedEnvelopeConsumerService>,
>;

/// Type alias for the layered raw envelope consumer service
type RawEnvelopeLayeredService = common::nats::NatsConsumeTracingService<
    common::nats::NatsConsumeLoggingService<RawEnvelopeConsumerService>,
>;

pub struct AnalyticsWorker {
    processed_consumer: TowerConsumer<ProcessedEnvelopeLayeredService>,
    raw_consumer: TowerConsumer<RawEnvelopeLayeredService>,
    inserter_commit_handle: InserterCommitHandle,
    inserter_commit_interval: Duration,
}

impl AnalyticsWorker {
    pub async fn new(
        device_repository: Arc<PostgresDeviceRepository>,
        organization_repository: Arc<PostgresOrganizationRepository>,
        clickhouse_client: ClickHouseClient,
        nats_client: Arc<NatsClient>,
        config: AnalyticsWorkerConfig,
    ) -> anyhow::Result<Self> {
        info!("initializing analytics ingester module with Tower-based consumers");

        // Initialize ClickHouse inserter for buffered writes
        let inserter_config = InserterConfig {
            max_entries: config.clickhouse_inserter_max_entries,
            period: Duration::from_secs(config.clickhouse_inserter_period_secs),
        };
        let inserter = ClickHouseInserterRepository::new(
            &clickhouse_client,
            "processed_envelopes",
            inserter_config,
        )?;

        // Create commit handle for background flushing
        let inserter_commit_handle = InserterCommitHandle::new(inserter.clone());

        // Build ProcessedEnvelope Tower service with middleware layers
        let processed_envelope_inner = ProcessedEnvelopeConsumerService::new(inserter);
        let processed_envelope_service = ServiceBuilder::new()
            .layer(NatsConsumeTracingLayer::new(NatsConsumeTracingConfig::new(
                "process_processed_envelope",
            )))
            .layer(NatsConsumeLoggingLayer::new())
            .service(processed_envelope_inner);

        // Create processed envelope Tower consumer
        let consumer_client = nats_client.create_consumer_client();
        let processed_consumer = TowerConsumer::new(
            consumer_client,
            &config.processed_envelopes_stream,
            "ponix-all-in-one",
            &config.processed_envelopes_subject,
            config.nats_batch_size,
            config.nats_batch_wait_secs,
            processed_envelope_service,
        )
        .await?;

        // Initialize RawEnvelope infrastructure
        let publisher_client = nats_client.create_publisher_client();
        let payload_converter = Arc::new(CelPayloadConverter::new());
        let processed_envelope_producer = Arc::new(ProcessedEnvelopeProducer::new(
            publisher_client,
            config.processed_envelopes_stream.clone(),
        ));

        // Initialize schema validator
        let schema_validator = Arc::new(JsonSchemaValidator::new());

        let raw_envelope_domain_service = Arc::new(RawEnvelopeService::new(
            device_repository,
            organization_repository,
            payload_converter,
            processed_envelope_producer,
            schema_validator,
        ));

        // Build RawEnvelope Tower service with middleware layers
        let raw_envelope_inner = RawEnvelopeConsumerService::new(raw_envelope_domain_service);
        let raw_envelope_service = ServiceBuilder::new()
            .layer(NatsConsumeTracingLayer::new(NatsConsumeTracingConfig::new(
                "process_raw_envelope",
            )))
            .layer(NatsConsumeLoggingLayer::new())
            .service(raw_envelope_inner);

        // Create raw envelope Tower consumer
        let raw_consumer_client = nats_client.create_consumer_client();
        let raw_consumer = TowerConsumer::new(
            raw_consumer_client,
            &config.raw_stream,
            "ponix-all-in-one-raw",
            &config.raw_subject,
            config.nats_batch_size,
            config.nats_batch_wait_secs,
            raw_envelope_service,
        )
        .await?;

        info!("analytics ingester initialized with Tower middleware");

        Ok(Self {
            processed_consumer,
            raw_consumer,
            inserter_commit_handle,
            inserter_commit_interval: Duration::from_secs(config.clickhouse_inserter_period_secs),
        })
    }

    #[allow(clippy::type_complexity)]
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
            // ClickHouse inserter commit loop
            Box::new({
                let commit_handle = self.inserter_commit_handle;
                let interval = self.inserter_commit_interval;
                move |ctx| Box::pin(async move { commit_handle.run(ctx, interval).await })
            }),
        ]
    }
}

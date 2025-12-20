use crate::domain::{
    GatewayOrchestrationService, GatewayOrchestrationServiceConfig, GatewayRunnerFactory,
    InMemoryGatewayProcessStore,
};
use crate::mqtt::EmqxGatewayRunner;
use crate::nats::{GatewayCdcConsumer, RawEnvelopeProducer};
use common::nats::NatsClient;
use common::postgres::PostgresGatewayRepository;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub struct GatewayOrchestratorConfig {
    pub gateway_stream: String,
    pub gateway_consumer_name: String,
    pub gateway_filter_subject: String,
    pub raw_envelopes_stream: String,
}

pub struct GatewayOrchestrator {
    cdc_consumer: GatewayCdcConsumer,
}

impl GatewayOrchestrator {
    pub async fn new(
        gateway_repository: Arc<PostgresGatewayRepository>,
        nats_client: Arc<NatsClient>,
        orchestrator_shutdown_token: CancellationToken,
        config: GatewayOrchestratorConfig,
    ) -> anyhow::Result<Self> {
        debug!("initializing gateway API module");

        // Create raw envelope producer for gateway processes
        let publisher_client = nats_client.create_publisher_client();
        let raw_envelope_producer: Arc<dyn common::domain::RawEnvelopeProducer> = Arc::new(
            RawEnvelopeProducer::new(publisher_client, config.raw_envelopes_stream.clone()),
        );

        // Configure gateway runner factory with supported gateway types
        let mut runner_factory = GatewayRunnerFactory::new();
        runner_factory.register_emqx(|| Arc::new(EmqxGatewayRunner::new()));

        // Initialize orchestrator
        let orchestrator_config = GatewayOrchestrationServiceConfig::default();
        let process_store = Arc::new(InMemoryGatewayProcessStore::new());
        let orchestrator = Arc::new(GatewayOrchestrationService::new(
            gateway_repository,
            process_store,
            orchestrator_config,
            orchestrator_shutdown_token,
            raw_envelope_producer,
            runner_factory,
        ));

        // Start orchestrator to load existing gateways
        orchestrator.launch_gateways().await?;
        debug!("gateway orchestrator started");

        // Setup CDC consumer
        let cdc_consumer = GatewayCdcConsumer::new(
            Arc::clone(&nats_client),
            config.gateway_stream.clone(),
            config.gateway_consumer_name.clone(),
            config.gateway_filter_subject.clone(),
            Arc::clone(&orchestrator),
        )
        .await?;

        Ok(Self { cdc_consumer })
    }

    #[allow(clippy::type_complexity)]
    pub fn into_runner_process(
        self,
    ) -> Box<
        dyn FnOnce(
                CancellationToken,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>,
            > + Send,
    > {
        // Gateway CDC Consumer process - handles NATS CDC events and orchestrates gateways
        Box::new({
            let cdc_consumer = self.cdc_consumer;
            move |ctx| Box::pin(async move { cdc_consumer.run(ctx).await })
        })
    }
}

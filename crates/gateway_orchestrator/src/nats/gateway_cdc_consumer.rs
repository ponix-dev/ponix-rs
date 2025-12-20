use crate::domain::GatewayOrchestrationService;
use crate::nats::GatewayCdcService;
use anyhow::Result;
use common::nats::{
    NatsClient, NatsConsumeLoggingLayer, NatsConsumeTracingConfig, NatsConsumeTracingLayer,
    TowerConsumer,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tracing::debug;

/// Type alias for the layered gateway CDC consumer service
type GatewayCdcLayeredService =
    NatsConsumeTracingService<NatsConsumeLoggingService<GatewayCdcService>>;

// Re-export the service types we need for the type alias
use common::nats::NatsConsumeLoggingService;
use common::nats::NatsConsumeTracingService;

pub struct GatewayCdcConsumer {
    consumer: TowerConsumer<GatewayCdcLayeredService>,
}

impl GatewayCdcConsumer {
    pub async fn new(
        nats_client: Arc<NatsClient>,
        stream_name: String,
        consumer_name: String,
        filter_subject: String,
        orchestrator: Arc<GatewayOrchestrationService>,
    ) -> Result<Self> {
        debug!(
            stream = %stream_name,
            consumer = %consumer_name,
            filter = %filter_subject,
            "initializing gateway CDC consumer with Tower middleware"
        );

        // Build the Tower service with middleware layers
        let inner_service = GatewayCdcService::new(orchestrator);
        let layered_service = ServiceBuilder::new()
            .layer(NatsConsumeTracingLayer::new(NatsConsumeTracingConfig::new(
                "process_gateway_cdc",
            )))
            .layer(NatsConsumeLoggingLayer::new())
            .service(inner_service);

        // Create the Tower consumer
        let consumer_client = nats_client.create_consumer_client();
        let consumer = TowerConsumer::new(
            consumer_client,
            &stream_name,
            &consumer_name,
            &filter_subject,
            10, // batch size
            5,  // max wait seconds
            layered_service,
        )
        .await?;

        Ok(Self { consumer })
    }

    /// Run the CDC consumer loop
    pub async fn run(self, ctx: CancellationToken) -> Result<()> {
        debug!("starting gateway CDC consumer");
        self.consumer.run(ctx).await
    }
}

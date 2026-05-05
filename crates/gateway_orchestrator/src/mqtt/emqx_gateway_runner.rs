use async_trait::async_trait;
use common::domain::{Gateway, RawEnvelopeProducer};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::domain::{GatewayOrchestrationServiceConfig, GatewayRunner};
use crate::mqtt::subscriber::run_mqtt_subscriber;

/// Gateway runner implementation for EMQX (and EMQX-compatible) MQTT brokers.
///
/// Connects to the broker at `gateway.broker_url`, authenticates with
/// `gateway.credentials` when present, and subscribes to topics matching
/// `{organization_id}/+` to receive device messages.
#[derive(Debug, Default)]
pub struct EmqxGatewayRunner;

impl EmqxGatewayRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl GatewayRunner for EmqxGatewayRunner {
    #[instrument(
        name = "emqx_gateway_runner",
        skip_all,
        fields(
            gateway_id = %gateway.gateway_id,
            organization_id = %gateway.organization_id,
            broker_url = %gateway.broker_url,
        )
    )]
    async fn run(
        &self,
        gateway: Gateway,
        config: GatewayOrchestrationServiceConfig,
        process_token: CancellationToken,
        shutdown_token: CancellationToken,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    ) {
        run_mqtt_subscriber(
            gateway,
            config,
            process_token,
            shutdown_token,
            raw_envelope_producer,
        )
        .await;
    }
}

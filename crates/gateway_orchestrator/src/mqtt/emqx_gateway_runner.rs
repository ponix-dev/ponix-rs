use async_trait::async_trait;
use common::domain::{Gateway, GatewayConfig, RawEnvelopeProducer};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::domain::{GatewayOrchestrationServiceConfig, GatewayRunner};
use crate::mqtt::subscriber::run_mqtt_subscriber;

/// Gateway runner implementation for EMQX MQTT brokers.
///
/// Connects to an EMQX broker and subscribes to topics matching
/// `{organization_id}/+` to receive device messages.
#[derive(Debug, Default)]
pub struct EmqxGatewayRunner;

impl EmqxGatewayRunner {
    pub fn new() -> Self {
        Self
    }

    /// Extract broker URL from gateway config for tracing
    fn extract_broker_url(config: &GatewayConfig) -> &str {
        match config {
            GatewayConfig::Emqx(emqx) => &emqx.broker_url,
        }
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
            broker_url = %Self::extract_broker_url(&gateway.gateway_config),
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

    fn gateway_type(&self) -> &'static str {
        "EMQX"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_type() {
        let runner = EmqxGatewayRunner::new();
        assert_eq!(runner.gateway_type(), "EMQX");
    }
}

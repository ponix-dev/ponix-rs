use async_trait::async_trait;
use common::domain::{Gateway, RawEnvelopeProducer};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

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
}

#[async_trait]
impl GatewayRunner for EmqxGatewayRunner {
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

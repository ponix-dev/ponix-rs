use async_trait::async_trait;
use common::domain::{Gateway, RawEnvelopeProducer};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::domain::GatewayOrchestrationServiceConfig;

/// Trait for running gateway subscriber processes.
///
/// All gateways today are LoRaWAN MQTT connections. The trait remains so that
/// future provider-specific runners (TTN, ChirpStack) can implement custom
/// connection / topic semantics.
#[async_trait]
pub trait GatewayRunner: Send + Sync {
    /// Run the gateway subscriber process.
    ///
    /// This method should:
    /// 1. Connect to the gateway's MQTT broker (using credentials when present)
    /// 2. Subscribe to messages for the organization
    /// 3. Convert received messages to RawEnvelopes
    /// 4. Publish RawEnvelopes to NATS via the producer
    /// 5. Handle reconnection and retries according to config
    /// 6. Stop gracefully when either cancellation token is triggered
    ///
    /// # Arguments
    /// * `gateway` — full gateway record, including credentials when present
    /// * `config` — orchestration service configuration (retry settings, etc.)
    /// * `process_token` — token for cancelling this specific gateway process
    /// * `shutdown_token` — token for global shutdown signal
    /// * `raw_envelope_producer` — producer for publishing RawEnvelopes to NATS
    async fn run(
        &self,
        gateway: Gateway,
        config: GatewayOrchestrationServiceConfig,
        process_token: CancellationToken,
        shutdown_token: CancellationToken,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    );
}

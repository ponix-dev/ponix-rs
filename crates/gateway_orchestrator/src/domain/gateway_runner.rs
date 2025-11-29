use async_trait::async_trait;
use common::domain::{Gateway, RawEnvelopeProducer};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::domain::GatewayOrchestrationServiceConfig;

/// Trait for running gateway subscriber processes.
///
/// Each gateway type (EMQX, TTN, ChirpStack, etc.) implements this trait
/// to define how to connect and subscribe to messages from that gateway.
#[async_trait]
pub trait GatewayRunner: Send + Sync {
    /// Run the gateway subscriber process.
    ///
    /// This method should:
    /// 1. Connect to the gateway's message broker/API
    /// 2. Subscribe to messages for the organization
    /// 3. Convert received messages to RawEnvelopes
    /// 4. Publish RawEnvelopes to NATS via the producer
    /// 5. Handle reconnection and retries according to config
    /// 6. Stop gracefully when either cancellation token is triggered
    ///
    /// # Arguments
    /// * `gateway` - The gateway configuration and metadata
    /// * `config` - Orchestration service configuration (retry settings, etc.)
    /// * `process_token` - Token for cancelling this specific gateway process
    /// * `shutdown_token` - Token for global shutdown signal
    /// * `raw_envelope_producer` - Producer for publishing RawEnvelopes to NATS
    async fn run(
        &self,
        gateway: Gateway,
        config: GatewayOrchestrationServiceConfig,
        process_token: CancellationToken,
        shutdown_token: CancellationToken,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    );

    /// Returns the gateway type name this runner handles (e.g., "EMQX", "TTN")
    fn gateway_type(&self) -> &'static str;
}

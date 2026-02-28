use async_trait::async_trait;
use common::domain::{DomainResult, Gateway, RawEnvelopeProducer};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::domain::{DeploymentHandle, GatewayOrchestrationServiceConfig, GatewayRunner};

/// Trait for deploying gateway processes.
///
/// Abstracts WHERE gateways run (in-process, Docker, Kubernetes) from
/// WHAT they do (protocol logic via `GatewayRunner`).
#[async_trait]
pub trait GatewayDeployer: Send + Sync {
    /// Deploy a gateway process using the provided runner.
    ///
    /// # Arguments
    /// * `gateway` - The gateway to deploy
    /// * `runner` - The protocol runner (already resolved by factory)
    /// * `config` - Orchestration service configuration
    /// * `shutdown_token` - Global shutdown signal
    /// * `raw_envelope_producer` - Producer for publishing RawEnvelopes
    ///
    /// # Returns
    /// A deployment handle for managing the running gateway process
    async fn deploy(
        &self,
        gateway: &Gateway,
        runner: Arc<dyn GatewayRunner>,
        config: GatewayOrchestrationServiceConfig,
        shutdown_token: CancellationToken,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    ) -> DomainResult<Box<dyn DeploymentHandle>>;
}

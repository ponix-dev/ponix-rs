use async_trait::async_trait;
use common::domain::{DomainResult, Gateway, RawEnvelopeProducer};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::domain::{
    DeploymentHandle, GatewayDeployer, GatewayOrchestrationServiceConfig, GatewayRunner,
};

use super::hash_gateway_config;

/// Deployer that runs gateway processes in-process via `tokio::spawn`.
pub struct InProcessDeployer;

#[async_trait]
impl GatewayDeployer for InProcessDeployer {
    async fn deploy(
        &self,
        gateway: &Gateway,
        runner: Arc<dyn GatewayRunner>,
        config: GatewayOrchestrationServiceConfig,
        shutdown_token: CancellationToken,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    ) -> DomainResult<Box<dyn DeploymentHandle>> {
        let gateway_clone = gateway.clone();
        let process_token = CancellationToken::new();
        let process_token_clone = process_token.clone();

        let join_handle = tokio::spawn(async move {
            runner
                .run(
                    gateway_clone,
                    config,
                    process_token_clone,
                    shutdown_token,
                    raw_envelope_producer,
                )
                .await;
        });

        let handle = InProcessDeploymentHandle {
            gateway: gateway.clone(),
            config_hash: hash_gateway_config(&gateway.gateway_config),
            cancellation_token: process_token,
            join_handle: Some(join_handle),
        };

        Ok(Box::new(handle))
    }
}

/// Handle to an in-process gateway task spawned via `tokio::spawn`.
pub struct InProcessDeploymentHandle {
    gateway: Gateway,
    config_hash: String,
    cancellation_token: CancellationToken,
    join_handle: Option<JoinHandle<()>>,
}

#[async_trait]
impl DeploymentHandle for InProcessDeploymentHandle {
    fn gateway(&self) -> &Gateway {
        &self.gateway
    }

    fn config_hash(&self) -> &str {
        &self.config_hash
    }

    fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    async fn wait_for_stop(&mut self, timeout: Duration) -> anyhow::Result<()> {
        if let Some(handle) = self.join_handle.take() {
            match tokio::time::timeout(timeout, handle).await {
                Ok(Ok(())) => {
                    debug!(
                        "process for gateway {} stopped gracefully",
                        self.gateway.gateway_id
                    );
                }
                Ok(Err(e)) => {
                    error!(
                        "process for gateway {} panicked: {:?}",
                        self.gateway.gateway_id, e
                    );
                }
                Err(_) => {
                    warn!(
                        "process for gateway {} did not stop within timeout",
                        self.gateway.gateway_id
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::domain::{EmqxGatewayConfig, GatewayConfig, MockRawEnvelopeProducer};

    struct MockRunner;

    #[async_trait]
    impl GatewayRunner for MockRunner {
        async fn run(
            &self,
            _gateway: Gateway,
            _config: GatewayOrchestrationServiceConfig,
            process_token: CancellationToken,
            shutdown_token: CancellationToken,
            _raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
        ) {
            tokio::select! {
                _ = process_token.cancelled() => {}
                _ = shutdown_token.cancelled() => {}
            }
        }

        fn gateway_type(&self) -> &'static str {
            "MOCK"
        }
    }

    fn test_gateway() -> Gateway {
        Gateway {
            gateway_id: "gw-test".to_string(),
            organization_id: "org-001".to_string(),
            name: "Test Gateway".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "mqtt://localhost:1883".to_string(),
            }),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
            deleted_at: None,
        }
    }

    fn mock_producer() -> Arc<dyn RawEnvelopeProducer> {
        let mut mock = MockRawEnvelopeProducer::new();
        mock.expect_publish_raw_envelope().returning(|_| Ok(()));
        Arc::new(mock)
    }

    #[tokio::test]
    async fn test_deploy_returns_handle_with_correct_gateway() {
        let deployer = InProcessDeployer;
        let gateway = test_gateway();
        let runner: Arc<dyn GatewayRunner> = Arc::new(MockRunner);
        let config = GatewayOrchestrationServiceConfig::default();
        let shutdown_token = CancellationToken::new();

        let handle = deployer
            .deploy(
                &gateway,
                runner,
                config,
                shutdown_token.clone(),
                mock_producer(),
            )
            .await
            .unwrap();

        assert_eq!(handle.gateway().gateway_id, "gw-test");
        assert!(!handle.config_hash().is_empty());

        // Cleanup
        handle.cancel();
        shutdown_token.cancel();
    }

    #[tokio::test]
    async fn test_cancel_and_wait_for_stop() {
        let deployer = InProcessDeployer;
        let gateway = test_gateway();
        let runner: Arc<dyn GatewayRunner> = Arc::new(MockRunner);
        let config = GatewayOrchestrationServiceConfig::default();
        let shutdown_token = CancellationToken::new();

        let mut handle = deployer
            .deploy(&gateway, runner, config, shutdown_token, mock_producer())
            .await
            .unwrap();

        handle.cancel();
        let result = handle.wait_for_stop(Duration::from_secs(5)).await;
        assert!(result.is_ok());
    }
}

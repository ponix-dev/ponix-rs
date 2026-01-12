use crate::domain::{
    GatewayOrchestrationServiceConfig, GatewayProcessHandle, GatewayProcessStore,
    GatewayRunnerFactory,
};
use common::domain::{
    DomainResult, Gateway, GatewayRepository, GetGatewayRepoInput, RawEnvelopeProducer,
};

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

pub struct GatewayOrchestrationService {
    gateway_repository: Arc<dyn GatewayRepository>,
    process_store: Arc<dyn GatewayProcessStore>,
    config: GatewayOrchestrationServiceConfig,
    shutdown_token: CancellationToken,
    raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    runner_factory: GatewayRunnerFactory,
}

impl GatewayOrchestrationService {
    pub fn new(
        gateway_repository: Arc<dyn GatewayRepository>,
        process_store: Arc<dyn GatewayProcessStore>,
        config: GatewayOrchestrationServiceConfig,
        shutdown_token: CancellationToken,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
        runner_factory: GatewayRunnerFactory,
    ) -> Self {
        Self {
            gateway_repository,
            process_store,
            config,
            shutdown_token,
            raw_envelope_producer,
            runner_factory,
        }
    }

    /// Load all non-deleted gateways and start their processes
    #[instrument(skip(self))]
    pub async fn launch_gateways(&self) -> DomainResult<()> {
        debug!("starting GatewayOrchestrator - loading all non-deleted gateways");

        let gateways = self.gateway_repository.list_all_gateways().await?;
        debug!("found {} non-deleted gateways to start", gateways.len());

        for gateway in gateways {
            if let Err(e) = self.start_gateway_process(&gateway).await {
                error!(
                    "failed to start process for gateway {}: {}",
                    gateway.gateway_id, e
                );
                // Continue starting other processes even if one fails
            }
        }

        Ok(())
    }

    /// Handle gateway created event
    #[instrument(skip(self), fields(gateway_id = %gateway.gateway_id))]
    pub async fn handle_gateway_created(&self, gateway: Gateway) -> DomainResult<()> {
        debug!("handling gateway created: {}", gateway.gateway_id);
        self.start_gateway_process(&gateway).await
    }

    /// Handle gateway updated event
    #[instrument(skip(self), fields(gateway_id = %gateway.gateway_id))]
    pub async fn handle_gateway_updated(&self, gateway: Gateway) -> DomainResult<()> {
        debug!("handling gateway updated: {}", gateway.gateway_id);

        // Check if soft deleted
        if gateway.deleted_at.is_some() {
            debug!(
                "gateway {} has been soft deleted, stopping process",
                gateway.gateway_id
            );
            return self.stop_gateway_process(&gateway.gateway_id).await;
        }

        let config_changed = match self
            .gateway_repository
            .get_gateway(GetGatewayRepoInput {
                gateway_id: gateway.gateway_id.clone(),
                organization_id: gateway.organization_id.clone(),
            })
            .await?
        {
            Some(old_gateway) => old_gateway.gateway_config != gateway.gateway_config,
            None => {
                warn!(
                    "old gateway state not found for {}, starting new process",
                    gateway.gateway_id
                );
                true
            }
        };

        if config_changed {
            debug!(
                "gateway {} config changed, restarting process",
                gateway.gateway_id
            );
            // Stop existing process
            self.stop_gateway_process(&gateway.gateway_id).await?;
            // Start with new config
            self.start_gateway_process(&gateway).await?;
        } else {
            debug!(
                "gateway {} updated but config unchanged, no action needed",
                gateway.gateway_id
            );
        }

        Ok(())
    }

    /// Handle gateway deleted event
    #[instrument(skip(self), fields(gateway_id = %gateway_id))]
    pub async fn handle_gateway_deleted(&self, gateway_id: &str) -> DomainResult<()> {
        debug!("handling gateway deleted: {}", gateway_id);
        self.stop_gateway_process(gateway_id).await
    }

    /// Stop all running gateway processes
    #[instrument(skip(self))]
    pub async fn shutdown(&self) -> DomainResult<()> {
        info!("shutting down GatewayOrchestrator");

        let gateway_ids = self.process_store.list_gateway_ids().await?;
        info!("stopping {} gateway processes", gateway_ids.len());

        for gateway_id in gateway_ids {
            if let Err(e) = self.stop_gateway_process(&gateway_id).await {
                error!("Failed to stop process for gateway {}: {}", gateway_id, e);
                // Continue stopping other processes
            }
        }

        debug!("GatewayOrchestrator shutdown complete");
        Ok(())
    }

    /// Start a gateway process
    #[instrument(skip(self), fields(gateway_id = %gateway.gateway_id))]
    async fn start_gateway_process(&self, gateway: &Gateway) -> DomainResult<()> {
        // Check if process already exists
        if self.process_store.exists(&gateway.gateway_id).await? {
            warn!(
                "process already exists for gateway {}, skipping",
                gateway.gateway_id
            );
            return Ok(());
        }

        // Get the appropriate runner for this gateway type
        let runner = self.runner_factory.create_runner(&gateway.gateway_config)?;
        let gateway_type = self
            .runner_factory
            .gateway_type_name(&gateway.gateway_config);

        let gateway_clone = gateway.clone();
        let config = self.config.clone();
        let process_token = CancellationToken::new();
        let process_token_clone = process_token.clone();
        let shutdown_token = self.shutdown_token.clone();
        let producer = Arc::clone(&self.raw_envelope_producer);

        // Spawn gateway process using the appropriate runner
        let join_handle = tokio::spawn(async move {
            runner
                .run(
                    gateway_clone,
                    config,
                    process_token_clone,
                    shutdown_token,
                    producer,
                )
                .await;
        });

        // Store handle
        let handle = GatewayProcessHandle::new(join_handle, process_token, gateway.clone());
        self.process_store
            .upsert(gateway.gateway_id.clone(), handle)
            .await?;

        info!(
            gateway_type = gateway_type,
            "started gateway process for {}", gateway.gateway_id
        );
        Ok(())
    }

    /// Stop a gateway process
    #[instrument(skip(self), fields(gateway_id = %gateway_id))]
    async fn stop_gateway_process(&self, gateway_id: &str) -> DomainResult<()> {
        match self.process_store.remove(gateway_id).await? {
            Some(handle) => {
                info!("stopping process for gateway {}", gateway_id);
                handle.cancel();

                // Wait for process to complete with timeout
                let timeout = tokio::time::Duration::from_secs(5);
                match tokio::time::timeout(timeout, handle.join_handle).await {
                    Ok(Ok(())) => {
                        debug!("process for gateway {} stopped gracefully", gateway_id);
                    }
                    Ok(Err(e)) => {
                        error!("process for gateway {} panicked: {:?}", gateway_id, e);
                    }
                    Err(_) => {
                        warn!(
                            "process for gateway {} did not stop within timeout",
                            gateway_id
                        );
                        // Process will be dropped and cleaned up by tokio
                    }
                }

                Ok(())
            }
            None => {
                warn!("no process found for gateway {}", gateway_id);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GatewayProcessStore, GatewayRunner, InMemoryGatewayProcessStore};
    use async_trait::async_trait;
    use common::domain::{
        EmqxGatewayConfig, GatewayConfig, GetGatewayRepoInput, MockGatewayRepository,
        MockRawEnvelopeProducer,
    };

    // Mock runner for testing - immediately completes
    struct MockGatewayRunner;

    #[async_trait]
    impl GatewayRunner for MockGatewayRunner {
        async fn run(
            &self,
            _gateway: Gateway,
            _config: GatewayOrchestrationServiceConfig,
            process_token: CancellationToken,
            shutdown_token: CancellationToken,
            _raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
        ) {
            // Wait for cancellation
            tokio::select! {
                _ = process_token.cancelled() => {}
                _ = shutdown_token.cancelled() => {}
            }
        }

        fn gateway_type(&self) -> &'static str {
            "MOCK_EMQX"
        }
    }

    // Helper to create mock producer
    fn create_mock_producer() -> Arc<dyn RawEnvelopeProducer> {
        let mut mock = MockRawEnvelopeProducer::new();
        mock.expect_publish_raw_envelope().returning(|_| Ok(()));
        Arc::new(mock)
    }

    // Helper to create a factory with mock runner registered
    fn create_test_factory() -> GatewayRunnerFactory {
        let mut factory = GatewayRunnerFactory::new();
        factory.register_emqx(|| Arc::new(MockGatewayRunner));
        factory
    }

    // Helper to create test gateway
    fn create_test_gateway(gateway_id: &str, org_id: &str) -> Gateway {
        Gateway {
            gateway_id: gateway_id.to_string(),
            organization_id: org_id.to_string(),
            name: format!("Test Gateway {}", gateway_id),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            }),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn test_start_loads_all_gateways() {
        let mut mock_repo = MockGatewayRepository::new();

        let gateways = vec![
            create_test_gateway("gw1", "org1"),
            create_test_gateway("gw2", "org1"),
        ];

        mock_repo
            .expect_list_all_gateways()
            .times(1)
            .returning(move || Ok(gateways.clone()));

        let store = Arc::new(InMemoryGatewayProcessStore::new());
        let orchestrator = GatewayOrchestrationService::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestrationServiceConfig::default(),
            CancellationToken::new(),
            create_mock_producer(),
            create_test_factory(),
        );

        let result = orchestrator.launch_gateways().await;
        assert!(result.is_ok());

        // Verify processes were started
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_handle_gateway_created_starts_process() {
        let mock_repo = MockGatewayRepository::new();
        let store = Arc::new(InMemoryGatewayProcessStore::new());

        let orchestrator = GatewayOrchestrationService::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestrationServiceConfig::default(),
            CancellationToken::new(),
            create_mock_producer(),
            create_test_factory(),
        );

        let gateway = create_test_gateway("gw1", "org1");
        let result = orchestrator.handle_gateway_created(gateway).await;
        assert!(result.is_ok());

        // Verify process was started
        assert_eq!(store.count().await.unwrap(), 1);
        assert!(store.exists("gw1").await.unwrap());
    }

    #[tokio::test]
    async fn test_handle_gateway_deleted_stops_process() {
        let mock_repo = MockGatewayRepository::new();
        let store = Arc::new(InMemoryGatewayProcessStore::new());

        let orchestrator = GatewayOrchestrationService::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestrationServiceConfig::default(),
            CancellationToken::new(),
            create_mock_producer(),
            create_test_factory(),
        );

        // Start a process
        let gateway = create_test_gateway("gw1", "org1");
        orchestrator.handle_gateway_created(gateway).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        // Delete it
        let result = orchestrator.handle_gateway_deleted("gw1").await;
        assert!(result.is_ok());

        // Give time for graceful shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify process was stopped
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_handle_gateway_updated_with_config_change() {
        let mut mock_repo = MockGatewayRepository::new();

        // Setup mock to return the old gateway when get_gateway is called
        let old_gateway = create_test_gateway("gw1", "org1");
        let old_gateway_clone = old_gateway.clone();

        mock_repo
            .expect_get_gateway()
            .withf(|input: &GetGatewayRepoInput| {
                input.gateway_id == "gw1" && input.organization_id == "org1"
            })
            .times(1)
            .returning(move |_| Ok(Some(old_gateway_clone.clone())));

        let store = Arc::new(InMemoryGatewayProcessStore::new());

        let orchestrator = GatewayOrchestrationService::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestrationServiceConfig::default(),
            CancellationToken::new(),
            create_mock_producer(),
            create_test_factory(),
        );

        // Start a process
        orchestrator
            .handle_gateway_created(old_gateway.clone())
            .await
            .unwrap();

        // Update with different config
        let mut new_gateway = old_gateway.clone();
        new_gateway.gateway_config = GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt.newhost.com:8883".to_string(),
        });

        let result = orchestrator.handle_gateway_updated(new_gateway).await;
        assert!(result.is_ok());

        // Give time for restart
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Process should still exist (restarted)
        assert!(store.exists("gw1").await.unwrap());
    }

    #[tokio::test]
    async fn test_handle_gateway_updated_with_soft_delete() {
        let mock_repo = MockGatewayRepository::new();
        let store = Arc::new(InMemoryGatewayProcessStore::new());

        let orchestrator = GatewayOrchestrationService::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestrationServiceConfig::default(),
            CancellationToken::new(),
            create_mock_producer(),
            create_test_factory(),
        );

        // Start a process
        let old_gateway = create_test_gateway("gw1", "org1");
        orchestrator
            .handle_gateway_created(old_gateway.clone())
            .await
            .unwrap();

        // Update with deleted_at set
        let mut new_gateway = old_gateway.clone();
        new_gateway.deleted_at = Some(chrono::Utc::now());

        let result = orchestrator.handle_gateway_updated(new_gateway).await;
        assert!(result.is_ok());

        // Give time for shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Process should be removed
        assert_eq!(store.count().await.unwrap(), 0);
    }
}

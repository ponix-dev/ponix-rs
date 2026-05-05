use crate::domain::{
    hash_gateway_connection, DeploymentHandleStore, GatewayDeployer,
    GatewayOrchestrationServiceConfig, GatewayRunner,
};
use common::domain::{
    DomainError, DomainResult, Gateway, GatewayRepository, GetGatewayRepoInput, RawEnvelopeProducer,
};

use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

pub struct GatewayOrchestrationService {
    gateway_repository: Arc<dyn GatewayRepository>,
    handle_store: Arc<dyn DeploymentHandleStore>,
    deployer: Arc<dyn GatewayDeployer>,
    config: GatewayOrchestrationServiceConfig,
    shutdown_token: CancellationToken,
    raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
    runner: Arc<dyn GatewayRunner>,
}

impl GatewayOrchestrationService {
    pub fn new(
        gateway_repository: Arc<dyn GatewayRepository>,
        handle_store: Arc<dyn DeploymentHandleStore>,
        deployer: Arc<dyn GatewayDeployer>,
        config: GatewayOrchestrationServiceConfig,
        shutdown_token: CancellationToken,
        raw_envelope_producer: Arc<dyn RawEnvelopeProducer>,
        runner: Arc<dyn GatewayRunner>,
    ) -> Self {
        Self {
            gateway_repository,
            handle_store,
            deployer,
            config,
            shutdown_token,
            raw_envelope_producer,
            runner,
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

    /// Handle gateway created event.
    ///
    /// CDC events do not carry credentials (excluded from the publication).
    /// We fetch the full record from the repository before starting the
    /// runner so the MQTT client gets the credentials it needs.
    #[instrument(skip(self), fields(gateway_id = %gateway.gateway_id))]
    pub async fn handle_gateway_created(&self, gateway: Gateway) -> DomainResult<()> {
        debug!("handling gateway created: {}", gateway.gateway_id);
        let full = self.fetch_full_gateway(&gateway).await?;
        self.start_gateway_process(&full).await
    }

    /// Handle gateway updated event
    #[instrument(skip(self), fields(gateway_id = %gateway.gateway_id))]
    pub async fn handle_gateway_updated(&self, gateway: Gateway) -> DomainResult<()> {
        debug!("handling gateway updated: {}", gateway.gateway_id);

        if gateway.deleted_at.is_some() {
            debug!(
                "gateway {} has been soft deleted, stopping process",
                gateway.gateway_id
            );
            return self.stop_gateway_process(&gateway.gateway_id).await;
        }

        // Always fetch the full record so we have credentials for the new hash
        // and for the new runner.
        let full = self.fetch_full_gateway(&gateway).await?;

        let connection_changed = match self
            .handle_store
            .get_config_hash(&gateway.gateway_id)
            .await?
        {
            Some(old_hash) => old_hash != hash_gateway_connection(&full),
            None => true,
        };

        if connection_changed {
            debug!(
                "gateway {} connection changed, restarting process",
                gateway.gateway_id
            );
            self.stop_gateway_process(&gateway.gateway_id).await?;
            self.start_gateway_process(&full).await?;
        } else {
            debug!(
                "gateway {} updated but connection unchanged, no action needed",
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

        let gateway_ids = self.handle_store.list_gateway_ids().await?;
        info!("stopping {} gateway processes", gateway_ids.len());

        for gateway_id in gateway_ids {
            if let Err(e) = self.stop_gateway_process(&gateway_id).await {
                error!("Failed to stop process for gateway {}: {}", gateway_id, e);
            }
        }

        debug!("GatewayOrchestrator shutdown complete");
        Ok(())
    }

    /// Look up the full gateway record from PostgreSQL.
    ///
    /// CDC events arrive without credentials (the publication excludes the
    /// `username`/`password` columns), so we read the full record from the
    /// repository whenever we need to act on a gateway.
    async fn fetch_full_gateway(&self, gateway: &Gateway) -> DomainResult<Gateway> {
        let repo_input = GetGatewayRepoInput {
            gateway_id: gateway.gateway_id.clone(),
            organization_id: gateway.organization_id.clone(),
        };

        match self.gateway_repository.get_gateway(repo_input).await? {
            Some(full) => Ok(full),
            None => Err(DomainError::GatewayNotFound(gateway.gateway_id.clone())),
        }
    }

    /// Start a gateway process
    #[instrument(skip(self), fields(gateway_id = %gateway.gateway_id))]
    async fn start_gateway_process(&self, gateway: &Gateway) -> DomainResult<()> {
        if self.handle_store.exists(&gateway.gateway_id).await? {
            warn!(
                "process already exists for gateway {}, skipping",
                gateway.gateway_id
            );
            return Ok(());
        }

        let handle = self
            .deployer
            .deploy(
                gateway,
                Arc::clone(&self.runner),
                self.config.clone(),
                self.shutdown_token.clone(),
                Arc::clone(&self.raw_envelope_producer),
            )
            .await?;

        self.handle_store
            .upsert(gateway.gateway_id.clone(), handle)
            .await?;

        // Never log credentials.
        info!(
            broker_url = %gateway.broker_url,
            authenticated = gateway.credentials.is_some(),
            "started gateway process for {}", gateway.gateway_id
        );
        Ok(())
    }

    /// Stop a gateway process
    #[instrument(skip(self), fields(gateway_id = %gateway_id))]
    async fn stop_gateway_process(&self, gateway_id: &str) -> DomainResult<()> {
        match self.handle_store.remove(gateway_id).await? {
            Some(mut handle) => {
                info!("stopping process for gateway {}", gateway_id);
                handle.cancel();
                if let Err(e) = handle.wait_for_stop(Duration::from_secs(5)).await {
                    error!("error waiting for gateway {} to stop: {}", gateway_id, e);
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
    use crate::domain::{InMemoryDeploymentHandleStore, InProcessDeployer};
    use async_trait::async_trait;
    use common::domain::{MockGatewayRepository, MockRawEnvelopeProducer, MqttCredentials};

    /// Mock runner for testing — waits for cancellation, then completes.
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
            tokio::select! {
                _ = process_token.cancelled() => {}
                _ = shutdown_token.cancelled() => {}
            }
        }
    }

    fn create_mock_producer() -> Arc<dyn RawEnvelopeProducer> {
        let mut mock = MockRawEnvelopeProducer::new();
        mock.expect_publish_raw_envelope().returning(|_| Ok(()));
        Arc::new(mock)
    }

    fn create_test_gateway(gateway_id: &str, org_id: &str) -> Gateway {
        Gateway {
            gateway_id: gateway_id.to_string(),
            organization_id: org_id.to_string(),
            name: format!("Test Gateway {}", gateway_id),
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            credentials: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
            deleted_at: None,
        }
    }

    fn expect_get_gateway(repo: &mut MockGatewayRepository, gateway: Gateway) {
        let returned = gateway.clone();
        repo.expect_get_gateway()
            .withf(move |input| input.gateway_id == gateway.gateway_id)
            .returning(move |_| Ok(Some(returned.clone())));
    }

    fn create_test_service(
        mock_repo: MockGatewayRepository,
        store: Arc<InMemoryDeploymentHandleStore>,
    ) -> GatewayOrchestrationService {
        GatewayOrchestrationService::new(
            Arc::new(mock_repo),
            store,
            Arc::new(InProcessDeployer),
            GatewayOrchestrationServiceConfig::default(),
            CancellationToken::new(),
            create_mock_producer(),
            Arc::new(MockGatewayRunner),
        )
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

        let store = Arc::new(InMemoryDeploymentHandleStore::new());
        let orchestrator = create_test_service(mock_repo, store.clone());

        orchestrator.launch_gateways().await.unwrap();
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_handle_gateway_created_starts_process() {
        let mut mock_repo = MockGatewayRepository::new();
        let gateway = create_test_gateway("gw1", "org1");
        expect_get_gateway(&mut mock_repo, gateway.clone());

        let store = Arc::new(InMemoryDeploymentHandleStore::new());
        let orchestrator = create_test_service(mock_repo, store.clone());

        orchestrator.handle_gateway_created(gateway).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 1);
        assert!(store.exists("gw1").await.unwrap());
    }

    #[tokio::test]
    async fn test_handle_gateway_created_fetches_credentials_from_repo() {
        // CDC event arrives without credentials. The orchestrator should
        // fetch the full gateway from the repository so the runner sees them.
        let mut mock_repo = MockGatewayRepository::new();
        let cdc_event_gateway = create_test_gateway("gw1", "org1");
        let mut full_gateway = cdc_event_gateway.clone();
        full_gateway.credentials = Some(MqttCredentials {
            username: "alice".to_string(),
            password: "s3cret".to_string(),
        });

        expect_get_gateway(&mut mock_repo, full_gateway);

        let store = Arc::new(InMemoryDeploymentHandleStore::new());
        let orchestrator = create_test_service(mock_repo, store.clone());

        orchestrator
            .handle_gateway_created(cdc_event_gateway)
            .await
            .unwrap();

        assert!(store.exists("gw1").await.unwrap());
    }

    #[tokio::test]
    async fn test_handle_gateway_deleted_stops_process() {
        let mut mock_repo = MockGatewayRepository::new();
        let gateway = create_test_gateway("gw1", "org1");
        expect_get_gateway(&mut mock_repo, gateway.clone());

        let store = Arc::new(InMemoryDeploymentHandleStore::new());
        let orchestrator = create_test_service(mock_repo, store.clone());

        orchestrator.handle_gateway_created(gateway).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        orchestrator.handle_gateway_deleted("gw1").await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(store.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_handle_gateway_updated_with_connection_change() {
        let mut mock_repo = MockGatewayRepository::new();

        let old_gateway = create_test_gateway("gw1", "org1");
        let mut new_gateway = old_gateway.clone();
        new_gateway.broker_url = "mqtt://mqtt.newhost.com:8883".to_string();

        // First call (create) returns old, subsequent calls (update) return new
        let create_resp = old_gateway.clone();
        let update_resp = new_gateway.clone();
        let mut call_count = 0;
        mock_repo.expect_get_gateway().returning(move |_| {
            call_count += 1;
            Ok(Some(if call_count == 1 {
                create_resp.clone()
            } else {
                update_resp.clone()
            }))
        });

        let store = Arc::new(InMemoryDeploymentHandleStore::new());
        let orchestrator = create_test_service(mock_repo, store.clone());

        orchestrator
            .handle_gateway_created(old_gateway.clone())
            .await
            .unwrap();

        let expected_hash = hash_gateway_connection(&new_gateway);

        orchestrator
            .handle_gateway_updated(new_gateway)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        assert!(store.exists("gw1").await.unwrap());
        let stored_hash = store.get_config_hash("gw1").await.unwrap();
        assert_eq!(stored_hash, Some(expected_hash));
    }

    #[tokio::test]
    async fn test_handle_gateway_updated_with_soft_delete() {
        let mut mock_repo = MockGatewayRepository::new();
        let old_gateway = create_test_gateway("gw1", "org1");
        expect_get_gateway(&mut mock_repo, old_gateway.clone());

        let store = Arc::new(InMemoryDeploymentHandleStore::new());
        let orchestrator = create_test_service(mock_repo, store.clone());

        orchestrator
            .handle_gateway_created(old_gateway.clone())
            .await
            .unwrap();

        let mut new_gateway = old_gateway.clone();
        new_gateway.deleted_at = Some(chrono::Utc::now());

        orchestrator
            .handle_gateway_updated(new_gateway)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(store.count().await.unwrap(), 0);
    }
}

use crate::error::DomainResult;
use crate::gateway::Gateway;
use crate::gateway_orchestrator_config::{GatewayOrchestratorConfig, PRINT_INTERVAL_SECS};
use crate::gateway_process::GatewayProcessHandle;
use crate::gateway_process_store::GatewayProcessStore;
use crate::repository::GatewayRepository;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct GatewayOrchestrator {
    gateway_repository: Arc<dyn GatewayRepository>,
    process_store: Arc<dyn GatewayProcessStore>,
    config: GatewayOrchestratorConfig,
    shutdown_token: CancellationToken,
}

impl GatewayOrchestrator {
    pub fn new(
        gateway_repository: Arc<dyn GatewayRepository>,
        process_store: Arc<dyn GatewayProcessStore>,
        config: GatewayOrchestratorConfig,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            gateway_repository,
            process_store,
            config,
            shutdown_token,
        }
    }

    /// Load all non-deleted gateways and start their processes
    pub async fn start(&self) -> DomainResult<()> {
        info!("Starting GatewayOrchestrator - loading all non-deleted gateways");

        let gateways = self.gateway_repository.list_all_gateways().await?;
        info!("Found {} non-deleted gateways to start", gateways.len());

        for gateway in gateways {
            if let Err(e) = self.start_gateway_process(&gateway).await {
                error!(
                    "Failed to start process for gateway {}: {}",
                    gateway.gateway_id, e
                );
                // Continue starting other processes even if one fails
            }
        }

        Ok(())
    }

    /// Handle gateway created event
    pub async fn handle_gateway_created(&self, gateway: Gateway) -> DomainResult<()> {
        info!("Handling gateway created: {}", gateway.gateway_id);
        self.start_gateway_process(&gateway).await
    }

    /// Handle gateway updated event
    pub async fn handle_gateway_updated(
        &self,
        gateway: Gateway,
    ) -> DomainResult<()> {
        info!("Handling gateway updated: {}", gateway.gateway_id);

        // Check if soft deleted
        if gateway.deleted_at.is_some() {
            info!(
                "Gateway {} has been soft deleted, stopping process",
                gateway.gateway_id
            );
            return self.stop_gateway_process(&gateway.gateway_id).await;
        }

        let config_changed = match self.gateway_repository.get_gateway(&gateway.gateway_id).await? {
                Some(old_gateway) => old_gateway.gateway_config != gateway.gateway_config,
                None => {
                    warn!(
                        "Old gateway state not found for {}, starting new process",
                        gateway.gateway_id
                    );
                    true
                },
            };
                

        if config_changed {
            info!(
                "Gateway {} config changed, restarting process",
                gateway.gateway_id
            );
            // Stop existing process
            self.stop_gateway_process(&gateway.gateway_id).await?;
            // Start with new config
            self.start_gateway_process(&gateway).await?;
        } else {
            info!(
                "Gateway {} updated but config unchanged, no action needed",
                gateway.gateway_id
            );
        }

        Ok(())
    }

    /// Handle gateway deleted event
    pub async fn handle_gateway_deleted(&self, gateway_id: &str) -> DomainResult<()> {
        info!("Handling gateway deleted: {}", gateway_id);
        self.stop_gateway_process(gateway_id).await
    }

    /// Stop all running gateway processes
    pub async fn shutdown(&self) -> DomainResult<()> {
        info!("Shutting down GatewayOrchestrator");

        let gateway_ids = self.process_store.list_gateway_ids().await?;
        info!("Stopping {} gateway processes", gateway_ids.len());

        for gateway_id in gateway_ids {
            if let Err(e) = self.stop_gateway_process(&gateway_id).await {
                error!("Failed to stop process for gateway {}: {}", gateway_id, e);
                // Continue stopping other processes
            }
        }

        info!("GatewayOrchestrator shutdown complete");
        Ok(())
    }

    /// Start a gateway process
    async fn start_gateway_process(&self, gateway: &Gateway) -> DomainResult<()> {
        // Check if process already exists
        if self
            .process_store
            .exists(&gateway.gateway_id)
            .await?
        {
            warn!(
                "Process already exists for gateway {}, skipping",
                gateway.gateway_id
            );
            return Ok(());
        }

        let gateway_clone = gateway.clone();
        let config = self.config.clone();
        let process_token = CancellationToken::new();
        let process_token_clone = process_token.clone();
        let shutdown_token = self.shutdown_token.clone();

        // Spawn print process
        let join_handle = tokio::spawn(async move {
            run_gateway_print_process(
                gateway_clone,
                config,
                process_token_clone,
                shutdown_token,
            )
            .await;
        });

        // Store handle
        let handle = GatewayProcessHandle::new(join_handle, process_token, gateway.clone());
        self.process_store
            .upsert(gateway.gateway_id.clone(), handle)
            .await?;

        info!("Started process for gateway {}", gateway.gateway_id);
        Ok(())
    }

    /// Stop a gateway process
    async fn stop_gateway_process(&self, gateway_id: &str) -> DomainResult<()> {
        match self.process_store.remove(gateway_id).await? {
            Some(handle) => {
                info!("Stopping process for gateway {}", gateway_id);
                handle.cancel();

                // Wait for process to complete with timeout
                let timeout = tokio::time::Duration::from_secs(5);
                match tokio::time::timeout(timeout, handle.join_handle).await {
                    Ok(Ok(())) => {
                        info!("Process for gateway {} stopped gracefully", gateway_id);
                    }
                    Ok(Err(e)) => {
                        error!("Process for gateway {} panicked: {:?}", gateway_id, e);
                    }
                    Err(_) => {
                        warn!("Process for gateway {} did not stop within timeout", gateway_id);
                        // Process will be dropped and cleaned up by tokio
                    }
                }

                Ok(())
            }
            None => {
                warn!("No process found for gateway {}", gateway_id);
                Ok(())
            }
        }
    }
}

/// Run the gateway print process
async fn run_gateway_print_process(
    gateway: Gateway,
    config: GatewayOrchestratorConfig,
    process_token: CancellationToken,
    shutdown_token: CancellationToken,
) {
    info!(
        "Starting print process for gateway: {} ({})",
        gateway.gateway_id, gateway.organization_id
    );

    let mut retry_count = 0;

    loop {
        // Check for cancellation
        if process_token.is_cancelled() || shutdown_token.is_cancelled() {
            info!("Print process for gateway {} cancelled", gateway.gateway_id);
            break;
        }

        // Print gateway config
        match print_gateway_config(&gateway) {
            Ok(()) => {
                retry_count = 0; // Reset retry count on success
            }
            Err(e) => {
                error!(
                    "Error printing config for gateway {}: {}",
                    gateway.gateway_id, e
                );

                retry_count += 1;
                if retry_count >= config.max_retry_attempts {
                    error!(
                        "Max retry attempts ({}) reached for gateway {}, stopping process",
                        config.max_retry_attempts, gateway.gateway_id
                    );
                    break;
                }

                info!(
                    "Retrying print for gateway {} (attempt {}/{})",
                    gateway.gateway_id, retry_count, config.max_retry_attempts
                );

                // Wait before retry
                tokio::select! {
                    _ = process_token.cancelled() => break,
                    _ = shutdown_token.cancelled() => break,
                    _ = tokio::time::sleep(config.retry_delay()) => {}
                }
                continue;
            }
        }

        // Wait for next print interval (hard-coded 5 seconds)
        tokio::select! {
            _ = process_token.cancelled() => {
                info!("Print process for gateway {} cancelled", gateway.gateway_id);
                break;
            }
            _ = shutdown_token.cancelled() => {
                info!("Print process for gateway {} shutdown signal received", gateway.gateway_id);
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(PRINT_INTERVAL_SECS)) => {}
        }
    }

    info!("Print process for gateway {} stopped", gateway.gateway_id);
}

/// Print gateway configuration to stdout
fn print_gateway_config(gateway: &Gateway) -> DomainResult<()> {
    match &gateway.gateway_config {
        crate::GatewayConfig::Emqx(emqx) => {
            println!(
                "[GATEWAY {}] org={} type={} broker_url={}",
                gateway.gateway_id, gateway.organization_id, gateway.gateway_type, emqx.broker_url
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::MockGatewayRepository;
    use crate::gateway_process_store::GatewayProcessStore;
    use crate::in_memory_gateway_process_store::InMemoryGatewayProcessStore;

    // Helper to create test gateway
    fn create_test_gateway(gateway_id: &str, org_id: &str) -> Gateway {
        use crate::{EmqxGatewayConfig, GatewayConfig};

        Gateway {
            gateway_id: gateway_id.to_string(),
            organization_id: org_id.to_string(),
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
        let orchestrator = GatewayOrchestrator::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestratorConfig::default(),
            CancellationToken::new(),
        );

        let result = orchestrator.start().await;
        assert!(result.is_ok());

        // Verify processes were started
        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_handle_gateway_created_starts_process() {
        let mock_repo = MockGatewayRepository::new();
        let store = Arc::new(InMemoryGatewayProcessStore::new());

        let orchestrator = GatewayOrchestrator::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestratorConfig::default(),
            CancellationToken::new(),
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

        let orchestrator = GatewayOrchestrator::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestratorConfig::default(),
            CancellationToken::new(),
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
        use crate::{EmqxGatewayConfig, GatewayConfig};

        let mut mock_repo = MockGatewayRepository::new();

        // Setup mock to return the old gateway when get_gateway is called
        let old_gateway = create_test_gateway("gw1", "org1");
        let old_gateway_clone = old_gateway.clone();

        mock_repo
            .expect_get_gateway()
            .withf(|gateway_id| gateway_id == "gw1")
            .times(1)
            .returning(move |_| Ok(Some(old_gateway_clone.clone())));

        let store = Arc::new(InMemoryGatewayProcessStore::new());

        let orchestrator = GatewayOrchestrator::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestratorConfig::default(),
            CancellationToken::new(),
        );

        // Start a process
        orchestrator.handle_gateway_created(old_gateway.clone()).await.unwrap();

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

        let orchestrator = GatewayOrchestrator::new(
            Arc::new(mock_repo),
            store.clone(),
            GatewayOrchestratorConfig::default(),
            CancellationToken::new(),
        );

        // Start a process
        let old_gateway = create_test_gateway("gw1", "org1");
        orchestrator.handle_gateway_created(old_gateway.clone()).await.unwrap();

        // Update with deleted_at set
        let mut new_gateway = old_gateway.clone();
        new_gateway.deleted_at = Some(chrono::Utc::now());

        let result = orchestrator.handle_gateway_updated( new_gateway).await;
        assert!(result.is_ok());

        // Give time for shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Process should be removed
        assert_eq!(store.count().await.unwrap(), 0);
    }
}

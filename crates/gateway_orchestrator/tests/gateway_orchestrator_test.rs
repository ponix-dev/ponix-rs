#![cfg(feature = "integration-tests")]

use async_trait::async_trait;
use common::domain::{
    CreateGatewayInputWithId, DeleteGatewayInput, DomainError, DomainResult, EmqxGatewayConfig,
    Gateway, GatewayConfig, GatewayRepository, GetGatewayInput, MockRawEnvelopeProducer,
    RawEnvelopeProducer, UpdateGatewayInput,
};
use gateway_orchestrator::domain::{
    GatewayOrchestrationService, GatewayOrchestrationServiceConfig, GatewayProcessStore,
    GatewayRunner, GatewayRunnerFactory, InMemoryGatewayProcessStore,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

// Mock runner for testing - waits for cancellation
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

    fn gateway_type(&self) -> &'static str {
        "MOCK_EMQX"
    }
}

// Helper to create mock producer for tests
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

// Mock implementation of GatewayRepository for testing
mod mocks {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    pub struct MockGatewayRepository {
        gateways: Mutex<HashMap<String, Gateway>>,
    }

    impl MockGatewayRepository {
        pub fn new() -> Self {
            Self {
                gateways: Mutex::new(HashMap::new()),
            }
        }

        pub fn add_gateway(&self, gateway: Gateway) {
            let mut gateways = self.gateways.lock().unwrap();
            gateways.insert(gateway.gateway_id.clone(), gateway);
        }
    }

    #[async_trait]
    impl GatewayRepository for MockGatewayRepository {
        async fn create_gateway(&self, _input: CreateGatewayInputWithId) -> DomainResult<Gateway> {
            unimplemented!("Not needed for orchestrator tests")
        }

        async fn get_gateway(&self, input: GetGatewayInput) -> DomainResult<Option<Gateway>> {
            let gateways = self.gateways.lock().unwrap();
            Ok(gateways.get(&input.gateway_id).cloned())
        }

        async fn list_gateways(&self, _organization_id: &str) -> DomainResult<Vec<Gateway>> {
            unimplemented!("Not needed for orchestrator tests")
        }

        async fn list_all_gateways(&self) -> DomainResult<Vec<Gateway>> {
            let gateways = self.gateways.lock().unwrap();
            Ok(gateways.values().cloned().collect())
        }

        async fn update_gateway(&self, input: UpdateGatewayInput) -> DomainResult<Gateway> {
            let mut gateways = self.gateways.lock().unwrap();
            if let Some(gateway) = gateways.get_mut(&input.gateway_id) {
                if let Some(config) = input.gateway_config {
                    gateway.gateway_config = config;
                }
                if let Some(gateway_type) = input.gateway_type {
                    gateway.gateway_type = gateway_type;
                }
                gateway.updated_at = Some(chrono::Utc::now());
                Ok(gateway.clone())
            } else {
                Err(DomainError::GatewayNotFound(input.gateway_id.clone()))
            }
        }

        async fn delete_gateway(&self, input: DeleteGatewayInput) -> DomainResult<()> {
            let mut gateways = self.gateways.lock().unwrap();
            if let Some(gateway) = gateways.get_mut(&input.gateway_id) {
                gateway.deleted_at = Some(chrono::Utc::now());
                Ok(())
            } else {
                Err(DomainError::GatewayNotFound(input.gateway_id))
            }
        }
    }
}

fn create_test_gateway(id: &str, org_id: &str, broker_url: &str) -> Gateway {
    Gateway {
        gateway_id: id.to_string(),
        organization_id: org_id.to_string(),
        gateway_type: "EMQX".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: broker_url.to_string(),
            subscription_group: "ponix".to_string(),
        }),
        created_at: Some(chrono::Utc::now()),
        updated_at: None,
        deleted_at: None,
    }
}

#[tokio::test]
async fn test_orchestrator_starts_existing_gateways() {
    // Arrange
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let config = GatewayOrchestrationServiceConfig::default();
    let shutdown_token = CancellationToken::new();

    // Add 3 test gateways to the repository
    gateway_repo.add_gateway(create_test_gateway(
        "gw-001",
        "org-001",
        "mqtt://test1.example.com:1883",
    ));
    gateway_repo.add_gateway(create_test_gateway(
        "gw-002",
        "org-001",
        "mqtt://test2.example.com:1883",
    ));
    gateway_repo.add_gateway(create_test_gateway(
        "gw-003",
        "org-002",
        "mqtt://test3.example.com:1883",
    ));

    let orchestrator = GatewayOrchestrationService::new(
        gateway_repo.clone(),
        process_store.clone(),
        config,
        shutdown_token.clone(),
        create_mock_producer(),
        create_test_factory(),
    );

    // Act
    orchestrator
        .launch_gateways()
        .await
        .expect("Failed to start orchestrator");

    // Give processes time to start
    sleep(Duration::from_millis(100)).await;

    // Assert
    let gateway_ids = process_store.list_gateway_ids().await.unwrap();
    assert_eq!(gateway_ids.len(), 3, "All 3 gateways should have started");
    assert!(gateway_ids.contains(&"gw-001".to_string()));
    assert!(gateway_ids.contains(&"gw-002".to_string()));
    assert!(gateway_ids.contains(&"gw-003".to_string()));

    // Verify all processes exist
    assert!(process_store.exists("gw-001").await.unwrap());
    assert!(process_store.exists("gw-002").await.unwrap());
    assert!(process_store.exists("gw-003").await.unwrap());

    // Cleanup
    shutdown_token.cancel();
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_created() {
    // Arrange
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let config = GatewayOrchestrationServiceConfig::default();
    let shutdown_token = CancellationToken::new();

    let orchestrator = GatewayOrchestrationService::new(
        gateway_repo.clone(),
        process_store.clone(),
        config,
        shutdown_token.clone(),
        create_mock_producer(),
        create_test_factory(),
    );

    // Start with no gateways
    orchestrator
        .launch_gateways()
        .await
        .expect("Failed to start orchestrator");

    // Act - handle gateway created event
    let new_gateway = create_test_gateway("gw-new", "org-001", "mqtt://new.example.com:1883");
    orchestrator
        .handle_gateway_created(new_gateway)
        .await
        .expect("Failed to handle gateway created");

    // Give process time to start
    sleep(Duration::from_millis(100)).await;

    // Assert
    let gateway_ids = process_store.list_gateway_ids().await.unwrap();
    assert_eq!(gateway_ids.len(), 1, "New gateway should have started");
    assert!(gateway_ids.contains(&"gw-new".to_string()));
    assert!(process_store.exists("gw-new").await.unwrap());

    // Cleanup
    shutdown_token.cancel();
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_updated_with_config_change() {
    // Arrange
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let config = GatewayOrchestrationServiceConfig::default();
    let shutdown_token = CancellationToken::new();

    // Add initial gateway
    let initial_gateway =
        create_test_gateway("gw-update", "org-001", "mqtt://original.example.com:1883");
    gateway_repo.add_gateway(initial_gateway);

    let orchestrator = GatewayOrchestrationService::new(
        gateway_repo.clone(),
        process_store.clone(),
        config,
        shutdown_token.clone(),
        create_mock_producer(),
        create_test_factory(),
    );

    orchestrator
        .launch_gateways()
        .await
        .expect("Failed to start orchestrator");
    sleep(Duration::from_millis(100)).await;

    // Verify process started
    assert!(process_store.exists("gw-update").await.unwrap());

    // Act - update gateway with new config
    let mut updated_gateway =
        create_test_gateway("gw-update", "org-001", "mqtt://updated.example.com:8883");
    updated_gateway.updated_at = Some(chrono::Utc::now());

    orchestrator
        .handle_gateway_updated(updated_gateway)
        .await
        .expect("Failed to handle gateway updated");

    // Give process time to restart
    sleep(Duration::from_millis(200)).await;

    // Assert - process should still exist after config change
    assert!(
        process_store.exists("gw-update").await.unwrap(),
        "Process should still exist after config change"
    );

    // Verify the process has the updated config
    let handle = process_store.get("gw-update").await.unwrap().unwrap();
    let GatewayConfig::Emqx(emqx) = &handle.gateway.gateway_config;
    assert_eq!(
        emqx.broker_url, "mqtt://updated.example.com:8883",
        "Process should have the updated broker URL"
    );

    // Cleanup
    shutdown_token.cancel();
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_soft_delete() {
    // Arrange
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let config = GatewayOrchestrationServiceConfig::default();
    let shutdown_token = CancellationToken::new();

    // Add initial gateway
    let gateway = create_test_gateway("gw-delete", "org-001", "mqtt://delete.example.com:1883");
    gateway_repo.add_gateway(gateway);

    let orchestrator = GatewayOrchestrationService::new(
        gateway_repo.clone(),
        process_store.clone(),
        config,
        shutdown_token.clone(),
        create_mock_producer(),
        create_test_factory(),
    );

    orchestrator
        .launch_gateways()
        .await
        .expect("Failed to start orchestrator");
    sleep(Duration::from_millis(100)).await;

    // Verify process started
    assert!(process_store.exists("gw-delete").await.unwrap());

    // Act - soft delete gateway (set deleted_at)
    let mut deleted_gateway =
        create_test_gateway("gw-delete", "org-001", "mqtt://delete.example.com:1883");
    deleted_gateway.deleted_at = Some(chrono::Utc::now());

    orchestrator
        .handle_gateway_updated(deleted_gateway)
        .await
        .expect("Failed to handle gateway soft delete");

    // Give process time to stop
    sleep(Duration::from_millis(200)).await;

    // Assert - process should be stopped
    assert!(
        !process_store.exists("gw-delete").await.unwrap(),
        "Process should be stopped after soft delete"
    );

    // Cleanup
    shutdown_token.cancel();
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_deleted() {
    // Arrange
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let config = GatewayOrchestrationServiceConfig::default();
    let shutdown_token = CancellationToken::new();

    // Add initial gateway
    let gateway = create_test_gateway("gw-hard", "org-001", "mqtt://hard.example.com:1883");
    gateway_repo.add_gateway(gateway);

    let orchestrator = GatewayOrchestrationService::new(
        gateway_repo.clone(),
        process_store.clone(),
        config,
        shutdown_token.clone(),
        create_mock_producer(),
        create_test_factory(),
    );

    orchestrator
        .launch_gateways()
        .await
        .expect("Failed to start orchestrator");
    sleep(Duration::from_millis(100)).await;

    // Verify process started
    assert!(process_store.exists("gw-hard").await.unwrap());

    // Act - hard delete gateway
    orchestrator
        .handle_gateway_deleted("gw-hard")
        .await
        .expect("Failed to handle gateway deleted");

    // Give process time to stop
    sleep(Duration::from_millis(200)).await;

    // Assert - process should be stopped
    assert!(
        !process_store.exists("gw-hard").await.unwrap(),
        "Process should be stopped after hard delete"
    );

    // Cleanup
    shutdown_token.cancel();
}

#[tokio::test]
async fn test_orchestrator_shutdown_stops_all_processes() {
    // Arrange
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let config = GatewayOrchestrationServiceConfig::default();
    let shutdown_token = CancellationToken::new();

    // Add multiple gateways
    gateway_repo.add_gateway(create_test_gateway(
        "gw-shutdown-1",
        "org-001",
        "mqtt://test1.example.com:1883",
    ));
    gateway_repo.add_gateway(create_test_gateway(
        "gw-shutdown-2",
        "org-001",
        "mqtt://test2.example.com:1883",
    ));
    gateway_repo.add_gateway(create_test_gateway(
        "gw-shutdown-3",
        "org-002",
        "mqtt://test3.example.com:1883",
    ));

    let orchestrator = GatewayOrchestrationService::new(
        gateway_repo.clone(),
        process_store.clone(),
        config,
        shutdown_token.clone(),
        create_mock_producer(),
        create_test_factory(),
    );

    orchestrator
        .launch_gateways()
        .await
        .expect("Failed to start orchestrator");
    sleep(Duration::from_millis(100)).await;

    // Verify all processes started
    assert_eq!(
        process_store.list_gateway_ids().await.unwrap().len(),
        3,
        "All processes should be started"
    );

    // Act - shutdown orchestrator
    orchestrator
        .shutdown()
        .await
        .expect("Failed to shutdown orchestrator");

    // Give processes time to stop
    sleep(Duration::from_millis(200)).await;

    // Assert - all processes should be stopped
    assert_eq!(
        process_store.list_gateway_ids().await.unwrap().len(),
        0,
        "All processes should be stopped after shutdown"
    );

    // Cleanup
    shutdown_token.cancel();
}

#[tokio::test]
async fn test_orchestrator_ignores_duplicate_create() {
    // Arrange
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let process_store = Arc::new(InMemoryGatewayProcessStore::new());
    let config = GatewayOrchestrationServiceConfig::default();
    let shutdown_token = CancellationToken::new();

    let gateway = create_test_gateway("gw-dup", "org-001", "mqtt://dup.example.com:1883");
    gateway_repo.add_gateway(gateway.clone());

    let orchestrator = GatewayOrchestrationService::new(
        gateway_repo.clone(),
        process_store.clone(),
        config,
        shutdown_token.clone(),
        create_mock_producer(),
        create_test_factory(),
    );

    orchestrator
        .launch_gateways()
        .await
        .expect("Failed to start orchestrator");
    sleep(Duration::from_millis(100)).await;

    let initial_count = process_store.list_gateway_ids().await.unwrap().len();
    assert_eq!(initial_count, 1);

    // Act - try to create the same gateway again
    let result = orchestrator.handle_gateway_created(gateway).await;

    // Assert - should succeed but not create duplicate process
    assert!(result.is_ok(), "Should handle duplicate gracefully");
    sleep(Duration::from_millis(100)).await;

    let final_count = process_store.list_gateway_ids().await.unwrap().len();
    assert_eq!(
        final_count, 1,
        "Should still have only 1 process after duplicate create"
    );

    // Cleanup
    shutdown_token.cancel();
}

#![cfg(feature = "integration-tests")]

use async_trait::async_trait;
use common::domain::{
    CreateGatewayRepoInput, DeleteGatewayRepoInput, DomainError, DomainResult, Gateway,
    GatewayRepository, GetGatewayRepoInput, ListGatewaysRepoInput, MockRawEnvelopeProducer,
    RawEnvelopeProducer, UpdateGatewayRepoInput,
};
use gateway_orchestrator::domain::{
    hash_gateway_connection, DeploymentHandleStore, GatewayOrchestrationService,
    GatewayOrchestrationServiceConfig, GatewayRunner, InMemoryDeploymentHandleStore,
    InProcessDeployer,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

/// Mock runner — waits for cancellation and exits cleanly.
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

mod mocks {
    use super::*;
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
            self.gateways
                .lock()
                .unwrap()
                .insert(gateway.gateway_id.clone(), gateway);
        }
    }

    #[async_trait]
    impl GatewayRepository for MockGatewayRepository {
        async fn create_gateway(&self, _input: CreateGatewayRepoInput) -> DomainResult<Gateway> {
            unimplemented!("Not needed for orchestrator tests")
        }

        async fn get_gateway(&self, input: GetGatewayRepoInput) -> DomainResult<Option<Gateway>> {
            Ok(self
                .gateways
                .lock()
                .unwrap()
                .get(&input.gateway_id)
                .cloned())
        }

        async fn list_gateways(&self, _input: ListGatewaysRepoInput) -> DomainResult<Vec<Gateway>> {
            unimplemented!("Not needed for orchestrator tests")
        }

        async fn list_all_gateways(&self) -> DomainResult<Vec<Gateway>> {
            Ok(self.gateways.lock().unwrap().values().cloned().collect())
        }

        async fn update_gateway(&self, input: UpdateGatewayRepoInput) -> DomainResult<Gateway> {
            let mut gateways = self.gateways.lock().unwrap();
            if let Some(gateway) = gateways.get_mut(&input.gateway_id) {
                if let Some(broker_url) = input.broker_url {
                    gateway.broker_url = broker_url;
                }
                if input.credentials.is_some() {
                    gateway.credentials = input.credentials;
                }
                gateway.updated_at = Some(chrono::Utc::now());
                Ok(gateway.clone())
            } else {
                Err(DomainError::GatewayNotFound(input.gateway_id))
            }
        }

        async fn delete_gateway(&self, input: DeleteGatewayRepoInput) -> DomainResult<()> {
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
        name: format!("Test Gateway {}", id),
        broker_url: broker_url.to_string(),
        credentials: None,
        created_at: Some(chrono::Utc::now()),
        updated_at: None,
        deleted_at: None,
    }
}

fn create_test_orchestrator(
    gateway_repo: Arc<mocks::MockGatewayRepository>,
    handle_store: Arc<InMemoryDeploymentHandleStore>,
    shutdown_token: CancellationToken,
) -> GatewayOrchestrationService {
    GatewayOrchestrationService::new(
        gateway_repo,
        handle_store,
        Arc::new(InProcessDeployer),
        GatewayOrchestrationServiceConfig::default(),
        shutdown_token,
        create_mock_producer(),
        Arc::new(MockGatewayRunner),
    )
}

#[tokio::test]
async fn test_orchestrator_starts_existing_gateways() {
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let handle_store = Arc::new(InMemoryDeploymentHandleStore::new());
    let shutdown_token = CancellationToken::new();

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

    let orchestrator = create_test_orchestrator(
        gateway_repo.clone(),
        handle_store.clone(),
        shutdown_token.clone(),
    );

    orchestrator.launch_gateways().await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let gateway_ids = handle_store.list_gateway_ids().await.unwrap();
    assert_eq!(gateway_ids.len(), 3);
    assert!(gateway_ids.contains(&"gw-001".to_string()));
    assert!(gateway_ids.contains(&"gw-002".to_string()));
    assert!(gateway_ids.contains(&"gw-003".to_string()));

    shutdown_token.cancel();
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_created() {
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let handle_store = Arc::new(InMemoryDeploymentHandleStore::new());
    let shutdown_token = CancellationToken::new();

    // Pre-populate the repo with the gateway the CDC event will reference,
    // since the orchestrator fetches credentials by gateway_id from the repo.
    let gateway = create_test_gateway("gw-new", "org-001", "mqtt://new.example.com:1883");
    gateway_repo.add_gateway(gateway.clone());

    let orchestrator = create_test_orchestrator(
        gateway_repo.clone(),
        handle_store.clone(),
        shutdown_token.clone(),
    );

    orchestrator.launch_gateways().await.unwrap();
    handle_store.remove("gw-new").await.unwrap(); // pretend launch didn't pick it up

    orchestrator.handle_gateway_created(gateway).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    assert!(handle_store.exists("gw-new").await.unwrap());

    shutdown_token.cancel();
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_updated_with_connection_change() {
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let handle_store = Arc::new(InMemoryDeploymentHandleStore::new());
    let shutdown_token = CancellationToken::new();

    let initial_gateway =
        create_test_gateway("gw-update", "org-001", "mqtt://original.example.com:1883");
    gateway_repo.add_gateway(initial_gateway);

    let orchestrator = create_test_orchestrator(
        gateway_repo.clone(),
        handle_store.clone(),
        shutdown_token.clone(),
    );

    orchestrator.launch_gateways().await.unwrap();
    sleep(Duration::from_millis(100)).await;
    assert!(handle_store.exists("gw-update").await.unwrap());

    // Update the gateway in the repository so the orchestrator picks up the
    // new connection details when it does its fetch.
    let updated = create_test_gateway("gw-update", "org-001", "mqtt://updated.example.com:8883");
    gateway_repo.add_gateway(updated.clone());

    orchestrator
        .handle_gateway_updated(updated.clone())
        .await
        .unwrap();
    sleep(Duration::from_millis(200)).await;

    assert!(handle_store.exists("gw-update").await.unwrap());
    let expected_hash = hash_gateway_connection(&updated);
    let stored_hash = handle_store.get_config_hash("gw-update").await.unwrap();
    assert_eq!(stored_hash, Some(expected_hash));

    shutdown_token.cancel();
    sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_soft_delete() {
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let handle_store = Arc::new(InMemoryDeploymentHandleStore::new());
    let shutdown_token = CancellationToken::new();

    gateway_repo.add_gateway(create_test_gateway(
        "gw-delete",
        "org-001",
        "mqtt://delete.example.com:1883",
    ));

    let orchestrator = create_test_orchestrator(
        gateway_repo.clone(),
        handle_store.clone(),
        shutdown_token.clone(),
    );

    orchestrator.launch_gateways().await.unwrap();
    sleep(Duration::from_millis(100)).await;
    assert!(handle_store.exists("gw-delete").await.unwrap());

    let mut deleted_gateway =
        create_test_gateway("gw-delete", "org-001", "mqtt://delete.example.com:1883");
    deleted_gateway.deleted_at = Some(chrono::Utc::now());

    orchestrator
        .handle_gateway_updated(deleted_gateway)
        .await
        .unwrap();
    sleep(Duration::from_millis(200)).await;
    assert!(!handle_store.exists("gw-delete").await.unwrap());

    shutdown_token.cancel();
}

#[tokio::test]
async fn test_orchestrator_handles_gateway_deleted() {
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let handle_store = Arc::new(InMemoryDeploymentHandleStore::new());
    let shutdown_token = CancellationToken::new();

    gateway_repo.add_gateway(create_test_gateway(
        "gw-hard",
        "org-001",
        "mqtt://hard.example.com:1883",
    ));

    let orchestrator = create_test_orchestrator(
        gateway_repo.clone(),
        handle_store.clone(),
        shutdown_token.clone(),
    );

    orchestrator.launch_gateways().await.unwrap();
    sleep(Duration::from_millis(100)).await;
    assert!(handle_store.exists("gw-hard").await.unwrap());

    orchestrator
        .handle_gateway_deleted("gw-hard")
        .await
        .unwrap();
    sleep(Duration::from_millis(200)).await;
    assert!(!handle_store.exists("gw-hard").await.unwrap());

    shutdown_token.cancel();
}

#[tokio::test]
async fn test_orchestrator_shutdown_stops_all_processes() {
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let handle_store = Arc::new(InMemoryDeploymentHandleStore::new());
    let shutdown_token = CancellationToken::new();

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

    let orchestrator = create_test_orchestrator(
        gateway_repo.clone(),
        handle_store.clone(),
        shutdown_token.clone(),
    );

    orchestrator.launch_gateways().await.unwrap();
    sleep(Duration::from_millis(100)).await;
    assert_eq!(handle_store.list_gateway_ids().await.unwrap().len(), 3);

    orchestrator.shutdown().await.unwrap();
    sleep(Duration::from_millis(200)).await;
    assert_eq!(handle_store.list_gateway_ids().await.unwrap().len(), 0);

    shutdown_token.cancel();
}

#[tokio::test]
async fn test_orchestrator_ignores_duplicate_create() {
    let gateway_repo = Arc::new(mocks::MockGatewayRepository::new());
    let handle_store = Arc::new(InMemoryDeploymentHandleStore::new());
    let shutdown_token = CancellationToken::new();

    let gateway = create_test_gateway("gw-dup", "org-001", "mqtt://dup.example.com:1883");
    gateway_repo.add_gateway(gateway.clone());

    let orchestrator = create_test_orchestrator(
        gateway_repo.clone(),
        handle_store.clone(),
        shutdown_token.clone(),
    );

    orchestrator.launch_gateways().await.unwrap();
    sleep(Duration::from_millis(100)).await;
    assert_eq!(handle_store.list_gateway_ids().await.unwrap().len(), 1);

    orchestrator.handle_gateway_created(gateway).await.unwrap();
    sleep(Duration::from_millis(100)).await;
    assert_eq!(handle_store.list_gateway_ids().await.unwrap().len(), 1);

    shutdown_token.cancel();
}

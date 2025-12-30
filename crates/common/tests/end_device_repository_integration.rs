#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateDeviceRepoInput, CreateEndDeviceDefinitionRepoInput, DeviceRepository,
    DomainError, EndDeviceDefinitionRepository, GetDeviceRepoInput,
    GetDeviceWithDefinitionRepoInput, ListDevicesRepoInput,
};
use common::postgres::{
    PostgresClient, PostgresDeviceRepository, PostgresEndDeviceDefinitionRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

async fn setup_test_db() -> (
    ContainerAsync<Postgres>,
    PostgresDeviceRepository,
    PostgresEndDeviceDefinitionRepository,
    PostgresClient,
) {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    // Run migrations
    let migrations_dir = format!(
        "{}/../../crates/init_process/migrations/postgres",
        env!("CARGO_MANIFEST_DIR")
    );
    let dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        host, port
    );
    let goose_path = which::which("goose").expect("goose binary not found");

    let migration_runner = MigrationRunner::new(
        goose_path.to_string_lossy().to_string(),
        migrations_dir,
        "postgres".to_string(),
        dsn.clone(),
    );

    migration_runner
        .run_migrations()
        .await
        .expect("Migrations failed");

    // Create client
    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .expect("Failed to create client");

    let device_repo = PostgresDeviceRepository::new(client.clone());
    let definition_repo = PostgresEndDeviceDefinitionRepository::new(client.clone());

    (postgres, device_repo, definition_repo, client)
}

async fn create_test_organization(client: &PostgresClient, org_id: &str) {
    let conn = client.get_connection().await.unwrap();
    conn.execute(
        "INSERT INTO organizations (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        &[&org_id, &"Test Organization"],
    )
    .await
    .unwrap();
}

async fn create_test_workspace(client: &PostgresClient, org_id: &str) -> String {
    // Ensure organization exists first
    create_test_organization(client, org_id).await;

    // Create workspace with unique ID
    let workspace_id = format!("ws-{}-{}", org_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let conn = client.get_connection().await.unwrap();
    conn.execute(
        "INSERT INTO workspaces (id, name, organization_id) VALUES ($1, $2, $3)",
        &[&workspace_id, &"Test Workspace", &org_id],
    )
    .await
    .unwrap();
    workspace_id
}

async fn create_test_definition(
    definition_repo: &PostgresEndDeviceDefinitionRepository,
    client: &PostgresClient,
    org_id: &str,
) -> String {
    // Create organization first
    create_test_organization(client, org_id).await;

    // Create definition with unique ID based on org and timestamp
    let def_id = format!("def-{}-{}", org_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let input = CreateEndDeviceDefinitionRepoInput {
        id: def_id.clone(),
        organization_id: org_id.to_string(),
        name: "Test Definition".to_string(),
        json_schema: "{}".to_string(),
        payload_conversion: "cayenne_lpp_decode(input)".to_string(),
    };
    definition_repo.create_definition(input).await.unwrap();
    def_id
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_and_get_device() {
    let (_container, device_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org-456").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org-456").await;

    let input = CreateDeviceRepoInput {
        device_id: "test-device-123".to_string(),
        organization_id: "test-org-456".to_string(),
        definition_id: definition_id.clone(),
        workspace_id: workspace_id.clone(),
        name: "Test Device".to_string(),
    };

    // Create device
    let created = device_repo.create_device(input.clone()).await.unwrap();
    assert_eq!(created.device_id, "test-device-123");
    assert_eq!(created.name, "Test Device");
    assert_eq!(created.definition_id, definition_id);
    assert_eq!(created.workspace_id, workspace_id);
    assert!(created.created_at.is_some());

    // Get device
    let get_input = GetDeviceRepoInput {
        device_id: "test-device-123".to_string(),
        organization_id: "test-org-456".to_string(),
    };
    let retrieved = device_repo.get_device(get_input).await.unwrap();
    assert!(retrieved.is_some());

    let device = retrieved.unwrap();
    assert_eq!(device.device_id, "test-device-123");
    assert_eq!(device.name, "Test Device");
    assert_eq!(device.definition_id, definition_id);
    assert_eq!(device.workspace_id, workspace_id);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_device_with_definition() {
    let (_container, device_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org-789").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org-789").await;

    let input = CreateDeviceRepoInput {
        device_id: "test-device-with-def".to_string(),
        organization_id: "test-org-789".to_string(),
        definition_id: definition_id.clone(),
        workspace_id,
        name: "Test Device With Def".to_string(),
    };

    device_repo.create_device(input).await.unwrap();

    // Get device with definition (joined query)
    let get_input = GetDeviceWithDefinitionRepoInput {
        device_id: "test-device-with-def".to_string(),
        organization_id: "test-org-789".to_string(),
    };
    let result = device_repo
        .get_device_with_definition(get_input)
        .await
        .unwrap();
    assert!(result.is_some());

    let device_with_def = result.unwrap();
    assert_eq!(device_with_def.device_id, "test-device-with-def");
    assert_eq!(device_with_def.name, "Test Device With Def");
    assert_eq!(device_with_def.definition_id, definition_id);
    assert_eq!(device_with_def.definition_name, "Test Definition");
    assert_eq!(
        device_with_def.payload_conversion,
        "cayenne_lpp_decode(input)"
    );
    assert_eq!(device_with_def.json_schema, "{}");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_nonexistent_device() {
    let (_container, device_repo, _definition_repo, _client) = setup_test_db().await;

    let get_input = GetDeviceRepoInput {
        device_id: "nonexistent-device".to_string(),
        organization_id: "some-org".to_string(),
    };
    let result = device_repo.get_device(get_input).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_devices_by_workspace() {
    let (_container, device_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org").await;

    // Create multiple devices in the same workspace
    for i in 1..=3 {
        let input = CreateDeviceRepoInput {
            device_id: format!("device-{}", i),
            organization_id: "test-org".to_string(),
            definition_id: definition_id.clone(),
            workspace_id: workspace_id.clone(),
            name: format!("Device {}", i),
        };
        device_repo.create_device(input).await.unwrap();
    }

    // List devices by workspace
    let list_input = ListDevicesRepoInput {
        organization_id: "test-org".to_string(),
        workspace_id: workspace_id.clone(),
    };
    let devices = device_repo.list_devices(list_input).await.unwrap();

    assert_eq!(devices.len(), 3);
    assert!(devices.iter().all(|d| d.organization_id == "test-org"));
    assert!(devices.iter().all(|d| d.workspace_id == workspace_id));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_devices_for_empty_workspace() {
    let (_container, device_repo, _definition_repo, _client) = setup_test_db().await;

    let list_input = ListDevicesRepoInput {
        organization_id: "empty-org".to_string(),
        workspace_id: "empty-workspace".to_string(),
    };
    let devices = device_repo.list_devices(list_input).await.unwrap();

    assert_eq!(devices.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_duplicate_device() {
    let (_container, device_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org").await;

    let input = CreateDeviceRepoInput {
        device_id: "duplicate-device".to_string(),
        organization_id: "test-org".to_string(),
        definition_id: definition_id.clone(),
        workspace_id,
        name: "Original Device".to_string(),
    };

    // First creation should succeed
    device_repo.create_device(input.clone()).await.unwrap();

    // Second creation should fail with DeviceAlreadyExists
    let result = device_repo.create_device(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DeviceAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_device_with_wrong_organization_returns_none() {
    let (_container, device_repo, definition_repo, client) = setup_test_db().await;

    // Create two organizations and definitions
    let conn = client.get_connection().await.unwrap();
    conn.execute(
        "INSERT INTO organizations (id, name) VALUES ($1, $2)",
        &[&"org-1", &"Organization 1"],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO organizations (id, name) VALUES ($1, $2)",
        &[&"org-2", &"Organization 2"],
    )
    .await
    .unwrap();

    // Create workspace for org-1
    let workspace_id = format!("ws-org1-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    conn.execute(
        "INSERT INTO workspaces (id, name, organization_id) VALUES ($1, $2, $3)",
        &[&workspace_id, &"Workspace for Org 1", &"org-1"],
    )
    .await
    .unwrap();

    // Create definition for org-1
    let def_id = format!("def-org1-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let def_input = CreateEndDeviceDefinitionRepoInput {
        id: def_id.clone(),
        organization_id: "org-1".to_string(),
        name: "Definition for Org 1".to_string(),
        json_schema: "{}".to_string(),
        payload_conversion: "test".to_string(),
    };
    definition_repo.create_definition(def_input).await.unwrap();

    // Create device in org-1
    let input = CreateDeviceRepoInput {
        device_id: "device-in-org-1".to_string(),
        organization_id: "org-1".to_string(),
        definition_id: def_id,
        workspace_id,
        name: "Device in Org 1".to_string(),
    };
    device_repo.create_device(input).await.unwrap();

    // Try to get device with correct organization - should succeed
    let correct_input = GetDeviceRepoInput {
        device_id: "device-in-org-1".to_string(),
        organization_id: "org-1".to_string(),
    };
    let result = device_repo.get_device(correct_input).await.unwrap();
    assert!(result.is_some());

    // Try to get device with wrong organization - should return None
    let wrong_input = GetDeviceRepoInput {
        device_id: "device-in-org-1".to_string(),
        organization_id: "org-2".to_string(),
    };
    let result = device_repo.get_device(wrong_input).await.unwrap();
    assert!(
        result.is_none(),
        "Device from org-1 should not be visible to org-2"
    );
}

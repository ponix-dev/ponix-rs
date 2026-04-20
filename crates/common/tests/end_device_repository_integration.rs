#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateEndDeviceDefinitionRepoInput, CreateEndDeviceRepoInput, DomainError,
    EndDeviceDefinitionRepository, EndDeviceRepository, GetEndDeviceRepoInput,
    GetEndDeviceWithDefinitionRepoInput, ListEndDevicesRepoInput,
};
use common::postgres::{
    PostgresClient, PostgresEndDeviceDefinitionRepository, PostgresEndDeviceRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

async fn setup_test_db() -> (
    ContainerAsync<GenericImage>,
    PostgresEndDeviceRepository,
    PostgresEndDeviceDefinitionRepository,
    PostgresClient,
) {
    let postgres = GenericImage::new("postgres", "17")
        .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_exposed_port(5432.into())
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .start()
        .await
        .unwrap();
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

    let end_device_repo = PostgresEndDeviceRepository::new(client.clone());
    let definition_repo = PostgresEndDeviceDefinitionRepository::new(client.clone());

    (postgres, end_device_repo, definition_repo, client)
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

async fn create_test_definition(
    definition_repo: &PostgresEndDeviceDefinitionRepository,
    client: &PostgresClient,
    org_id: &str,
) -> String {
    // Create organization first
    create_test_organization(client, org_id).await;

    // Create definition with unique ID based on org and timestamp
    let def_id = format!(
        "def-{}-{}",
        org_id,
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let input = CreateEndDeviceDefinitionRepoInput {
        id: def_id.clone(),
        organization_id: org_id.to_string(),
        name: "Test Definition".to_string(),
        contracts: vec![common::domain::PayloadContract {
            match_expression: "true".to_string(),
            transform_expression: "cayenne_lpp_decode(input)".to_string(),
            json_schema: "{}".to_string(),
            compiled_match: vec![],
            compiled_transform: vec![],
        }],
    };
    definition_repo.create_definition(input).await.unwrap();
    def_id
}

async fn create_test_gateway(client: &PostgresClient, org_id: &str) -> String {
    // Ensure organization exists first
    create_test_organization(client, org_id).await;

    // Create gateway with unique ID
    let gateway_id = format!(
        "gw-{}-{}",
        org_id,
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let conn = client.get_connection().await.unwrap();
    conn.execute(
        "INSERT INTO gateways (gateway_id, organization_id, name, gateway_type, gateway_config) VALUES ($1, $2, $3, $4, $5)",
        &[&gateway_id, &org_id, &"Test Gateway", &"emqx", &serde_json::json!({"broker_url": "mqtt://localhost:1883"})],
    )
    .await
    .unwrap();
    gateway_id
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_and_get_end_device() {
    let (_container, end_device_repo, definition_repo, client) = setup_test_db().await;

    let definition_id = create_test_definition(&definition_repo, &client, "test-org-456").await;
    let gateway_id = create_test_gateway(&client, "test-org-456").await;

    let input = CreateEndDeviceRepoInput {
        end_device_id: "test-end-device-123".to_string(),
        organization_id: "test-org-456".to_string(),
        definition_id: definition_id.clone(),
        gateway_id: gateway_id.clone(),
        name: "Test End Device".to_string(),
    };

    // Create end device
    let created = end_device_repo
        .create_end_device(input.clone())
        .await
        .unwrap();
    assert_eq!(created.end_device_id, "test-end-device-123");
    assert_eq!(created.name, "Test End Device");
    assert_eq!(created.definition_id, definition_id);
    assert_eq!(created.gateway_id, gateway_id);
    assert!(created.created_at.is_some());

    // Get end device
    let get_input = GetEndDeviceRepoInput {
        end_device_id: "test-end-device-123".to_string(),
        organization_id: "test-org-456".to_string(),
    };
    let retrieved = end_device_repo.get_end_device(get_input).await.unwrap();
    assert!(retrieved.is_some());

    let end_device = retrieved.unwrap();
    assert_eq!(end_device.end_device_id, "test-end-device-123");
    assert_eq!(end_device.name, "Test End Device");
    assert_eq!(end_device.definition_id, definition_id);
    assert_eq!(end_device.gateway_id, gateway_id);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_end_device_with_definition() {
    let (_container, end_device_repo, definition_repo, client) = setup_test_db().await;

    let definition_id = create_test_definition(&definition_repo, &client, "test-org-789").await;
    let gateway_id = create_test_gateway(&client, "test-org-789").await;

    let input = CreateEndDeviceRepoInput {
        end_device_id: "test-end-device-with-def".to_string(),
        organization_id: "test-org-789".to_string(),
        definition_id: definition_id.clone(),
        gateway_id: gateway_id.clone(),
        name: "Test End Device With Def".to_string(),
    };

    end_device_repo.create_end_device(input).await.unwrap();

    // Get end device with definition (joined query)
    let get_input = GetEndDeviceWithDefinitionRepoInput {
        end_device_id: "test-end-device-with-def".to_string(),
        organization_id: "test-org-789".to_string(),
    };
    let result = end_device_repo
        .get_end_device_with_definition(get_input)
        .await
        .unwrap();
    assert!(result.is_some());

    let end_device_with_def = result.unwrap();
    assert_eq!(
        end_device_with_def.end_device_id,
        "test-end-device-with-def"
    );
    assert_eq!(end_device_with_def.name, "Test End Device With Def");
    assert_eq!(end_device_with_def.definition_id, definition_id);
    assert_eq!(end_device_with_def.gateway_id, gateway_id);
    assert_eq!(end_device_with_def.definition_name, "Test Definition");
    assert_eq!(end_device_with_def.contracts.len(), 1);
    assert_eq!(
        end_device_with_def.contracts[0].transform_expression,
        "cayenne_lpp_decode(input)"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_nonexistent_end_device() {
    let (_container, end_device_repo, _definition_repo, _client) = setup_test_db().await;

    let get_input = GetEndDeviceRepoInput {
        end_device_id: "nonexistent-end-device".to_string(),
        organization_id: "some-org".to_string(),
    };
    let result = end_device_repo.get_end_device(get_input).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_end_devices_by_organization() {
    let (_container, end_device_repo, definition_repo, client) = setup_test_db().await;

    let definition_id = create_test_definition(&definition_repo, &client, "test-org").await;
    let gateway_id = create_test_gateway(&client, "test-org").await;

    // Create multiple end devices in the same organization
    for i in 1..=3 {
        let input = CreateEndDeviceRepoInput {
            end_device_id: format!("end-device-{}", i),
            organization_id: "test-org".to_string(),
            definition_id: definition_id.clone(),
            gateway_id: gateway_id.clone(),
            name: format!("End Device {}", i),
        };
        end_device_repo.create_end_device(input).await.unwrap();
    }

    // List end devices by organization
    let list_input = ListEndDevicesRepoInput {
        organization_id: "test-org".to_string(),
    };
    let end_devices = end_device_repo.list_end_devices(list_input).await.unwrap();

    assert_eq!(end_devices.len(), 3);
    assert!(end_devices.iter().all(|d| d.organization_id == "test-org"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_end_devices_for_empty_organization() {
    let (_container, end_device_repo, _definition_repo, _client) = setup_test_db().await;

    let list_input = ListEndDevicesRepoInput {
        organization_id: "empty-org".to_string(),
    };
    let end_devices = end_device_repo.list_end_devices(list_input).await.unwrap();

    assert_eq!(end_devices.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_duplicate_end_device() {
    let (_container, end_device_repo, definition_repo, client) = setup_test_db().await;

    let definition_id = create_test_definition(&definition_repo, &client, "test-org").await;
    let gateway_id = create_test_gateway(&client, "test-org").await;

    let input = CreateEndDeviceRepoInput {
        end_device_id: "duplicate-end-device".to_string(),
        organization_id: "test-org".to_string(),
        definition_id: definition_id.clone(),
        gateway_id,
        name: "Original End Device".to_string(),
    };

    // First creation should succeed
    end_device_repo
        .create_end_device(input.clone())
        .await
        .unwrap();

    // Second creation should fail with EndDeviceAlreadyExists
    let result = end_device_repo.create_end_device(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::EndDeviceAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_end_device_with_wrong_organization_returns_none() {
    let (_container, end_device_repo, definition_repo, client) = setup_test_db().await;

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

    // Create definition for org-1
    let def_id = format!(
        "def-org1-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let def_input = CreateEndDeviceDefinitionRepoInput {
        id: def_id.clone(),
        organization_id: "org-1".to_string(),
        name: "Definition for Org 1".to_string(),
        contracts: vec![common::domain::PayloadContract {
            match_expression: "true".to_string(),
            transform_expression: "test".to_string(),
            json_schema: "{}".to_string(),
            compiled_match: vec![],
            compiled_transform: vec![],
        }],
    };
    definition_repo.create_definition(def_input).await.unwrap();

    // Create gateway for org-1
    let gateway_id = format!(
        "gw-org1-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    conn.execute(
        "INSERT INTO gateways (gateway_id, organization_id, name, gateway_type, gateway_config) VALUES ($1, $2, $3, $4, $5)",
        &[&gateway_id, &"org-1", &"Gateway for Org 1", &"emqx", &serde_json::json!({"broker_url": "mqtt://localhost:1883"})],
    )
    .await
    .unwrap();

    // Create end device in org-1
    let input = CreateEndDeviceRepoInput {
        end_device_id: "end-device-in-org-1".to_string(),
        organization_id: "org-1".to_string(),
        definition_id: def_id,
        gateway_id,
        name: "End Device in Org 1".to_string(),
    };
    end_device_repo.create_end_device(input).await.unwrap();

    // Try to get end device with correct organization - should succeed
    let correct_input = GetEndDeviceRepoInput {
        end_device_id: "end-device-in-org-1".to_string(),
        organization_id: "org-1".to_string(),
    };
    let result = end_device_repo.get_end_device(correct_input).await.unwrap();
    assert!(result.is_some());

    // Try to get end device with wrong organization - should return None
    let wrong_input = GetEndDeviceRepoInput {
        end_device_id: "end-device-in-org-1".to_string(),
        organization_id: "org-2".to_string(),
    };
    let result = end_device_repo.get_end_device(wrong_input).await.unwrap();
    assert!(
        result.is_none(),
        "End device from org-1 should not be visible to org-2"
    );
}

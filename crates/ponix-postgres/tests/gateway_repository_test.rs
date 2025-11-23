#![cfg(feature = "integration-tests")]

use ponix_domain::{
    gateway::{CreateGatewayInputWithId, UpdateGatewayInput},
    organization::CreateOrganizationInputWithId,
    repository::{GatewayRepository, OrganizationRepository},
};
use ponix_postgres::{
    MigrationRunner, PostgresClient, PostgresGatewayRepository, PostgresOrganizationRepository,
};
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

async fn setup_test_db() -> (
    ContainerAsync<Postgres>,
    PostgresGatewayRepository,
    PostgresOrganizationRepository,
    PostgresClient,
) {
    let postgres = Postgres::default().start().await.unwrap();
    let host = postgres.get_host().await.unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();

    // Run migrations
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
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

    let gateway_repo = PostgresGatewayRepository::new(client.clone());
    let org_repo = PostgresOrganizationRepository::new(client.clone());

    (postgres, gateway_repo, org_repo, client)
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_crud_operations() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    // Create organization first
    let org_input = CreateOrganizationInputWithId {
        id: "org-test-001".to_string(),
        name: "Test Organization".to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    // Test Create
    let create_input = CreateGatewayInputWithId {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
        gateway_type: "emqx".to_string(),
        gateway_config: serde_json::json!({
            "host": "mqtt.example.com",
            "port": 1883,
            "topic_pattern": "{organization}/{device}"
        }),
    };

    let created = gateway_repo.create_gateway(create_input).await.unwrap();
    assert_eq!(created.gateway_id, "gw-test-001");
    assert_eq!(created.gateway_type, "emqx");
    assert!(created.created_at.is_some());

    // Test Get
    let retrieved = gateway_repo.get_gateway("gw-test-001").await.unwrap();
    assert!(retrieved.is_some());
    let gateway = retrieved.unwrap();
    assert_eq!(gateway.gateway_id, "gw-test-001");

    // Test Update
    let update_input = UpdateGatewayInput {
        gateway_id: "gw-test-001".to_string(),
        gateway_type: None,
        gateway_config: Some(serde_json::json!({
            "host": "mqtt2.example.com",
            "port": 8883
        })),
    };
    let updated = gateway_repo.update_gateway(update_input).await.unwrap();
    assert_eq!(updated.gateway_config["host"], "mqtt2.example.com");

    // Test List
    let gateways = gateway_repo
        .list_gateways("org-test-001")
        .await
        .unwrap();
    assert_eq!(gateways.len(), 1);

    // Test Delete
    gateway_repo.delete_gateway("gw-test-001").await.unwrap();

    // Verify soft delete
    let deleted = gateway_repo.get_gateway("gw-test-001").await.unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_unique_constraint() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    let org_input = CreateOrganizationInputWithId {
        id: "org-test-002".to_string(),
        name: "Test Organization 2".to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    let create_input = CreateGatewayInputWithId {
        gateway_id: "gw-test-002".to_string(),
        organization_id: "org-test-002".to_string(),
        gateway_type: "emqx".to_string(),
        gateway_config: serde_json::json!({}),
    };

    gateway_repo
        .create_gateway(create_input.clone())
        .await
        .unwrap();

    // Try to create with same ID
    let result = gateway_repo.create_gateway(create_input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ponix_domain::DomainError::GatewayAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_excludes_soft_deleted() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    // Create organization
    let org_input = CreateOrganizationInputWithId {
        id: "org-test-003".to_string(),
        name: "Test Organization 3".to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    // Create multiple gateways
    for i in 1..=3 {
        let create_input = CreateGatewayInputWithId {
            gateway_id: format!("gw-test-{}", i),
            organization_id: "org-test-003".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: serde_json::json!({}),
        };
        gateway_repo.create_gateway(create_input).await.unwrap();
    }

    // Soft delete one
    gateway_repo.delete_gateway("gw-test-2").await.unwrap();

    // List should only return 2
    let gateways = gateway_repo
        .list_gateways("org-test-003")
        .await
        .unwrap();
    assert_eq!(gateways.len(), 2);
    assert!(gateways.iter().all(|g| g.gateway_id != "gw-test-2"));
}

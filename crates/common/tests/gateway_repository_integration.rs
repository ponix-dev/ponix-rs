#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateGatewayRepoInput, CreateOrganizationRepoInputWithId, DeleteGatewayRepoInput, DomainError,
    EmqxGatewayConfig, GatewayConfig, GatewayRepository, GetGatewayRepoInput,
    ListGatewaysRepoInput, OrganizationRepository, RegisterUserRepoInputWithId,
    UpdateGatewayRepoInput, UserRepository,
};
use common::postgres::{
    PostgresClient, PostgresGatewayRepository, PostgresOrganizationRepository,
    PostgresUserRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

const TEST_USER_ID: &str = "test-user-001";

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

    // Create a test user first (needed for user_organizations foreign key)
    let user_repo = PostgresUserRepository::new(client.clone());
    let user_input = RegisterUserRepoInputWithId {
        id: TEST_USER_ID.to_string(),
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    user_repo.register_user(user_input).await.unwrap();

    let gateway_repo = PostgresGatewayRepository::new(client.clone());
    let org_repo = PostgresOrganizationRepository::new(client.clone());

    (postgres, gateway_repo, org_repo, client)
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_crud_operations() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    // Create organization first
    let org_input = CreateOrganizationRepoInputWithId {
        id: "org-test-001".to_string(),
        name: "Test Organization".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    // Test Create
    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
        gateway_type: "emqx".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            subscription_group: "test-group".to_string(),
        }),
    };

    let created = gateway_repo.create_gateway(create_input).await.unwrap();
    assert_eq!(created.gateway_id, "gw-test-001");
    assert_eq!(created.gateway_type, "emqx");

    // Verify config
    match &created.gateway_config {
        GatewayConfig::Emqx(emqx) => {
            assert_eq!(emqx.broker_url, "mqtt://mqtt.example.com:1883");
        }
    }
    assert!(created.created_at.is_some());

    // Test Get
    let get_input = GetGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
    };
    let retrieved = gateway_repo.get_gateway(get_input).await.unwrap();
    assert!(retrieved.is_some());
    let gateway = retrieved.unwrap();
    assert_eq!(gateway.gateway_id, "gw-test-001");

    // Test Update
    let update_input = UpdateGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
        gateway_type: None,
        gateway_config: Some(GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt2.example.com:8883".to_string(),
            subscription_group: "test-group".to_string(),
        })),
    };
    let updated = gateway_repo.update_gateway(update_input).await.unwrap();

    // Verify updated config
    match &updated.gateway_config {
        GatewayConfig::Emqx(emqx) => {
            assert_eq!(emqx.broker_url, "mqtt://mqtt2.example.com:8883");
        }
    }

    // Test List
    let list_input = ListGatewaysRepoInput {
        organization_id: "org-test-001".to_string(),
    };
    let gateways = gateway_repo.list_gateways(list_input).await.unwrap();
    assert_eq!(gateways.len(), 1);

    // Test Delete
    let delete_input = DeleteGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
    };
    gateway_repo.delete_gateway(delete_input).await.unwrap();

    // Verify soft delete
    let get_deleted_input = GetGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
    };
    let deleted = gateway_repo.get_gateway(get_deleted_input).await.unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_unique_constraint() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    let org_input = CreateOrganizationRepoInputWithId {
        id: "org-test-002".to_string(),
        name: "Test Organization 2".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-test-002".to_string(),
        organization_id: "org-test-002".to_string(),
        gateway_type: "emqx".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: String::new(),
            subscription_group: "test-group".to_string(),
        }),
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
        DomainError::GatewayAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_excludes_soft_deleted() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    // Create organization
    let org_input = CreateOrganizationRepoInputWithId {
        id: "org-test-003".to_string(),
        name: "Test Organization 3".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    // Create multiple gateways
    for i in 1..=3 {
        let create_input = CreateGatewayRepoInput {
            gateway_id: format!("gw-test-{}", i),
            organization_id: "org-test-003".to_string(),
            gateway_type: "emqx".to_string(),
            gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
                broker_url: format!("mqtt://mqtt{}.example.com:1883", i),
                subscription_group: "test-group".to_string(),
            }),
        };
        gateway_repo.create_gateway(create_input).await.unwrap();
    }

    // Soft delete one
    let delete_input = DeleteGatewayRepoInput {
        gateway_id: "gw-test-2".to_string(),
        organization_id: "org-test-003".to_string(),
    };
    gateway_repo.delete_gateway(delete_input).await.unwrap();

    // List should only return 2
    let list_input = ListGatewaysRepoInput {
        organization_id: "org-test-003".to_string(),
    };
    let gateways = gateway_repo.list_gateways(list_input).await.unwrap();
    assert_eq!(gateways.len(), 2);
    assert!(gateways.iter().all(|g| g.gateway_id != "gw-test-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_gateway_with_wrong_organization_returns_none() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    // Create two organizations
    let org1_input = CreateOrganizationRepoInputWithId {
        id: "org-cross-1".to_string(),
        name: "Organization 1".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org1_input).await.unwrap();

    let org2_input = CreateOrganizationRepoInputWithId {
        id: "org-cross-2".to_string(),
        name: "Organization 2".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org2_input).await.unwrap();

    // Create gateway in org-cross-1
    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-cross-test".to_string(),
        organization_id: "org-cross-1".to_string(),
        gateway_type: "emqx".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            subscription_group: "test-group".to_string(),
        }),
    };
    gateway_repo.create_gateway(create_input).await.unwrap();

    // Get with correct organization - should succeed
    let correct_input = GetGatewayRepoInput {
        gateway_id: "gw-cross-test".to_string(),
        organization_id: "org-cross-1".to_string(),
    };
    let result = gateway_repo.get_gateway(correct_input).await.unwrap();
    assert!(result.is_some());

    // Get with wrong organization - should return None
    let wrong_input = GetGatewayRepoInput {
        gateway_id: "gw-cross-test".to_string(),
        organization_id: "org-cross-2".to_string(),
    };
    let result = gateway_repo.get_gateway(wrong_input).await.unwrap();
    assert!(
        result.is_none(),
        "Gateway from org-cross-1 should not be visible to org-cross-2"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_update_gateway_with_wrong_organization_returns_not_found() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    // Create two organizations
    let org1_input = CreateOrganizationRepoInputWithId {
        id: "org-update-1".to_string(),
        name: "Organization 1".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org1_input).await.unwrap();

    let org2_input = CreateOrganizationRepoInputWithId {
        id: "org-update-2".to_string(),
        name: "Organization 2".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org2_input).await.unwrap();

    // Create gateway in org-update-1
    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-update-test".to_string(),
        organization_id: "org-update-1".to_string(),
        gateway_type: "emqx".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            subscription_group: "test-group".to_string(),
        }),
    };
    gateway_repo.create_gateway(create_input).await.unwrap();

    // Update with wrong organization - should fail
    let wrong_update = UpdateGatewayRepoInput {
        gateway_id: "gw-update-test".to_string(),
        organization_id: "org-update-2".to_string(),
        gateway_type: Some("changed".to_string()),
        gateway_config: None,
    };
    let result = gateway_repo.update_gateway(wrong_update).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::GatewayNotFound(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_delete_gateway_with_wrong_organization_returns_not_found() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    // Create two organizations
    let org1_input = CreateOrganizationRepoInputWithId {
        id: "org-delete-1".to_string(),
        name: "Organization 1".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org1_input).await.unwrap();

    let org2_input = CreateOrganizationRepoInputWithId {
        id: "org-delete-2".to_string(),
        name: "Organization 2".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org2_input).await.unwrap();

    // Create gateway in org-delete-1
    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-delete-test".to_string(),
        organization_id: "org-delete-1".to_string(),
        gateway_type: "emqx".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            subscription_group: "test-group".to_string(),
        }),
    };
    gateway_repo.create_gateway(create_input).await.unwrap();

    // Delete with wrong organization - should fail
    let wrong_delete = DeleteGatewayRepoInput {
        gateway_id: "gw-delete-test".to_string(),
        organization_id: "org-delete-2".to_string(),
    };
    let result = gateway_repo.delete_gateway(wrong_delete).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::GatewayNotFound(_)
    ));

    // Verify gateway still exists with correct organization
    let correct_get = GetGatewayRepoInput {
        gateway_id: "gw-delete-test".to_string(),
        organization_id: "org-delete-1".to_string(),
    };
    let result = gateway_repo.get_gateway(correct_get).await.unwrap();
    assert!(
        result.is_some(),
        "Gateway should still exist after failed cross-org delete"
    );
}

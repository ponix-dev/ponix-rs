#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateGatewayRepoInput, CreateOrganizationRepoInputWithId, DeleteGatewayRepoInput, DomainError,
    GatewayRepository, GetGatewayRepoInput, ListGatewaysRepoInput, MqttCredentials,
    OrganizationRepository, RegisterUserRepoInputWithId, UpdateGatewayRepoInput, UserRepository,
};
use common::postgres::{
    PostgresClient, PostgresGatewayRepository, PostgresOrganizationRepository,
    PostgresUserRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

const TEST_USER_ID: &str = "test-user-001";

async fn setup_test_db() -> (
    ContainerAsync<GenericImage>,
    PostgresGatewayRepository,
    PostgresOrganizationRepository,
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

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .expect("Failed to create client");

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

    let org_input = CreateOrganizationRepoInputWithId {
        id: "org-test-001".to_string(),
        name: "Test Organization".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
        name: "Test Gateway 001".to_string(),
        broker_url: "mqtt://mqtt.example.com:1883".to_string(),
        credentials: None,
    };

    let created = gateway_repo.create_gateway(create_input).await.unwrap();
    assert_eq!(created.gateway_id, "gw-test-001");
    assert_eq!(created.broker_url, "mqtt://mqtt.example.com:1883");
    assert!(created.credentials.is_none());
    assert!(created.created_at.is_some());

    let get_input = GetGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
    };
    let retrieved = gateway_repo.get_gateway(get_input).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().gateway_id, "gw-test-001");

    let update_input = UpdateGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
        name: None,
        broker_url: Some("mqtt://mqtt2.example.com:8883".to_string()),
        credentials: None,
    };
    let updated = gateway_repo.update_gateway(update_input).await.unwrap();
    assert_eq!(updated.broker_url, "mqtt://mqtt2.example.com:8883");

    let list_input = ListGatewaysRepoInput {
        organization_id: "org-test-001".to_string(),
    };
    let gateways = gateway_repo.list_gateways(list_input).await.unwrap();
    assert_eq!(gateways.len(), 1);

    let delete_input = DeleteGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
    };
    gateway_repo.delete_gateway(delete_input).await.unwrap();

    let get_deleted_input = GetGatewayRepoInput {
        gateway_id: "gw-test-001".to_string(),
        organization_id: "org-test-001".to_string(),
    };
    let deleted = gateway_repo.get_gateway(get_deleted_input).await.unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_credentials_round_trip() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    let org_input = CreateOrganizationRepoInputWithId {
        id: "org-creds".to_string(),
        name: "Creds Org".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    let creds = MqttCredentials {
        username: "alice".to_string(),
        password: "s3cret".to_string(),
    };

    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-creds".to_string(),
        organization_id: "org-creds".to_string(),
        name: "With Credentials".to_string(),
        broker_url: "mqtt://broker:1883".to_string(),
        credentials: Some(creds.clone()),
    };
    let created = gateway_repo.create_gateway(create_input).await.unwrap();
    assert_eq!(created.credentials.as_ref().unwrap().username, "alice");
    assert_eq!(created.credentials.as_ref().unwrap().password, "s3cret");

    let get = gateway_repo
        .get_gateway(GetGatewayRepoInput {
            gateway_id: "gw-creds".to_string(),
            organization_id: "org-creds".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(get.credentials, Some(creds));
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
        name: "Test Gateway 002".to_string(),
        broker_url: "mqtt://mqtt.example.com:1883".to_string(),
        credentials: None,
    };

    gateway_repo
        .create_gateway(create_input.clone())
        .await
        .unwrap();

    let result = gateway_repo.create_gateway(create_input).await;
    assert!(matches!(
        result.unwrap_err(),
        DomainError::GatewayAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_excludes_soft_deleted() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    let org_input = CreateOrganizationRepoInputWithId {
        id: "org-test-003".to_string(),
        name: "Test Organization 3".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();

    for i in 1..=3 {
        let create_input = CreateGatewayRepoInput {
            gateway_id: format!("gw-test-{}", i),
            organization_id: "org-test-003".to_string(),
            name: format!("Test Gateway {}", i),
            broker_url: format!("mqtt://mqtt{}.example.com:1883", i),
            credentials: None,
        };
        gateway_repo.create_gateway(create_input).await.unwrap();
    }

    let delete_input = DeleteGatewayRepoInput {
        gateway_id: "gw-test-2".to_string(),
        organization_id: "org-test-003".to_string(),
    };
    gateway_repo.delete_gateway(delete_input).await.unwrap();

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

    org_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: "org-cross-1".to_string(),
            name: "Organization 1".to_string(),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .unwrap();

    org_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: "org-cross-2".to_string(),
            name: "Organization 2".to_string(),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .unwrap();

    let create_input = CreateGatewayRepoInput {
        gateway_id: "gw-cross-test".to_string(),
        organization_id: "org-cross-1".to_string(),
        name: "Cross Org Test Gateway".to_string(),
        broker_url: "mqtt://mqtt.example.com:1883".to_string(),
        credentials: None,
    };
    gateway_repo.create_gateway(create_input).await.unwrap();

    let correct = gateway_repo
        .get_gateway(GetGatewayRepoInput {
            gateway_id: "gw-cross-test".to_string(),
            organization_id: "org-cross-1".to_string(),
        })
        .await
        .unwrap();
    assert!(correct.is_some());

    let wrong = gateway_repo
        .get_gateway(GetGatewayRepoInput {
            gateway_id: "gw-cross-test".to_string(),
            organization_id: "org-cross-2".to_string(),
        })
        .await
        .unwrap();
    assert!(wrong.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_update_gateway_with_wrong_organization_returns_not_found() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    org_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: "org-update-1".to_string(),
            name: "Organization 1".to_string(),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .unwrap();

    org_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: "org-update-2".to_string(),
            name: "Organization 2".to_string(),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .unwrap();

    gateway_repo
        .create_gateway(CreateGatewayRepoInput {
            gateway_id: "gw-update-test".to_string(),
            organization_id: "org-update-1".to_string(),
            name: "Update Test Gateway".to_string(),
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            credentials: None,
        })
        .await
        .unwrap();

    let result = gateway_repo
        .update_gateway(UpdateGatewayRepoInput {
            gateway_id: "gw-update-test".to_string(),
            organization_id: "org-update-2".to_string(),
            name: None,
            broker_url: Some("mqtt://changed:1883".to_string()),
            credentials: None,
        })
        .await;
    assert!(matches!(
        result.unwrap_err(),
        DomainError::GatewayNotFound(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_delete_gateway_with_wrong_organization_returns_not_found() {
    let (_container, gateway_repo, org_repo, _client) = setup_test_db().await;

    org_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: "org-delete-1".to_string(),
            name: "Organization 1".to_string(),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .unwrap();

    org_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: "org-delete-2".to_string(),
            name: "Organization 2".to_string(),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .unwrap();

    gateway_repo
        .create_gateway(CreateGatewayRepoInput {
            gateway_id: "gw-delete-test".to_string(),
            organization_id: "org-delete-1".to_string(),
            name: "Delete Test Gateway".to_string(),
            broker_url: "mqtt://mqtt.example.com:1883".to_string(),
            credentials: None,
        })
        .await
        .unwrap();

    let result = gateway_repo
        .delete_gateway(DeleteGatewayRepoInput {
            gateway_id: "gw-delete-test".to_string(),
            organization_id: "org-delete-2".to_string(),
        })
        .await;
    assert!(matches!(
        result.unwrap_err(),
        DomainError::GatewayNotFound(_)
    ));

    let still_exists = gateway_repo
        .get_gateway(GetGatewayRepoInput {
            gateway_id: "gw-delete-test".to_string(),
            organization_id: "org-delete-1".to_string(),
        })
        .await
        .unwrap();
    assert!(still_exists.is_some());
}

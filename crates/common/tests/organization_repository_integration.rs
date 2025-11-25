#![cfg(feature = "integration-tests")]

use common::{
    CreateOrganizationInputWithId, DeleteOrganizationInput, GetOrganizationInput,
    ListOrganizationsInput, OrganizationRepository, PostgresClient, PostgresOrganizationRepository,
    UpdateOrganizationInput,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

async fn setup_test_db() -> (
    ContainerAsync<Postgres>,
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

    let repository = PostgresOrganizationRepository::new(client.clone());

    (postgres, repository, client)
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_and_get_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let input = CreateOrganizationInputWithId {
        id: "test-org-123".to_string(),
        name: "Test Organization".to_string(),
    };

    // Create organization
    let created = repo.create_organization(input.clone()).await.unwrap();
    assert_eq!(created.id, "test-org-123");
    assert_eq!(created.name, "Test Organization");
    assert!(created.deleted_at.is_none());
    assert!(created.created_at.is_some());

    // Get organization
    let get_input = GetOrganizationInput {
        organization_id: "test-org-123".to_string(),
    };
    let retrieved = repo.get_organization(get_input).await.unwrap();
    assert!(retrieved.is_some());

    let org = retrieved.unwrap();
    assert_eq!(org.id, "test-org-123");
    assert_eq!(org.name, "Test Organization");
    assert!(org.deleted_at.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_nonexistent_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let get_input = GetOrganizationInput {
        organization_id: "nonexistent-org".to_string(),
    };
    let result = repo.get_organization(get_input).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_update_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create organization
    let input = CreateOrganizationInputWithId {
        id: "test-org-456".to_string(),
        name: "Original Name".to_string(),
    };
    repo.create_organization(input).await.unwrap();

    // Update organization
    let update_input = UpdateOrganizationInput {
        organization_id: "test-org-456".to_string(),
        name: "Updated Name".to_string(),
    };
    let updated = repo.update_organization(update_input).await.unwrap();
    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.id, "test-org-456");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_update_nonexistent_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let update_input = UpdateOrganizationInput {
        organization_id: "nonexistent".to_string(),
        name: "New Name".to_string(),
    };
    let result = repo.update_organization(update_input).await;
    assert!(matches!(
        result,
        Err(common::DomainError::OrganizationNotFound(_))
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_delete_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create organization
    let input = CreateOrganizationInputWithId {
        id: "test-org-789".to_string(),
        name: "To Be Deleted".to_string(),
    };
    repo.create_organization(input).await.unwrap();

    // Delete organization
    let delete_input = DeleteOrganizationInput {
        organization_id: "test-org-789".to_string(),
    };
    repo.delete_organization(delete_input).await.unwrap();

    // Verify it's not returned by get (soft deleted)
    let get_input = GetOrganizationInput {
        organization_id: "test-org-789".to_string(),
    };
    let result = repo.get_organization(get_input).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_delete_nonexistent_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let delete_input = DeleteOrganizationInput {
        organization_id: "nonexistent".to_string(),
    };
    let result = repo.delete_organization(delete_input).await;
    assert!(matches!(
        result,
        Err(common::DomainError::OrganizationNotFound(_))
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_organizations() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create multiple organizations
    for i in 1..=3 {
        let input = CreateOrganizationInputWithId {
            id: format!("org-{}", i),
            name: format!("Organization {}", i),
        };
        repo.create_organization(input).await.unwrap();
    }

    // List organizations
    let list_input = ListOrganizationsInput {};
    let organizations = repo.list_organizations(list_input).await.unwrap();

    assert_eq!(organizations.len(), 3);
    assert!(organizations.iter().all(|o| o.deleted_at.is_none()));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_excludes_soft_deleted() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create organizations
    for i in 1..=3 {
        let input = CreateOrganizationInputWithId {
            id: format!("org-{}", i),
            name: format!("Organization {}", i),
        };
        repo.create_organization(input).await.unwrap();
    }

    // Soft delete one
    let delete_input = DeleteOrganizationInput {
        organization_id: "org-2".to_string(),
    };
    repo.delete_organization(delete_input).await.unwrap();

    // List should only return 2
    let list_input = ListOrganizationsInput {};
    let organizations = repo.list_organizations(list_input).await.unwrap();

    assert_eq!(organizations.len(), 2);
    assert!(organizations.iter().all(|o| o.id != "org-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_duplicate_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let input = CreateOrganizationInputWithId {
        id: "duplicate-org".to_string(),
        name: "Original Organization".to_string(),
    };

    // First creation should succeed
    repo.create_organization(input.clone()).await.unwrap();

    // Second creation should fail with OrganizationAlreadyExists
    let result = repo.create_organization(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        common::DomainError::OrganizationAlreadyExists(_)
    ));
}

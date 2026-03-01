#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateOrganizationRepoInputWithId, DeleteOrganizationRepoInput, DomainError,
    GetOrganizationRepoInput, GetUserOrganizationsRepoInput, ListOrganizationsRepoInput,
    OrganizationRepository, RegisterUserRepoInputWithId, UpdateOrganizationRepoInput,
    UserRepository,
};
use common::postgres::{PostgresClient, PostgresOrganizationRepository, PostgresUserRepository};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

const TEST_USER_ID: &str = "test-user-001";

async fn setup_test_db() -> (
    ContainerAsync<GenericImage>,
    PostgresOrganizationRepository,
    PostgresClient,
) {
    let postgres = GenericImage::new("ghcr.io/ponix-dev/ponix-postgres", "latest")
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

    // Create a test user first (needed for user_organizations foreign key)
    let user_repo = PostgresUserRepository::new(client.clone());
    let user_input = RegisterUserRepoInputWithId {
        id: TEST_USER_ID.to_string(),
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    user_repo.register_user(user_input).await.unwrap();

    let repository = PostgresOrganizationRepository::new(client.clone());

    (postgres, repository, client)
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_and_get_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let input = CreateOrganizationRepoInputWithId {
        id: "test-org-123".to_string(),
        name: "Test Organization".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };

    // Create organization
    let created = repo.create_organization(input.clone()).await.unwrap();
    assert_eq!(created.id, "test-org-123");
    assert_eq!(created.name, "Test Organization");
    assert!(created.deleted_at.is_none());
    assert!(created.created_at.is_some());

    // Get organization
    let get_input = GetOrganizationRepoInput {
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

    let get_input = GetOrganizationRepoInput {
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
    let input = CreateOrganizationRepoInputWithId {
        id: "test-org-456".to_string(),
        name: "Original Name".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    repo.create_organization(input).await.unwrap();

    // Update organization
    let update_input = UpdateOrganizationRepoInput {
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

    let update_input = UpdateOrganizationRepoInput {
        organization_id: "nonexistent".to_string(),
        name: "New Name".to_string(),
    };
    let result = repo.update_organization(update_input).await;
    assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_delete_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create organization
    let input = CreateOrganizationRepoInputWithId {
        id: "test-org-789".to_string(),
        name: "To Be Deleted".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };
    repo.create_organization(input).await.unwrap();

    // Delete organization
    let delete_input = DeleteOrganizationRepoInput {
        organization_id: "test-org-789".to_string(),
    };
    repo.delete_organization(delete_input).await.unwrap();

    // Verify it's not returned by get (soft deleted)
    let get_input = GetOrganizationRepoInput {
        organization_id: "test-org-789".to_string(),
    };
    let result = repo.get_organization(get_input).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_delete_nonexistent_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let delete_input = DeleteOrganizationRepoInput {
        organization_id: "nonexistent".to_string(),
    };
    let result = repo.delete_organization(delete_input).await;
    assert!(matches!(result, Err(DomainError::OrganizationNotFound(_))));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_organizations() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create multiple organizations
    for i in 1..=3 {
        let input = CreateOrganizationRepoInputWithId {
            id: format!("org-{}", i),
            name: format!("Organization {}", i),
            user_id: TEST_USER_ID.to_string(),
        };
        repo.create_organization(input).await.unwrap();
    }

    // List organizations
    let list_input = ListOrganizationsRepoInput {};
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
        let input = CreateOrganizationRepoInputWithId {
            id: format!("org-{}", i),
            name: format!("Organization {}", i),
            user_id: TEST_USER_ID.to_string(),
        };
        repo.create_organization(input).await.unwrap();
    }

    // Soft delete one
    let delete_input = DeleteOrganizationRepoInput {
        organization_id: "org-2".to_string(),
    };
    repo.delete_organization(delete_input).await.unwrap();

    // List should only return 2
    let list_input = ListOrganizationsRepoInput {};
    let organizations = repo.list_organizations(list_input).await.unwrap();

    assert_eq!(organizations.len(), 2);
    assert!(organizations.iter().all(|o| o.id != "org-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_duplicate_organization() {
    let (_container, repo, _client) = setup_test_db().await;

    let input = CreateOrganizationRepoInputWithId {
        id: "duplicate-org".to_string(),
        name: "Original Organization".to_string(),
        user_id: TEST_USER_ID.to_string(),
    };

    // First creation should succeed
    repo.create_organization(input.clone()).await.unwrap();

    // Second creation should fail with OrganizationAlreadyExists
    let result = repo.create_organization(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::OrganizationAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_organization_with_nonexistent_user() {
    let (_container, repo, _client) = setup_test_db().await;

    let input = CreateOrganizationRepoInputWithId {
        id: "org-with-bad-user".to_string(),
        name: "Test Org".to_string(),
        user_id: "nonexistent-user".to_string(),
    };

    // Should fail with UserNotFound due to foreign key constraint
    let result = repo.create_organization(input).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), DomainError::UserNotFound(_)));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_organizations_by_user_id() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create multiple organizations for the test user
    for i in 1..=3 {
        let input = CreateOrganizationRepoInputWithId {
            id: format!("user-org-{}", i),
            name: format!("User Organization {}", i),
            user_id: TEST_USER_ID.to_string(),
        };
        repo.create_organization(input).await.unwrap();
    }

    // Get organizations for the user
    let input = GetUserOrganizationsRepoInput {
        user_id: TEST_USER_ID.to_string(),
    };
    let organizations = repo.get_organizations_by_user_id(input).await.unwrap();

    assert_eq!(organizations.len(), 3);
    assert!(organizations.iter().all(|o| o.deleted_at.is_none()));
    // Verify all org IDs start with "user-org-"
    assert!(organizations.iter().all(|o| o.id.starts_with("user-org-")));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_organizations_by_user_id_empty() {
    let (_container, repo, client) = setup_test_db().await;

    // Create a second user with no organizations
    let user_repo = PostgresUserRepository::new(client);
    let user_input = RegisterUserRepoInputWithId {
        id: "user-no-orgs".to_string(),
        email: "noorganizations@example.com".to_string(),
        name: "No Orgs User".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    user_repo.register_user(user_input).await.unwrap();

    // Get organizations for user with no orgs
    let input = GetUserOrganizationsRepoInput {
        user_id: "user-no-orgs".to_string(),
    };
    let organizations = repo.get_organizations_by_user_id(input).await.unwrap();

    assert!(organizations.is_empty());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_organizations_by_user_id_excludes_soft_deleted() {
    let (_container, repo, _client) = setup_test_db().await;

    // Create organizations for the test user
    for i in 1..=3 {
        let input = CreateOrganizationRepoInputWithId {
            id: format!("soft-del-org-{}", i),
            name: format!("Organization {}", i),
            user_id: TEST_USER_ID.to_string(),
        };
        repo.create_organization(input).await.unwrap();
    }

    // Soft delete one organization
    let delete_input = DeleteOrganizationRepoInput {
        organization_id: "soft-del-org-2".to_string(),
    };
    repo.delete_organization(delete_input).await.unwrap();

    // Get organizations for the user - should only return 2
    let input = GetUserOrganizationsRepoInput {
        user_id: TEST_USER_ID.to_string(),
    };
    let organizations = repo.get_organizations_by_user_id(input).await.unwrap();

    assert_eq!(organizations.len(), 2);
    assert!(organizations.iter().all(|o| o.id != "soft-del-org-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_organizations_by_user_id_multiple_users() {
    let (_container, repo, client) = setup_test_db().await;

    // Create a second user
    let user_repo = PostgresUserRepository::new(client);
    let user_input = RegisterUserRepoInputWithId {
        id: "second-user".to_string(),
        email: "second@example.com".to_string(),
        name: "Second User".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    user_repo.register_user(user_input).await.unwrap();

    // Create organizations for the first user
    for i in 1..=2 {
        let input = CreateOrganizationRepoInputWithId {
            id: format!("first-user-org-{}", i),
            name: format!("First User Org {}", i),
            user_id: TEST_USER_ID.to_string(),
        };
        repo.create_organization(input).await.unwrap();
    }

    // Create organizations for the second user
    for i in 1..=3 {
        let input = CreateOrganizationRepoInputWithId {
            id: format!("second-user-org-{}", i),
            name: format!("Second User Org {}", i),
            user_id: "second-user".to_string(),
        };
        repo.create_organization(input).await.unwrap();
    }

    // Get organizations for first user - should only see their 2 orgs
    let input1 = GetUserOrganizationsRepoInput {
        user_id: TEST_USER_ID.to_string(),
    };
    let orgs1 = repo.get_organizations_by_user_id(input1).await.unwrap();
    assert_eq!(orgs1.len(), 2);
    assert!(orgs1.iter().all(|o| o.id.starts_with("first-user-org-")));

    // Get organizations for second user - should only see their 3 orgs
    let input2 = GetUserOrganizationsRepoInput {
        user_id: "second-user".to_string(),
    };
    let orgs2 = repo.get_organizations_by_user_id(input2).await.unwrap();
    assert_eq!(orgs2.len(), 3);
    assert!(orgs2.iter().all(|o| o.id.starts_with("second-user-org-")));
}

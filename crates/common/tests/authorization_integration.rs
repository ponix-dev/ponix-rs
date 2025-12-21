#![cfg(feature = "integration-tests")]

use common::auth::{Action, AuthorizationProvider, CasbinAuthorizationService, OrgRole, Resource};
use common::domain::{RegisterUserRepoInputWithId, UserRepository};
use common::postgres::{
    create_postgres_authorization_adapter, PostgresClient, PostgresUserRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

const TEST_USER_ID: &str = "test-user-001";
const TEST_ORG_ID: &str = "test-org-001";

async fn setup_test_db() -> (
    ContainerAsync<Postgres>,
    CasbinAuthorizationService,
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

    // Create authorization service
    let adapter = create_postgres_authorization_adapter(&client)
        .await
        .expect("Failed to create authorization adapter");
    let auth_service = CasbinAuthorizationService::new(adapter)
        .await
        .expect("Failed to create authorization service");

    (postgres, auth_service, client)
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_admin_can_create_device() {
    let (_container, auth_service, _client) = setup_test_db().await;

    // Assign admin role
    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Check permission
    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_admin_can_read_device() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Read)
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_admin_can_update_device() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Update)
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_admin_can_delete_device() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Delete)
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_member_can_create_device() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Member)
        .await
        .unwrap();

    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_member_can_read_organization() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Member)
        .await
        .unwrap();

    let allowed = auth_service
        .check_permission(
            TEST_USER_ID,
            TEST_ORG_ID,
            Resource::Organization,
            Action::Read,
        )
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_user_without_role_denied() {
    let (_container, auth_service, _client) = setup_test_db().await;

    // No role assigned
    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await
        .unwrap();

    assert!(!allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_require_permission_denied_returns_error() {
    let (_container, auth_service, _client) = setup_test_db().await;

    // No role assigned - require_permission should return error
    let result = auth_service
        .require_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, common::domain::DomainError::PermissionDenied(_)),
        "Expected PermissionDenied error, got {:?}",
        err
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_require_permission_allowed_succeeds() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    let result = auth_service
        .require_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_super_admin_cross_org_access() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service.assign_super_admin(TEST_USER_ID).await.unwrap();

    // Should have access to any org
    let allowed = auth_service
        .check_permission(TEST_USER_ID, "any-org-id", Resource::Device, Action::Create)
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_super_admin_can_access_any_resource() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service.assign_super_admin(TEST_USER_ID).await.unwrap();

    // Check all resources and actions
    for resource in [Resource::Device, Resource::Gateway, Resource::Organization] {
        for action in [Action::Create, Action::Read, Action::Update, Action::Delete] {
            let allowed = auth_service
                .check_permission(TEST_USER_ID, "random-org", resource, action)
                .await
                .unwrap();

            assert!(
                allowed,
                "Super admin should have access to {:?} {:?}",
                action, resource
            );
        }
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_is_super_admin() {
    let (_container, auth_service, _client) = setup_test_db().await;

    // Initially not a super admin
    let is_super = auth_service.is_super_admin(TEST_USER_ID).await.unwrap();
    assert!(!is_super);

    // Assign super admin
    auth_service.assign_super_admin(TEST_USER_ID).await.unwrap();

    // Now should be super admin
    let is_super = auth_service.is_super_admin(TEST_USER_ID).await.unwrap();
    assert!(is_super);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_user_cannot_access_other_org() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Should NOT have access to org2
    let allowed = auth_service
        .check_permission(
            TEST_USER_ID,
            "other-org-id",
            Resource::Device,
            Action::Create,
        )
        .await
        .unwrap();

    assert!(!allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_roles_for_user_in_org() {
    let (_container, auth_service, _client) = setup_test_db().await;

    // Initially no roles
    let roles = auth_service
        .get_roles_for_user_in_org(TEST_USER_ID, TEST_ORG_ID)
        .await
        .unwrap();
    assert!(roles.is_empty());

    // Assign admin role
    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Now should have admin role
    let roles = auth_service
        .get_roles_for_user_in_org(TEST_USER_ID, TEST_ORG_ID)
        .await
        .unwrap();
    assert!(roles.contains(&"admin".to_string()));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_remove_role() {
    let (_container, auth_service, _client) = setup_test_db().await;

    // Assign role
    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Verify access
    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await
        .unwrap();
    assert!(allowed);

    // Remove role
    auth_service
        .remove_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Verify no access
    let allowed = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await
        .unwrap();
    assert!(!allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_multiple_users_in_same_org() {
    let (_container, auth_service, client) = setup_test_db().await;

    // Create second user
    let user_repo = PostgresUserRepository::new(client);
    let user_input = RegisterUserRepoInputWithId {
        id: "user-2".to_string(),
        email: "user2@example.com".to_string(),
        name: "User Two".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    user_repo.register_user(user_input).await.unwrap();

    // Assign admin to user 1
    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Assign member to user 2
    auth_service
        .assign_role("user-2", TEST_ORG_ID, OrgRole::Member)
        .await
        .unwrap();

    // Both should have access to read devices
    let allowed1 = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Read)
        .await
        .unwrap();
    let allowed2 = auth_service
        .check_permission("user-2", TEST_ORG_ID, Resource::Device, Action::Read)
        .await
        .unwrap();

    assert!(allowed1);
    assert!(allowed2);

    // Both should have device create access (member has same perms as admin for now)
    let allowed1 = auth_service
        .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
        .await
        .unwrap();
    let allowed2 = auth_service
        .check_permission("user-2", TEST_ORG_ID, Resource::Device, Action::Create)
        .await
        .unwrap();

    assert!(allowed1);
    assert!(allowed2);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_user_with_roles_in_multiple_orgs() {
    let (_container, auth_service, _client) = setup_test_db().await;

    // Assign user to multiple orgs
    auth_service
        .assign_role(TEST_USER_ID, "org-1", OrgRole::Admin)
        .await
        .unwrap();
    auth_service
        .assign_role(TEST_USER_ID, "org-2", OrgRole::Member)
        .await
        .unwrap();

    // Should have access to org-1
    let allowed = auth_service
        .check_permission(TEST_USER_ID, "org-1", Resource::Device, Action::Create)
        .await
        .unwrap();
    assert!(allowed);

    // Should have access to org-2
    let allowed = auth_service
        .check_permission(TEST_USER_ID, "org-2", Resource::Device, Action::Create)
        .await
        .unwrap();
    assert!(allowed);

    // Should NOT have access to org-3
    let allowed = auth_service
        .check_permission(TEST_USER_ID, "org-3", Resource::Device, Action::Create)
        .await
        .unwrap();
    assert!(!allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_permissions() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Admin should have all gateway permissions
    for action in [Action::Create, Action::Read, Action::Update, Action::Delete] {
        let allowed = auth_service
            .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Gateway, action)
            .await
            .unwrap();

        assert!(allowed, "Admin should have {:?} gateway permission", action);
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_organization_permissions() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
        .await
        .unwrap();

    // Admin should have read, update, delete on organization
    let allowed_read = auth_service
        .check_permission(
            TEST_USER_ID,
            TEST_ORG_ID,
            Resource::Organization,
            Action::Read,
        )
        .await
        .unwrap();
    let allowed_update = auth_service
        .check_permission(
            TEST_USER_ID,
            TEST_ORG_ID,
            Resource::Organization,
            Action::Update,
        )
        .await
        .unwrap();
    let allowed_delete = auth_service
        .check_permission(
            TEST_USER_ID,
            TEST_ORG_ID,
            Resource::Organization,
            Action::Delete,
        )
        .await
        .unwrap();

    assert!(allowed_read);
    assert!(allowed_update);
    assert!(allowed_delete);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_member_cannot_delete_organization() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Member)
        .await
        .unwrap();

    // Member should NOT have delete permission on organization
    let allowed = auth_service
        .check_permission(
            TEST_USER_ID,
            TEST_ORG_ID,
            Resource::Organization,
            Action::Delete,
        )
        .await
        .unwrap();

    assert!(!allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_member_cannot_update_organization() {
    let (_container, auth_service, _client) = setup_test_db().await;

    auth_service
        .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Member)
        .await
        .unwrap();

    // Member should NOT have update permission on organization
    let allowed = auth_service
        .check_permission(
            TEST_USER_ID,
            TEST_ORG_ID,
            Resource::Organization,
            Action::Update,
        )
        .await
        .unwrap();

    assert!(!allowed);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_role_persisted_in_database() {
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

    let client = PostgresClient::new(
        &host.to_string(),
        port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .expect("Failed to create client");

    // Create test user
    let user_repo = PostgresUserRepository::new(client.clone());
    let user_input = RegisterUserRepoInputWithId {
        id: TEST_USER_ID.to_string(),
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    user_repo.register_user(user_input).await.unwrap();

    // Create first authorization service and assign role
    {
        let adapter = create_postgres_authorization_adapter(&client)
            .await
            .expect("Failed to create authorization adapter");
        let auth_service = CasbinAuthorizationService::new(adapter)
            .await
            .expect("Failed to create authorization service");

        auth_service
            .assign_role(TEST_USER_ID, TEST_ORG_ID, OrgRole::Admin)
            .await
            .unwrap();
    }

    // Create second authorization service (simulating service restart)
    {
        let adapter = create_postgres_authorization_adapter(&client)
            .await
            .expect("Failed to create authorization adapter");
        let auth_service2 = CasbinAuthorizationService::new(adapter)
            .await
            .expect("Failed to create second authorization service");

        // Role should still be there
        let allowed = auth_service2
            .check_permission(TEST_USER_ID, TEST_ORG_ID, Resource::Device, Action::Create)
            .await
            .unwrap();

        assert!(allowed, "Role should persist across service restarts");
    }
}

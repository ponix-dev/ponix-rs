#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateDocumentRepoInput, CreateOrganizationRepoInputWithId, DeleteDocumentRepoInput,
    DocumentRepository, DomainError, GetDocumentRepoInput, ListDocumentsRepoInput,
    OrganizationRepository, RegisterUserRepoInputWithId, UpdateDocumentRepoInput, UserRepository,
};
use common::postgres::{
    PostgresClient, PostgresDocumentRepository, PostgresOrganizationRepository,
    PostgresUserRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

const TEST_USER_ID: &str = "test-user-001";

async fn setup_test_db() -> (
    ContainerAsync<GenericImage>,
    PostgresDocumentRepository,
    PostgresOrganizationRepository,
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

    let doc_repo = PostgresDocumentRepository::new(client.clone());
    let org_repo = PostgresOrganizationRepository::new(client.clone());

    (postgres, doc_repo, org_repo)
}

fn make_create_input(id: &str, org_id: &str) -> CreateDocumentRepoInput {
    CreateDocumentRepoInput {
        document_id: id.to_string(),
        organization_id: org_id.to_string(),
        name: "Manual.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        size_bytes: 1024,
        object_store_key: format!("{}/{}/Manual.pdf", org_id, id),
        checksum: "sha256-abc123".to_string(),
        metadata: serde_json::json!({"author": "test"}),
    }
}

async fn create_org(org_repo: &PostgresOrganizationRepository, org_id: &str) {
    let org_input = CreateOrganizationRepoInputWithId {
        id: org_id.to_string(),
        name: format!("Organization {}", org_id),
        user_id: TEST_USER_ID.to_string(),
    };
    org_repo.create_organization(org_input).await.unwrap();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_document_crud_operations() {
    let (_container, doc_repo, org_repo) = setup_test_db().await;

    create_org(&org_repo, "org-crud-001").await;

    // Create
    let create_input = make_create_input("doc-crud-001", "org-crud-001");
    let created = doc_repo.create_document(create_input).await.unwrap();
    assert_eq!(created.document_id, "doc-crud-001");
    assert_eq!(created.organization_id, "org-crud-001");
    assert_eq!(created.name, "Manual.pdf");
    assert_eq!(created.mime_type, "application/pdf");
    assert_eq!(created.size_bytes, 1024);
    assert_eq!(created.metadata, serde_json::json!({"author": "test"}));
    assert!(created.created_at.is_some());
    assert!(created.updated_at.is_some());
    assert!(created.deleted_at.is_none());

    // Get
    let get_input = GetDocumentRepoInput {
        document_id: "doc-crud-001".to_string(),
        organization_id: "org-crud-001".to_string(),
    };
    let retrieved = doc_repo.get_document(get_input).await.unwrap();
    assert!(retrieved.is_some());
    let doc = retrieved.unwrap();
    assert_eq!(doc.document_id, "doc-crud-001");
    assert_eq!(doc.checksum, "sha256-abc123");

    // Update name, file metadata, and custom metadata
    let update_input = UpdateDocumentRepoInput {
        document_id: "doc-crud-001".to_string(),
        organization_id: "org-crud-001".to_string(),
        name: Some("Updated Manual.pdf".to_string()),
        mime_type: Some("application/octet-stream".to_string()),
        size_bytes: Some(2048),
        checksum: Some("sha256-updated".to_string()),
        metadata: Some(serde_json::json!({"author": "test", "version": 2})),
    };
    let updated = doc_repo.update_document(update_input).await.unwrap();
    assert_eq!(updated.name, "Updated Manual.pdf");
    assert_eq!(updated.mime_type, "application/octet-stream");
    assert_eq!(updated.size_bytes, 2048);
    assert_eq!(updated.checksum, "sha256-updated");
    assert_eq!(
        updated.metadata,
        serde_json::json!({"author": "test", "version": 2})
    );

    // List
    let list_input = ListDocumentsRepoInput {
        organization_id: "org-crud-001".to_string(),
    };
    let documents = doc_repo.list_documents(list_input).await.unwrap();
    assert_eq!(documents.len(), 1);

    // Delete (soft)
    let delete_input = DeleteDocumentRepoInput {
        document_id: "doc-crud-001".to_string(),
        organization_id: "org-crud-001".to_string(),
    };
    doc_repo.delete_document(delete_input).await.unwrap();

    // Verify soft delete - get returns None
    let get_deleted = GetDocumentRepoInput {
        document_id: "doc-crud-001".to_string(),
        organization_id: "org-crud-001".to_string(),
    };
    let deleted = doc_repo.get_document(get_deleted).await.unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_document_unique_constraint() {
    let (_container, doc_repo, org_repo) = setup_test_db().await;

    create_org(&org_repo, "org-unique-001").await;

    let create_input = make_create_input("doc-unique-001", "org-unique-001");
    doc_repo
        .create_document(create_input.clone())
        .await
        .unwrap();

    // Same document_id should fail with DocumentAlreadyExists
    let result = doc_repo.create_document(create_input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_document_unique_object_store_key() {
    let (_container, doc_repo, org_repo) = setup_test_db().await;

    create_org(&org_repo, "org-key-001").await;

    let create_input = make_create_input("doc-key-001", "org-key-001");
    doc_repo.create_document(create_input).await.unwrap();

    // Different document_id but same object_store_key should fail
    let mut duplicate_key_input = make_create_input("doc-key-002", "org-key-001");
    duplicate_key_input.object_store_key = "org-key-001/doc-key-001/Manual.pdf".to_string();

    let result = doc_repo.create_document(duplicate_key_input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_document_foreign_key_organization() {
    let (_container, doc_repo, _org_repo) = setup_test_db().await;

    // Create document with non-existent organization
    let create_input = make_create_input("doc-fk-001", "org-nonexistent");
    let result = doc_repo.create_document(create_input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::OrganizationNotFound(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_excludes_soft_deleted() {
    let (_container, doc_repo, org_repo) = setup_test_db().await;

    create_org(&org_repo, "org-list-001").await;

    // Create 3 documents
    for i in 1..=3 {
        let mut input = make_create_input(&format!("doc-list-{}", i), "org-list-001");
        input.name = format!("Document {}.pdf", i);
        input.object_store_key = format!("org-list-001/doc-list-{}/file.pdf", i);
        doc_repo.create_document(input).await.unwrap();
    }

    // Soft delete one
    let delete_input = DeleteDocumentRepoInput {
        document_id: "doc-list-2".to_string(),
        organization_id: "org-list-001".to_string(),
    };
    doc_repo.delete_document(delete_input).await.unwrap();

    // List should only return 2
    let list_input = ListDocumentsRepoInput {
        organization_id: "org-list-001".to_string(),
    };
    let documents = doc_repo.list_documents(list_input).await.unwrap();
    assert_eq!(documents.len(), 2);
    assert!(documents.iter().all(|d| d.document_id != "doc-list-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_document_with_wrong_organization_returns_none() {
    let (_container, doc_repo, org_repo) = setup_test_db().await;

    create_org(&org_repo, "org-cross-1").await;
    create_org(&org_repo, "org-cross-2").await;

    let create_input = make_create_input("doc-cross-001", "org-cross-1");
    doc_repo.create_document(create_input).await.unwrap();

    // Get with correct organization - should succeed
    let correct_input = GetDocumentRepoInput {
        document_id: "doc-cross-001".to_string(),
        organization_id: "org-cross-1".to_string(),
    };
    let result = doc_repo.get_document(correct_input).await.unwrap();
    assert!(result.is_some());

    // Get with wrong organization - should return None
    let wrong_input = GetDocumentRepoInput {
        document_id: "doc-cross-001".to_string(),
        organization_id: "org-cross-2".to_string(),
    };
    let result = doc_repo.get_document(wrong_input).await.unwrap();
    assert!(
        result.is_none(),
        "Document from org-cross-1 should not be visible to org-cross-2"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_update_document_with_wrong_organization_returns_not_found() {
    let (_container, doc_repo, org_repo) = setup_test_db().await;

    create_org(&org_repo, "org-update-1").await;
    create_org(&org_repo, "org-update-2").await;

    let create_input = make_create_input("doc-update-001", "org-update-1");
    doc_repo.create_document(create_input).await.unwrap();

    // Update with wrong organization - should fail
    let wrong_update = UpdateDocumentRepoInput {
        document_id: "doc-update-001".to_string(),
        organization_id: "org-update-2".to_string(),
        name: Some("Hacked Name".to_string()),
        mime_type: None,
        size_bytes: None,
        checksum: None,
        metadata: None,
    };
    let result = doc_repo.update_document(wrong_update).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentNotFound(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_delete_document_with_wrong_organization_returns_not_found() {
    let (_container, doc_repo, org_repo) = setup_test_db().await;

    create_org(&org_repo, "org-delete-1").await;
    create_org(&org_repo, "org-delete-2").await;

    let create_input = make_create_input("doc-delete-001", "org-delete-1");
    doc_repo.create_document(create_input).await.unwrap();

    // Delete with wrong organization - should fail
    let wrong_delete = DeleteDocumentRepoInput {
        document_id: "doc-delete-001".to_string(),
        organization_id: "org-delete-2".to_string(),
    };
    let result = doc_repo.delete_document(wrong_delete).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentNotFound(_)
    ));

    // Verify document still exists with correct organization
    let correct_get = GetDocumentRepoInput {
        document_id: "doc-delete-001".to_string(),
        organization_id: "org-delete-1".to_string(),
    };
    let result = doc_repo.get_document(correct_get).await.unwrap();
    assert!(
        result.is_some(),
        "Document should still exist after failed cross-org delete"
    );
}

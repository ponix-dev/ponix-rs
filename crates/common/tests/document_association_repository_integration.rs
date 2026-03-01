#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateDataStreamDefinitionRepoInput, CreateDocumentRepoInput,
    CreateOrganizationRepoInputWithId, DataStreamDefinitionRepository,
    DocumentAssociationRepository, DocumentRepository, DomainError, LinkDocumentInput,
    ListDocumentsByTargetInput, OrganizationRepository, PayloadContract,
    RegisterUserRepoInputWithId, UnlinkDocumentInput, UserRepository,
};
use common::postgres::{
    PostgresClient, PostgresDataStreamDefinitionRepository, PostgresDocumentAssociationRepository,
    PostgresDocumentRepository, PostgresOrganizationRepository, PostgresUserRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

const TEST_USER_ID: &str = "test-user-001";
const ORG_ID: &str = "org-assoc-001";

struct TestContext {
    _container: ContainerAsync<GenericImage>,
    assoc_repo: PostgresDocumentAssociationRepository,
    doc_repo: PostgresDocumentRepository,
    client: PostgresClient,
}

async fn setup_test_db() -> TestContext {
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
    user_repo
        .register_user(RegisterUserRepoInputWithId {
            id: TEST_USER_ID.to_string(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            password_hash: "hashed_password".to_string(),
        })
        .await
        .unwrap();

    let assoc_repo = PostgresDocumentAssociationRepository::new(client.clone());
    let doc_repo = PostgresDocumentRepository::new(client.clone());

    TestContext {
        _container: postgres,
        assoc_repo,
        doc_repo,
        client,
    }
}

async fn create_org(client: &PostgresClient, org_id: &str) {
    let org_repo = PostgresOrganizationRepository::new(client.clone());
    org_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: org_id.to_string(),
            name: format!("Organization {}", org_id),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .unwrap();
}

async fn create_workspace(client: &PostgresClient, org_id: &str, ws_id: &str) {
    let conn = client.get_connection().await.unwrap();
    conn.execute(
        "INSERT INTO workspaces (id, name, organization_id) VALUES ($1, $2, $3)",
        &[&ws_id, &"Test Workspace", &org_id],
    )
    .await
    .unwrap();
}

async fn create_data_stream(
    client: &PostgresClient,
    org_id: &str,
    ws_id: &str,
    ds_id: &str,
    def_id: &str,
    gw_id: &str,
) {
    let conn = client.get_connection().await.unwrap();
    conn.execute(
        "INSERT INTO gateways (gateway_id, organization_id, name, gateway_type, gateway_config) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        &[&gw_id, &org_id, &"Test Gateway", &"emqx", &serde_json::json!({"broker_url": "mqtt://localhost:1883"})],
    )
    .await
    .unwrap();

    conn.execute(
        "INSERT INTO data_streams (data_stream_id, organization_id, workspace_id, definition_id, gateway_id, name) VALUES ($1, $2, $3, $4, $5, $6)",
        &[&ds_id, &org_id, &ws_id, &def_id, &gw_id, &"Test Data Stream"],
    )
    .await
    .unwrap();
}

async fn create_definition(client: &PostgresClient, org_id: &str, def_id: &str) {
    let def_repo = PostgresDataStreamDefinitionRepository::new(client.clone());
    def_repo
        .create_definition(CreateDataStreamDefinitionRepoInput {
            id: def_id.to_string(),
            organization_id: org_id.to_string(),
            name: "Test Definition".to_string(),
            contracts: vec![PayloadContract {
                match_expression: "true".to_string(),
                transform_expression: "cayenne_lpp_decode(input)".to_string(),
                json_schema: "{}".to_string(),
                compiled_match: vec![],
                compiled_transform: vec![],
            }],
        })
        .await
        .unwrap();
}

async fn create_doc(doc_repo: &PostgresDocumentRepository, doc_id: &str, org_id: &str) {
    doc_repo
        .create_document(CreateDocumentRepoInput {
            document_id: doc_id.to_string(),
            organization_id: org_id.to_string(),
            name: format!("{}.pdf", doc_id),
            mime_type: "application/pdf".to_string(),
            size_bytes: 1024,
            object_store_key: format!("{}/{}/file.pdf", org_id, doc_id),
            checksum: "sha256-abc123".to_string(),
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();
}

// ─── Data Stream association tests ───

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_link_and_list_data_stream_documents() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-001").await;
    create_definition(&ctx.client, ORG_ID, "def-001").await;
    create_data_stream(&ctx.client, ORG_ID, "ws-001", "ds-001", "def-001", "gw-001").await;

    create_doc(&ctx.doc_repo, "doc-ds-1", ORG_ID).await;
    create_doc(&ctx.doc_repo, "doc-ds-2", ORG_ID).await;

    // Link two documents to the data stream
    ctx.assoc_repo
        .link_to_data_stream(LinkDocumentInput {
            document_id: "doc-ds-1".to_string(),
            target_id: "ds-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: Some("ws-001".to_string()),
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .link_to_data_stream(LinkDocumentInput {
            document_id: "doc-ds-2".to_string(),
            target_id: "ds-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: Some("ws-001".to_string()),
        })
        .await
        .unwrap();

    // List should return both
    let docs = ctx
        .assoc_repo
        .list_data_stream_documents(ListDocumentsByTargetInput {
            target_id: "ds-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(docs.len(), 2);
    let ids: Vec<&str> = docs.iter().map(|d| d.document_id.as_str()).collect();
    assert!(ids.contains(&"doc-ds-1"));
    assert!(ids.contains(&"doc-ds-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_unlink_data_stream_document() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-001").await;
    create_definition(&ctx.client, ORG_ID, "def-001").await;
    create_data_stream(&ctx.client, ORG_ID, "ws-001", "ds-001", "def-001", "gw-001").await;

    create_doc(&ctx.doc_repo, "doc-unlink-ds", ORG_ID).await;

    ctx.assoc_repo
        .link_to_data_stream(LinkDocumentInput {
            document_id: "doc-unlink-ds".to_string(),
            target_id: "ds-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: Some("ws-001".to_string()),
        })
        .await
        .unwrap();

    // Unlink
    ctx.assoc_repo
        .unlink_from_data_stream(UnlinkDocumentInput {
            document_id: "doc-unlink-ds".to_string(),
            target_id: "ds-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    // List should now be empty
    let docs = ctx
        .assoc_repo
        .list_data_stream_documents(ListDocumentsByTargetInput {
            target_id: "ds-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(docs.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_duplicate_data_stream_link_fails() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-001").await;
    create_definition(&ctx.client, ORG_ID, "def-001").await;
    create_data_stream(&ctx.client, ORG_ID, "ws-001", "ds-001", "def-001", "gw-001").await;

    create_doc(&ctx.doc_repo, "doc-dup-ds", ORG_ID).await;

    let input = LinkDocumentInput {
        document_id: "doc-dup-ds".to_string(),
        target_id: "ds-001".to_string(),
        organization_id: ORG_ID.to_string(),
        workspace_id: Some("ws-001".to_string()),
    };

    ctx.assoc_repo
        .link_to_data_stream(input.clone())
        .await
        .unwrap();

    // Second link should fail with DocumentAssociationAlreadyExists
    let result = ctx.assoc_repo.link_to_data_stream(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentAssociationAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_unlink_nonexistent_association_fails() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;

    let result = ctx
        .assoc_repo
        .unlink_from_data_stream(UnlinkDocumentInput {
            document_id: "nonexistent-doc".to_string(),
            target_id: "nonexistent-ds".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentNotFound(_)
    ));
}

// ─── Definition association tests ───

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_link_and_list_definition_documents() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_definition(&ctx.client, ORG_ID, "def-link-001").await;

    create_doc(&ctx.doc_repo, "doc-def-1", ORG_ID).await;
    create_doc(&ctx.doc_repo, "doc-def-2", ORG_ID).await;

    ctx.assoc_repo
        .link_to_definition(LinkDocumentInput {
            document_id: "doc-def-1".to_string(),
            target_id: "def-link-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .link_to_definition(LinkDocumentInput {
            document_id: "doc-def-2".to_string(),
            target_id: "def-link-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    let docs = ctx
        .assoc_repo
        .list_definition_documents(ListDocumentsByTargetInput {
            target_id: "def-link-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(docs.len(), 2);
    let ids: Vec<&str> = docs.iter().map(|d| d.document_id.as_str()).collect();
    assert!(ids.contains(&"doc-def-1"));
    assert!(ids.contains(&"doc-def-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_unlink_definition_document() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_definition(&ctx.client, ORG_ID, "def-unlink-001").await;

    create_doc(&ctx.doc_repo, "doc-def-unlink", ORG_ID).await;

    ctx.assoc_repo
        .link_to_definition(LinkDocumentInput {
            document_id: "doc-def-unlink".to_string(),
            target_id: "def-unlink-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .unlink_from_definition(UnlinkDocumentInput {
            document_id: "doc-def-unlink".to_string(),
            target_id: "def-unlink-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    let docs = ctx
        .assoc_repo
        .list_definition_documents(ListDocumentsByTargetInput {
            target_id: "def-unlink-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(docs.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_duplicate_definition_link_fails() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_definition(&ctx.client, ORG_ID, "def-dup-001").await;

    create_doc(&ctx.doc_repo, "doc-def-dup", ORG_ID).await;

    let input = LinkDocumentInput {
        document_id: "doc-def-dup".to_string(),
        target_id: "def-dup-001".to_string(),
        organization_id: ORG_ID.to_string(),
        workspace_id: None,
    };

    ctx.assoc_repo
        .link_to_definition(input.clone())
        .await
        .unwrap();

    let result = ctx.assoc_repo.link_to_definition(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentAssociationAlreadyExists(_)
    ));
}

// ─── Workspace association tests ───

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_link_and_list_workspace_documents() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-link-001").await;

    create_doc(&ctx.doc_repo, "doc-ws-1", ORG_ID).await;
    create_doc(&ctx.doc_repo, "doc-ws-2", ORG_ID).await;

    ctx.assoc_repo
        .link_to_workspace(LinkDocumentInput {
            document_id: "doc-ws-1".to_string(),
            target_id: "ws-link-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .link_to_workspace(LinkDocumentInput {
            document_id: "doc-ws-2".to_string(),
            target_id: "ws-link-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    let docs = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-link-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(docs.len(), 2);
    let ids: Vec<&str> = docs.iter().map(|d| d.document_id.as_str()).collect();
    assert!(ids.contains(&"doc-ws-1"));
    assert!(ids.contains(&"doc-ws-2"));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_unlink_workspace_document() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-unlink-001").await;

    create_doc(&ctx.doc_repo, "doc-ws-unlink", ORG_ID).await;

    ctx.assoc_repo
        .link_to_workspace(LinkDocumentInput {
            document_id: "doc-ws-unlink".to_string(),
            target_id: "ws-unlink-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .unlink_from_workspace(UnlinkDocumentInput {
            document_id: "doc-ws-unlink".to_string(),
            target_id: "ws-unlink-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    let docs = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-unlink-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(docs.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_duplicate_workspace_link_fails() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-dup-001").await;

    create_doc(&ctx.doc_repo, "doc-ws-dup", ORG_ID).await;

    let input = LinkDocumentInput {
        document_id: "doc-ws-dup".to_string(),
        target_id: "ws-dup-001".to_string(),
        organization_id: ORG_ID.to_string(),
        workspace_id: None,
    };

    ctx.assoc_repo
        .link_to_workspace(input.clone())
        .await
        .unwrap();

    let result = ctx.assoc_repo.link_to_workspace(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DocumentAssociationAlreadyExists(_)
    ));
}

// ─── Cross-cutting tests ───

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_soft_deleted_documents_excluded_from_list() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-soft-001").await;

    create_doc(&ctx.doc_repo, "doc-soft-1", ORG_ID).await;
    create_doc(&ctx.doc_repo, "doc-soft-2", ORG_ID).await;

    // Link both to workspace
    for doc_id in &["doc-soft-1", "doc-soft-2"] {
        ctx.assoc_repo
            .link_to_workspace(LinkDocumentInput {
                document_id: doc_id.to_string(),
                target_id: "ws-soft-001".to_string(),
                organization_id: ORG_ID.to_string(),
                workspace_id: None,
            })
            .await
            .unwrap();
    }

    // Soft delete one document
    ctx.doc_repo
        .delete_document(common::domain::DeleteDocumentRepoInput {
            document_id: "doc-soft-1".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    // List should only return the non-deleted document
    let docs = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-soft-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].document_id, "doc-soft-2");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_document_linked_to_multiple_targets() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;
    create_workspace(&ctx.client, ORG_ID, "ws-multi-001").await;
    create_workspace(&ctx.client, ORG_ID, "ws-multi-002").await;
    create_definition(&ctx.client, ORG_ID, "def-multi-001").await;

    create_doc(&ctx.doc_repo, "doc-multi", ORG_ID).await;

    // Link same document to two workspaces and a definition
    ctx.assoc_repo
        .link_to_workspace(LinkDocumentInput {
            document_id: "doc-multi".to_string(),
            target_id: "ws-multi-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .link_to_workspace(LinkDocumentInput {
            document_id: "doc-multi".to_string(),
            target_id: "ws-multi-002".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .link_to_definition(LinkDocumentInput {
            document_id: "doc-multi".to_string(),
            target_id: "def-multi-001".to_string(),
            organization_id: ORG_ID.to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    // Each target should list the document
    let ws1_docs = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-multi-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(ws1_docs.len(), 1);
    assert_eq!(ws1_docs[0].document_id, "doc-multi");

    let ws2_docs = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-multi-002".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(ws2_docs.len(), 1);
    assert_eq!(ws2_docs[0].document_id, "doc-multi");

    let def_docs = ctx
        .assoc_repo
        .list_definition_documents(ListDocumentsByTargetInput {
            target_id: "def-multi-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(def_docs.len(), 1);
    assert_eq!(def_docs[0].document_id, "doc-multi");

    // Unlinking from one workspace doesn't affect the other or the definition
    ctx.assoc_repo
        .unlink_from_workspace(UnlinkDocumentInput {
            document_id: "doc-multi".to_string(),
            target_id: "ws-multi-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();

    let ws1_after = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-multi-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(ws1_after.len(), 0);

    let ws2_after = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-multi-002".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(ws2_after.len(), 1);

    let def_after = ctx
        .assoc_repo
        .list_definition_documents(ListDocumentsByTargetInput {
            target_id: "def-multi-001".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(def_after.len(), 1);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_cross_organization_isolation() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, "org-iso-1").await;
    create_org(&ctx.client, "org-iso-2").await;
    create_workspace(&ctx.client, "org-iso-1", "ws-iso-1").await;
    create_workspace(&ctx.client, "org-iso-2", "ws-iso-2").await;

    create_doc(&ctx.doc_repo, "doc-iso-1", "org-iso-1").await;
    create_doc(&ctx.doc_repo, "doc-iso-2", "org-iso-2").await;

    // Link docs to their org's workspace
    ctx.assoc_repo
        .link_to_workspace(LinkDocumentInput {
            document_id: "doc-iso-1".to_string(),
            target_id: "ws-iso-1".to_string(),
            organization_id: "org-iso-1".to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    ctx.assoc_repo
        .link_to_workspace(LinkDocumentInput {
            document_id: "doc-iso-2".to_string(),
            target_id: "ws-iso-2".to_string(),
            organization_id: "org-iso-2".to_string(),
            workspace_id: None,
        })
        .await
        .unwrap();

    // Org 1 should only see its documents
    let org1_docs = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-iso-1".to_string(),
            organization_id: "org-iso-1".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(org1_docs.len(), 1);
    assert_eq!(org1_docs[0].document_id, "doc-iso-1");

    // Org 2 querying org 1's workspace should see nothing
    let cross_org = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "ws-iso-1".to_string(),
            organization_id: "org-iso-2".to_string(),
        })
        .await
        .unwrap();
    assert_eq!(
        cross_org.len(),
        0,
        "Documents from org-iso-1 should not be visible to org-iso-2"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_empty_target_returns_empty_list() {
    let ctx = setup_test_db().await;
    create_org(&ctx.client, ORG_ID).await;

    // List documents for a target that has no links
    let ds_docs = ctx
        .assoc_repo
        .list_data_stream_documents(ListDocumentsByTargetInput {
            target_id: "nonexistent-ds".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(ds_docs.len(), 0);

    let def_docs = ctx
        .assoc_repo
        .list_definition_documents(ListDocumentsByTargetInput {
            target_id: "nonexistent-def".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(def_docs.len(), 0);

    let ws_docs = ctx
        .assoc_repo
        .list_workspace_documents(ListDocumentsByTargetInput {
            target_id: "nonexistent-ws".to_string(),
            organization_id: ORG_ID.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(ws_docs.len(), 0);
}

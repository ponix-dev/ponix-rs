#![cfg(feature = "integration-tests")]

use common::domain::{
    CreateDataStreamDefinitionRepoInput, CreateDataStreamRepoInput, DataStreamDefinitionRepository,
    DataStreamRepository, DomainError, GetDataStreamRepoInput,
    GetDataStreamWithDefinitionRepoInput, ListDataStreamsRepoInput,
};
use common::postgres::{
    PostgresClient, PostgresDataStreamDefinitionRepository, PostgresDataStreamRepository,
};
use goose::MigrationRunner;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

async fn setup_test_db() -> (
    ContainerAsync<Postgres>,
    PostgresDataStreamRepository,
    PostgresDataStreamDefinitionRepository,
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

    let data_stream_repo = PostgresDataStreamRepository::new(client.clone());
    let definition_repo = PostgresDataStreamDefinitionRepository::new(client.clone());

    (postgres, data_stream_repo, definition_repo, client)
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
    let workspace_id = format!(
        "ws-{}-{}",
        org_id,
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
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
    definition_repo: &PostgresDataStreamDefinitionRepository,
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
    let input = CreateDataStreamDefinitionRepoInput {
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
async fn test_create_and_get_data_stream() {
    let (_container, data_stream_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org-456").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org-456").await;
    let gateway_id = create_test_gateway(&client, "test-org-456").await;

    let input = CreateDataStreamRepoInput {
        data_stream_id: "test-data-stream-123".to_string(),
        organization_id: "test-org-456".to_string(),
        definition_id: definition_id.clone(),
        workspace_id: workspace_id.clone(),
        gateway_id: gateway_id.clone(),
        name: "Test Data Stream".to_string(),
    };

    // Create data stream
    let created = data_stream_repo
        .create_data_stream(input.clone())
        .await
        .unwrap();
    assert_eq!(created.data_stream_id, "test-data-stream-123");
    assert_eq!(created.name, "Test Data Stream");
    assert_eq!(created.definition_id, definition_id);
    assert_eq!(created.workspace_id, workspace_id);
    assert_eq!(created.gateway_id, gateway_id);
    assert!(created.created_at.is_some());

    // Get data stream
    let get_input = GetDataStreamRepoInput {
        data_stream_id: "test-data-stream-123".to_string(),
        organization_id: "test-org-456".to_string(),
        workspace_id: workspace_id.clone(),
    };
    let retrieved = data_stream_repo.get_data_stream(get_input).await.unwrap();
    assert!(retrieved.is_some());

    let data_stream = retrieved.unwrap();
    assert_eq!(data_stream.data_stream_id, "test-data-stream-123");
    assert_eq!(data_stream.name, "Test Data Stream");
    assert_eq!(data_stream.definition_id, definition_id);
    assert_eq!(data_stream.workspace_id, workspace_id);
    assert_eq!(data_stream.gateway_id, gateway_id);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_data_stream_with_definition() {
    let (_container, data_stream_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org-789").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org-789").await;
    let gateway_id = create_test_gateway(&client, "test-org-789").await;

    let input = CreateDataStreamRepoInput {
        data_stream_id: "test-data-stream-with-def".to_string(),
        organization_id: "test-org-789".to_string(),
        definition_id: definition_id.clone(),
        workspace_id,
        gateway_id: gateway_id.clone(),
        name: "Test Data Stream With Def".to_string(),
    };

    data_stream_repo.create_data_stream(input).await.unwrap();

    // Get data stream with definition (joined query)
    let get_input = GetDataStreamWithDefinitionRepoInput {
        data_stream_id: "test-data-stream-with-def".to_string(),
        organization_id: "test-org-789".to_string(),
    };
    let result = data_stream_repo
        .get_data_stream_with_definition(get_input)
        .await
        .unwrap();
    assert!(result.is_some());

    let data_stream_with_def = result.unwrap();
    assert_eq!(
        data_stream_with_def.data_stream_id,
        "test-data-stream-with-def"
    );
    assert_eq!(data_stream_with_def.name, "Test Data Stream With Def");
    assert_eq!(data_stream_with_def.definition_id, definition_id);
    assert_eq!(data_stream_with_def.gateway_id, gateway_id);
    assert_eq!(data_stream_with_def.definition_name, "Test Definition");
    assert_eq!(data_stream_with_def.contracts.len(), 1);
    assert_eq!(
        data_stream_with_def.contracts[0].transform_expression,
        "cayenne_lpp_decode(input)"
    );
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_nonexistent_data_stream() {
    let (_container, data_stream_repo, _definition_repo, _client) = setup_test_db().await;

    let get_input = GetDataStreamRepoInput {
        data_stream_id: "nonexistent-data-stream".to_string(),
        organization_id: "some-org".to_string(),
        workspace_id: "some-workspace".to_string(),
    };
    let result = data_stream_repo.get_data_stream(get_input).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_data_streams_by_workspace() {
    let (_container, data_stream_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org").await;
    let gateway_id = create_test_gateway(&client, "test-org").await;

    // Create multiple data streams in the same workspace
    for i in 1..=3 {
        let input = CreateDataStreamRepoInput {
            data_stream_id: format!("data-stream-{}", i),
            organization_id: "test-org".to_string(),
            definition_id: definition_id.clone(),
            workspace_id: workspace_id.clone(),
            gateway_id: gateway_id.clone(),
            name: format!("Data Stream {}", i),
        };
        data_stream_repo.create_data_stream(input).await.unwrap();
    }

    // List data streams by workspace
    let list_input = ListDataStreamsRepoInput {
        organization_id: "test-org".to_string(),
        workspace_id: workspace_id.clone(),
    };
    let data_streams = data_stream_repo
        .list_data_streams(list_input)
        .await
        .unwrap();

    assert_eq!(data_streams.len(), 3);
    assert!(data_streams.iter().all(|d| d.organization_id == "test-org"));
    assert!(data_streams.iter().all(|d| d.workspace_id == workspace_id));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_list_data_streams_for_empty_workspace() {
    let (_container, data_stream_repo, _definition_repo, _client) = setup_test_db().await;

    let list_input = ListDataStreamsRepoInput {
        organization_id: "empty-org".to_string(),
        workspace_id: "empty-workspace".to_string(),
    };
    let data_streams = data_stream_repo
        .list_data_streams(list_input)
        .await
        .unwrap();

    assert_eq!(data_streams.len(), 0);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_create_duplicate_data_stream() {
    let (_container, data_stream_repo, definition_repo, client) = setup_test_db().await;

    let workspace_id = create_test_workspace(&client, "test-org").await;
    let definition_id = create_test_definition(&definition_repo, &client, "test-org").await;
    let gateway_id = create_test_gateway(&client, "test-org").await;

    let input = CreateDataStreamRepoInput {
        data_stream_id: "duplicate-data-stream".to_string(),
        organization_id: "test-org".to_string(),
        definition_id: definition_id.clone(),
        workspace_id,
        gateway_id,
        name: "Original Data Stream".to_string(),
    };

    // First creation should succeed
    data_stream_repo
        .create_data_stream(input.clone())
        .await
        .unwrap();

    // Second creation should fail with DataStreamAlreadyExists
    let result = data_stream_repo.create_data_stream(input).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DomainError::DataStreamAlreadyExists(_)
    ));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_get_data_stream_with_wrong_organization_returns_none() {
    let (_container, data_stream_repo, definition_repo, client) = setup_test_db().await;

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
    let workspace_id = format!(
        "ws-org1-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    conn.execute(
        "INSERT INTO workspaces (id, name, organization_id) VALUES ($1, $2, $3)",
        &[&workspace_id, &"Workspace for Org 1", &"org-1"],
    )
    .await
    .unwrap();

    // Create definition for org-1
    let def_id = format!(
        "def-org1-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let def_input = CreateDataStreamDefinitionRepoInput {
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

    // Create data stream in org-1
    let input = CreateDataStreamRepoInput {
        data_stream_id: "data-stream-in-org-1".to_string(),
        organization_id: "org-1".to_string(),
        definition_id: def_id,
        workspace_id: workspace_id.clone(),
        gateway_id,
        name: "Data Stream in Org 1".to_string(),
    };
    data_stream_repo.create_data_stream(input).await.unwrap();

    // Try to get data stream with correct organization - should succeed
    let correct_input = GetDataStreamRepoInput {
        data_stream_id: "data-stream-in-org-1".to_string(),
        organization_id: "org-1".to_string(),
        workspace_id: workspace_id.clone(),
    };
    let result = data_stream_repo
        .get_data_stream(correct_input)
        .await
        .unwrap();
    assert!(result.is_some());

    // Try to get data stream with wrong organization - should return None
    let wrong_input = GetDataStreamRepoInput {
        data_stream_id: "data-stream-in-org-1".to_string(),
        organization_id: "org-2".to_string(),
        workspace_id: workspace_id.clone(),
    };
    let result = data_stream_repo.get_data_stream(wrong_input).await.unwrap();
    assert!(
        result.is_none(),
        "Data stream from org-1 should not be visible to org-2"
    );
}

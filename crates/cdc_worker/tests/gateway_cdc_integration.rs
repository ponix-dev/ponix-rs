#![cfg(feature = "integration-tests")]

use async_nats::jetstream;
use cdc_worker::domain::{CdcConfig, CdcProcess, EntityConfig, GatewayConverter};
use common::domain::{
    CreateGatewayRepoInput, CreateOrganizationRepoInputWithId, DeleteGatewayRepoInput,
    EmqxGatewayConfig, GatewayConfig, GatewayRepository, OrganizationRepository,
    RegisterUserRepoInputWithId, UpdateGatewayRepoInput, UserRepository,
};
use common::nats::NatsClient;
use common::postgres::{
    PostgresClient, PostgresGatewayRepository, PostgresOrganizationRepository,
    PostgresUserRepository,
};
use futures_util::stream::StreamExt;
use goose::MigrationRunner;
use ponix_proto_prost::gateway::v1::{Gateway, GatewayType};
use prost::Message;
use std::sync::Arc;
use std::time::Duration;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

// For manual CDC publication setup
use tokio_postgres::NoTls;

const STREAM_NAME: &str = "test_gateways";
const SUBJECT_PREFIX: &str = "test_gateways";
const TEST_USER_ID: &str = "test-user-001";

struct TestEnvironment {
    _postgres_container: ContainerAsync<GenericImage>,
    _nats_container: ContainerAsync<GenericImage>,
    gateway_repo: PostgresGatewayRepository,
    organization_repo: PostgresOrganizationRepository,
    nats_client: Arc<NatsClient>,
    _cdc_cancellation_token: CancellationToken,
}

async fn setup_test_env() -> TestEnvironment {
    // Initialize tracing for debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .try_init();

    // Start PostgreSQL container with logical replication enabled
    let postgres = GenericImage::new("ghcr.io/ponix-dev/ponix-postgres", "latest")
        .with_wait_for(testcontainers::core::WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_exposed_port(5432.into())
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_cmd([
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_replication_slots=10",
            "-c",
            "max_wal_senders=10",
        ])
        .start()
        .await
        .unwrap();

    let pg_host = postgres.get_host().await.unwrap();
    let pg_port = postgres.get_host_port_ipv4(5432).await.unwrap();

    info!("PostgreSQL started at {}:{}", pg_host, pg_port);

    // Start NATS container with JetStream enabled
    let nats_container = GenericImage::new("nats", "latest")
        .with_exposed_port(4222.into())
        .with_cmd(["-js"])
        .start()
        .await
        .unwrap();

    let nats_host = nats_container.get_host().await.unwrap();
    let nats_port = nats_container.get_host_port_ipv4(4222).await.unwrap();
    let nats_url = format!("nats://{}:{}", nats_host, nats_port);

    info!("NATS started at {}", nats_url);

    // Run PostgreSQL migrations
    let migrations_dir = format!(
        "{}/../../crates/init_process/migrations/postgres",
        env!("CARGO_MANIFEST_DIR")
    );
    let pg_dsn = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        pg_host, pg_port
    );
    let goose_path = which::which("goose").expect("goose binary not found");

    let migration_runner = MigrationRunner::new(
        goose_path.to_string_lossy().to_string(),
        migrations_dir,
        "postgres".to_string(),
        pg_dsn.clone(),
    );

    migration_runner
        .run_migrations()
        .await
        .expect("Migrations failed");

    info!("PostgreSQL migrations completed");

    // Set replica identity for gateways table (required for CDC)
    let client = tokio_postgres::connect(&pg_dsn, NoTls)
        .await
        .expect("Failed to connect for CDC setup");
    let (pg_client, connection) = client;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("PostgreSQL connection error: {}", e);
        }
    });

    pg_client
        .execute("ALTER TABLE gateways REPLICA IDENTITY FULL", &[])
        .await
        .expect("Failed to set replica identity");
    info!("CDC publication created by migration");

    // Create PostgreSQL client and repository
    let postgres_client = PostgresClient::new(
        &pg_host.to_string(),
        pg_port,
        "postgres",
        "postgres",
        "postgres",
        5,
    )
    .expect("Failed to create PostgreSQL client");

    let gateway_repo = PostgresGatewayRepository::new(postgres_client.clone());
    let organization_repo = PostgresOrganizationRepository::new(postgres_client.clone());

    // Create a test user (needed for user_organizations foreign key)
    let user_repo = PostgresUserRepository::new(postgres_client.clone());
    let user_input = RegisterUserRepoInputWithId {
        id: TEST_USER_ID.to_string(),
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        password_hash: "hashed_password".to_string(),
    };
    user_repo
        .register_user(user_input)
        .await
        .expect("Failed to create test user");
    info!("Test user created");

    // Create NATS client and ensure stream exists
    let nats_client = Arc::new(
        NatsClient::connect(&nats_url, Duration::from_secs(10))
            .await
            .expect("Failed to connect to NATS"),
    );

    // Create stream configuration
    nats_client
        .ensure_stream(STREAM_NAME)
        .await
        .expect("Failed to create NATS stream");

    info!("NATS stream '{}' created", STREAM_NAME);

    // Create CDC configuration
    let cdc_config = CdcConfig {
        pg_host: pg_host.to_string(),
        pg_port,
        pg_database: "postgres".to_string(),
        pg_user: "postgres".to_string(),
        pg_password: "postgres".to_string(),
        publication_name: "ponix_cdc_publication".to_string(),
        slot_name: "test_cdc_slot".to_string(),
        batch_size: 10,
        batch_timeout_ms: 100,
        retry_delay_ms: 1000,
        max_retry_attempts: 3,
    };

    // Create entity configuration for gateways
    let entity_config = EntityConfig {
        entity_name: SUBJECT_PREFIX.to_string(),
        table_name: "gateways".to_string(),
        converter: Box::new(GatewayConverter::new()),
    };

    // Create and start CDC process
    let cdc_cancellation_token = CancellationToken::new();
    let cdc_process = CdcProcess::new(
        cdc_config,
        Arc::clone(&nats_client),
        vec![entity_config],
        cdc_cancellation_token.clone(),
    );

    // Spawn CDC process in background
    let cdc_token_clone = cdc_cancellation_token.clone();
    tokio::spawn(async move {
        if let Err(e) = cdc_process.run().await {
            tracing::error!("CDC process error: {}", e);
        }
    });

    // Give CDC process time to start and create publication/slot
    sleep(Duration::from_secs(2)).await;
    info!("CDC process started");

    TestEnvironment {
        _postgres_container: postgres,
        _nats_container: nats_container,
        gateway_repo,
        organization_repo,
        nats_client,
        _cdc_cancellation_token: cdc_token_clone,
    }
}

/// Helper function to create an organization for testing
async fn create_test_organization(env: &TestEnvironment, org_id: &str) {
    env.organization_repo
        .create_organization(CreateOrganizationRepoInputWithId {
            id: org_id.to_string(),
            name: format!("Test Organization {}", org_id),
            user_id: TEST_USER_ID.to_string(),
        })
        .await
        .expect("Failed to create test organization");
}

async fn create_consumer_for_subject(
    stream: &jetstream::stream::Stream,
    subject: &str,
) -> Option<async_nats::jetstream::consumer::Consumer<async_nats::jetstream::consumer::pull::Config>>
{
    // Create ephemeral consumer starting from the first message
    // This ensures we capture all events for this subject
    stream
        .create_consumer(jetstream::consumer::pull::Config {
            filter_subject: subject.to_string(),
            deliver_policy: jetstream::consumer::DeliverPolicy::All,
            ack_policy: jetstream::consumer::AckPolicy::Explicit,
            ..Default::default()
        })
        .await
        .ok()
}

async fn consume_next_message(
    stream: &jetstream::stream::Stream,
    subject: &str,
    timeout_duration: Duration,
) -> Option<Gateway> {
    let consumer = create_consumer_for_subject(stream, subject).await?;

    // Try to fetch a message with timeout
    let result = timeout(timeout_duration, async {
        let mut messages = consumer.messages().await.ok()?;
        let msg_result = messages.next().await?;
        let message = msg_result.ok()?;

        // Decode the protobuf message
        let gateway = Gateway::decode(message.payload.clone()).ok()?;
        message.ack().await.ok()?;

        Some(gateway)
    })
    .await;

    result.ok().flatten()
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_create_cdc_event() {
    let env = setup_test_env().await;

    // Get JetStream context
    let js = env.nats_client.jetstream();
    let stream = js.get_stream(STREAM_NAME).await.unwrap();

    // Create organization first
    create_test_organization(&env, "test-org-001").await;

    // Create a gateway
    let gateway_id = "test-gateway-create-001".to_string();
    let input = CreateGatewayRepoInput {
        gateway_id: gateway_id.clone(),
        organization_id: "test-org-001".to_string(),
        name: "Test Gateway Create 001".to_string(),
        gateway_type: "EMQX".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://test.example.com:1883".to_string(),
        }),
    };

    env.gateway_repo
        .create_gateway(input)
        .await
        .expect("Failed to create gateway");

    info!("Gateway created in database");

    // Wait for and consume the CDC event
    let subject = format!("{}.create", SUBJECT_PREFIX);
    let gateway_event = consume_next_message(&stream, &subject, Duration::from_secs(10))
        .await
        .expect("Failed to receive CDC create event");

    // Validate the CDC event
    assert_eq!(gateway_event.gateway_id, gateway_id);
    assert_eq!(gateway_event.organization_id, "test-org-001");
    assert_eq!(gateway_event.r#type, GatewayType::Emqx as i32);
    assert!(gateway_event.created_at.is_some());

    // Validate config
    if let Some(config) = gateway_event.config {
        match config {
            ponix_proto_prost::gateway::v1::gateway::Config::EmqxConfig(emqx) => {
                assert_eq!(emqx.broker_url, "mqtt://test.example.com:1883");
            }
        }
    }

    debug!("CDC create event validated successfully");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_update_cdc_event() {
    let env = setup_test_env().await;

    // Get JetStream context
    let js = env.nats_client.jetstream();
    let stream = js.get_stream(STREAM_NAME).await.unwrap();

    // Create organization first
    create_test_organization(&env, "test-org-002").await;

    // Create a gateway first
    let gateway_id = "test-gateway-update-001".to_string();
    let create_input = CreateGatewayRepoInput {
        gateway_id: gateway_id.clone(),
        organization_id: "test-org-002".to_string(),
        name: "Test Gateway Update 001".to_string(),
        gateway_type: "EMQX".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://original.example.com:1883".to_string(),
        }),
    };

    env.gateway_repo
        .create_gateway(create_input)
        .await
        .expect("Failed to create gateway");

    // Consume the create event to clear the queue
    let create_subject = format!("{}.create", SUBJECT_PREFIX);
    consume_next_message(&stream, &create_subject, Duration::from_secs(5))
        .await
        .expect("Failed to consume initial create event");

    info!("Initial gateway created and create event consumed");

    // Update the gateway
    let update_input = UpdateGatewayRepoInput {
        gateway_id: gateway_id.clone(),
        organization_id: "test-org-002".to_string(),
        name: None,
        gateway_type: None,
        gateway_config: Some(GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://updated.example.com:8883".to_string(),
        })),
    };

    env.gateway_repo
        .update_gateway(update_input)
        .await
        .expect("Failed to update gateway");

    info!("Gateway updated in database");

    // Wait for and consume the CDC update event
    let update_subject = format!("{}.update", SUBJECT_PREFIX);
    let gateway_event = consume_next_message(&stream, &update_subject, Duration::from_secs(10))
        .await
        .expect("Failed to receive CDC update event");

    // Validate the CDC event
    assert_eq!(gateway_event.gateway_id, gateway_id);
    assert!(gateway_event.updated_at.is_some());

    // Validate config
    if let Some(config) = gateway_event.config {
        match config {
            ponix_proto_prost::gateway::v1::gateway::Config::EmqxConfig(emqx) => {
                assert_eq!(emqx.broker_url, "mqtt://updated.example.com:8883");
            }
        }
    }

    debug!("CDC update event validated successfully");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_gateway_delete_cdc_event() {
    let env = setup_test_env().await;

    // Get JetStream context
    let js = env.nats_client.jetstream();
    let stream = js.get_stream(STREAM_NAME).await.unwrap();

    // Create organization first
    create_test_organization(&env, "test-org-003").await;

    // Create a gateway first
    let gateway_id = "test-gateway-delete-001".to_string();
    let create_input = CreateGatewayRepoInput {
        gateway_id: gateway_id.clone(),
        organization_id: "test-org-003".to_string(),
        name: "Test Gateway Delete 001".to_string(),
        gateway_type: "EMQX".to_string(),
        gateway_config: GatewayConfig::Emqx(EmqxGatewayConfig {
            broker_url: "mqtt://delete-test.example.com:1883".to_string(),
        }),
    };

    env.gateway_repo
        .create_gateway(create_input)
        .await
        .expect("Failed to create gateway");

    // Consume the create event to clear the queue
    let create_subject = format!("{}.create", SUBJECT_PREFIX);
    consume_next_message(&stream, &create_subject, Duration::from_secs(5))
        .await
        .expect("Failed to consume initial create event");

    info!("Initial gateway created and create event consumed");

    // Delete the gateway (soft delete)
    let delete_input = DeleteGatewayRepoInput {
        gateway_id: gateway_id.clone(),
        organization_id: "test-org-003".to_string(),
    };
    env.gateway_repo
        .delete_gateway(delete_input)
        .await
        .expect("Failed to delete gateway");

    info!("Gateway deleted in database");

    // Wait for and consume the CDC update event (soft delete triggers UPDATE)
    let update_subject = format!("{}.update", SUBJECT_PREFIX);
    let gateway_event = consume_next_message(&stream, &update_subject, Duration::from_secs(10))
        .await
        .expect("Failed to receive CDC update event after delete");

    // Validate the CDC event - soft delete sets deleted_at
    assert_eq!(gateway_event.gateway_id, gateway_id);
    assert!(
        gateway_event.deleted_at.is_some(),
        "deleted_at should be set"
    );

    debug!("CDC delete event (soft delete) validated successfully");
}

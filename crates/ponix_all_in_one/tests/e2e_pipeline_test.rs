#![cfg(feature = "integration-tests")]

use analytics_worker::clickhouse::{
    ClickHouseInserterRepository, InserterCommitHandle, InserterConfig,
};
use analytics_worker::domain::{CelPayloadConverter, RawEnvelopeService};
use analytics_worker::nats::{
    ProcessedEnvelopeConsumerService, ProcessedEnvelopeProducer, RawEnvelopeConsumerService,
};
use anyhow::Result;
use goose::MigrationRunner;
use tower::ServiceBuilder;

use common::auth::MockAuthorizationProvider;
use common::clickhouse::ClickHouseClient;
use common::domain::{
    CreateEndDeviceDefinitionRepoInput, Device, EndDeviceDefinitionRepository, RawEnvelope,
    RawEnvelopeProducer as _,
};
use common::jsonschema::JsonSchemaValidator;
use common::nats::{
    NatsClient, NatsConsumeLoggingLayer, NatsConsumeTracingConfig, NatsConsumeTracingLayer,
    TowerConsumer,
};
use common::postgres::{
    PostgresClient, PostgresDeviceRepository, PostgresEndDeviceDefinitionRepository,
    PostgresOrganizationRepository,
};

use gateway_orchestrator::nats::RawEnvelopeProducer;

use ponix_api::domain::{CreateDeviceRequest, DeviceService};

use std::sync::Arc;
use std::time::Duration;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, Image};
use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::postgres::Postgres;
use tokio::time::sleep;

/// Custom ClickHouse image with version 24.10 for JSON type support
/// Exposes both HTTP (8123) and Native TCP (9000) ports
#[derive(Debug, Clone)]
struct ClickHouse24 {
    inner: ClickHouse,
    ports: Vec<ContainerPort>,
}

impl Default for ClickHouse24 {
    fn default() -> Self {
        Self {
            inner: ClickHouse::default(),
            ports: vec![
                ContainerPort::Tcp(8123), // HTTP interface
                ContainerPort::Tcp(9000), // Native TCP interface for migrations
            ],
        }
    }
}

impl Image for ClickHouse24 {
    fn name(&self) -> &str {
        "clickhouse/clickhouse-server"
    }

    fn tag(&self) -> &str {
        "24.10"
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        self.inner.ready_conditions()
    }

    fn env_vars(
        &self,
    ) -> impl IntoIterator<
        Item = (
            impl Into<std::borrow::Cow<'_, str>>,
            impl Into<std::borrow::Cow<'_, str>>,
        ),
    > {
        self.inner.env_vars()
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &self.ports
    }
}

/// Custom NATS image with JetStream enabled
#[derive(Debug, Clone)]
struct NatsWithJetStream {
    ports: Vec<ContainerPort>,
}

impl Default for NatsWithJetStream {
    fn default() -> Self {
        Self {
            ports: vec![ContainerPort::Tcp(4222)], // NATS client port
        }
    }
}

impl Image for NatsWithJetStream {
    fn name(&self) -> &str {
        "nats"
    }

    fn tag(&self) -> &str {
        "latest"
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        // Just wait a few seconds for NATS to start
        vec![WaitFor::seconds(3)]
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<std::borrow::Cow<'_, str>>> {
        // Enable JetStream with -js flag
        vec!["--js"]
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &self.ports
    }
}

/// Start all required containers in parallel
async fn start_containers() -> Result<(
    ContainerAsync<Postgres>,
    ContainerAsync<ClickHouse24>,
    ContainerAsync<NatsWithJetStream>,
    String, // postgres_url
    String, // clickhouse_http_url
    String, // clickhouse_native_url
    String, // nats_url
)> {
    // Start containers in parallel
    let (postgres, clickhouse, nats) = tokio::join!(
        Postgres::default().start(),
        ClickHouse24::default().start(),
        NatsWithJetStream::default().start()
    );

    let postgres = postgres?;
    let clickhouse = clickhouse?;
    let nats = nats?;

    // Extract connection details - PostgreSQL
    let pg_host = postgres.get_host().await?;
    let pg_port = postgres.get_host_port_ipv4(5432).await?;
    let postgres_url = format!(
        "postgres://postgres:postgres@{}:{}/postgres?sslmode=disable",
        pg_host, pg_port
    );

    // Extract connection details - ClickHouse
    let ch_host = clickhouse.get_host().await?;
    let ch_http_port = clickhouse.get_host_port_ipv4(8123).await?;
    let ch_native_port = clickhouse.get_host_port_ipv4(9000).await?;
    let clickhouse_http_url = format!("http://{}:{}", ch_host, ch_http_port);
    let clickhouse_native_url = format!("{}:{}", ch_host, ch_native_port);

    // Extract connection details - NATS
    let nats_host = nats.get_host().await?;
    let nats_port = nats.get_host_port_ipv4(4222).await?;
    let nats_url = format!("nats://{}:{}", nats_host, nats_port);

    Ok((
        postgres,
        clickhouse,
        nats,
        postgres_url,
        clickhouse_http_url,
        clickhouse_native_url,
        nats_url,
    ))
}

/// Run database migrations for both PostgreSQL and ClickHouse
async fn run_migrations(postgres_url: &str, clickhouse_native_url: &str) -> Result<()> {
    let goose_path = which::which("goose")
        .expect("goose binary not found - please install with: brew install goose");

    // Run PostgreSQL migrations
    // When running tests, CARGO_MANIFEST_DIR points to ponix-all-in-one crate
    // We need to go up to workspace root, then down to ponix-postgres/migrations
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find workspace root");

    let pg_migrations_dir = workspace_root.join("crates/init_process/migrations/postgres");
    let pg_runner = MigrationRunner::new(
        goose_path.to_string_lossy().to_string(),
        pg_migrations_dir.to_string_lossy().to_string(),
        "postgres".to_string(),
        postgres_url.to_string(),
    );
    pg_runner.run_migrations().await?;

    // Run ClickHouse migrations
    let ch_migrations_dir = workspace_root.join("crates/init_process/migrations/clickhouse");
    let ch_dsn = format!(
        "clickhouse://default:@{}/default?allow_experimental_json_type=1",
        clickhouse_native_url
    );
    let ch_runner = MigrationRunner::new(
        goose_path.to_string_lossy().to_string(),
        ch_migrations_dir.to_string_lossy().to_string(),
        "clickhouse".to_string(),
        ch_dsn,
    );
    ch_runner.run_migrations().await?;

    Ok(())
}

/// Initialize all required services (device, envelope, NATS)
async fn initialize_services(
    postgres_url: &str,
    nats_url: &str,
) -> Result<(
    Arc<DeviceService>,
    Arc<RawEnvelopeService>,
    Arc<NatsClient>,
    PostgresClient,
    Arc<PostgresEndDeviceDefinitionRepository>,
)> {
    // PostgreSQL client and repository
    // Parse postgres URL to extract connection details
    // Format: postgres://user:pass@host:port/dbname?sslmode=disable
    let url_parts: Vec<&str> = postgres_url.split('@').collect();
    let host_parts: Vec<&str> = url_parts[1].split('/').collect();
    let host_port: Vec<&str> = host_parts[0].split(':').collect();
    let host = host_port[0];
    let port: u16 = host_port[1].parse()?;

    let pg_client = PostgresClient::new(host, port, "postgres", "postgres", "postgres", 5)?;
    let device_repo = Arc::new(PostgresDeviceRepository::new(pg_client.clone()));
    let org_repo = Arc::new(PostgresOrganizationRepository::new(pg_client.clone()));
    let definition_repo = Arc::new(PostgresEndDeviceDefinitionRepository::new(pg_client.clone()));

    // Mock authorization provider that allows all operations (E2E test bypasses auth)
    let mut mock_auth = MockAuthorizationProvider::new();
    mock_auth
        .expect_require_permission()
        .returning(|_, _, _, _| Box::pin(async { Ok(()) }));

    // Device service
    let device_service = Arc::new(DeviceService::new(
        device_repo.clone(),
        org_repo.clone(),
        definition_repo.clone(),
        Arc::new(mock_auth),
    ));

    // NATS client - wait a moment for NATS to fully start with JetStream
    sleep(Duration::from_secs(2)).await;

    let nats_client = Arc::new(NatsClient::connect(nats_url, Duration::from_secs(30)).await?);

    // Ensure streams exist (JetStream must be enabled in NATS)
    nats_client.ensure_stream("raw_envelopes").await?;
    nats_client.ensure_stream("processed_envelopes").await?;

    // ProcessedEnvelopeProducer for RawEnvelopeService
    let processed_producer = Arc::new(ProcessedEnvelopeProducer::new(
        nats_client.create_publisher_client(),
        "processed_envelopes".to_string(),
    ));

    // Payload converter
    let payload_converter = Arc::new(CelPayloadConverter::new());

    // Schema validator
    let schema_validator = Arc::new(JsonSchemaValidator::new());

    // Raw envelope service
    let raw_envelope_service = Arc::new(RawEnvelopeService::new(
        device_repo,
        org_repo,
        payload_converter,
        processed_producer,
        schema_validator,
    ));

    Ok((
        device_service,
        raw_envelope_service,
        nats_client,
        pg_client,
        definition_repo,
    ))
}

/// Create a test organization and device with CEL expression for Cayenne LPP transformation
async fn create_test_device(
    device_service: &DeviceService,
    definition_repo: &PostgresEndDeviceDefinitionRepository,
    pg_client: &PostgresClient,
) -> Result<Device> {
    // First, create the test organization directly in the database
    // (OrganizationService not yet implemented in Phase 1)
    let conn = pg_client.get_connection().await?;
    conn.execute(
        "INSERT INTO organizations (id, name) VALUES ($1, $2)",
        &[&"test-org-123", &"Test Organization"],
    )
    .await?;

    // Create a workspace for the organization
    let workspace_id = format!(
        "ws-test-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    conn.execute(
        "INSERT INTO workspaces (id, name, organization_id) VALUES ($1, $2, $3)",
        &[&workspace_id, &"Test Workspace", &"test-org-123"],
    )
    .await?;

    // CEL expression that decodes Cayenne LPP and renames fields
    let cel_expression = r#"{
        'sensor_data': cayenne_lpp_decode(input),
        'device_type': 'environmental_sensor',
        'reading_temp_celsius': cayenne_lpp_decode(input).temperature_0,
        'reading_humidity_percent': cayenne_lpp_decode(input).humidity_1,
        'reading_pressure_hpa': cayenne_lpp_decode(input).barometer_2
    }"#;

    // Create the end device definition first
    let definition_id = format!(
        "def-test-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    definition_repo
        .create_definition(CreateEndDeviceDefinitionRepoInput {
            id: definition_id.clone(),
            organization_id: "test-org-123".to_string(),
            name: "Environmental Sensor Definition".to_string(),
            json_schema: "{}".to_string(),
            payload_conversion: cel_expression.to_string(),
        })
        .await?;

    let device = device_service
        .create_device(CreateDeviceRequest {
            user_id: "test-user-id".to_string(),
            organization_id: "test-org-123".to_string(),
            workspace_id,
            definition_id,
            name: "Test Environmental Sensor".to_string(),
        })
        .await?;

    Ok(device)
}

/// Produce raw messages with Cayenne LPP payload to the NATS stream
async fn produce_raw_messages(
    nats_client: &NatsClient,
    device: &Device,
    count: usize,
) -> Result<()> {
    // Create RawEnvelopeProducer
    let raw_producer = RawEnvelopeProducer::new(
        nats_client.create_publisher_client(),
        "raw_envelopes".to_string(),
    );

    // Cayenne LPP payload with 3 sensor readings:
    // Channel 0: Temperature (25.5°C) - type 0x67 (temp), value 0x00FF = 255, 255/10 = 25.5
    // Channel 1: Humidity (65%) - type 0x68 (humidity), value 0x82 = 130, 130/2 = 65.0
    // Channel 2: Barometer (1013.25 hPa) - type 0x73 (barometer), value 0x2795 = 10133, 10133/10 = 1013.3
    let cayenne_payload = vec![
        0x00, 0x67, 0x00, 0xFF, // Ch 0, temp: 25.5°C
        0x01, 0x68, 0x82, // Ch 1, humidity: 65%
        0x02, 0x73, 0x27, 0x95, // Ch 2, barometer: 1013.3 hPa
    ];

    let base_time = chrono::Utc::now();

    for i in 0..count {
        let raw_envelope = RawEnvelope {
            organization_id: device.organization_id.clone(),
            end_device_id: device.device_id.clone(),
            occurred_at: base_time + chrono::Duration::seconds(i as i64),
            payload: cayenne_payload.clone(),
        };

        raw_producer.publish_raw_envelope(&raw_envelope).await?;

        // Small delay to prevent overwhelming the system
        if i % 10 == 9 {
            sleep(Duration::from_millis(10)).await;
        }
    }

    Ok(())
}

/// Run consumers to process messages from NATS and write to ClickHouse
async fn run_consumers(
    raw_envelope_service: Arc<RawEnvelopeService>,
    clickhouse_http_url: &str,
    nats_client: Arc<NatsClient>,
) -> Result<()> {
    // Create cancellation token for graceful shutdown
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Create ClickHouse inserter for processed envelope consumer
    let ch_client = ClickHouseClient::new(clickhouse_http_url, "default", "default", "");
    let inserter_config = InserterConfig {
        max_entries: 100,
        period: Duration::from_millis(500),
    };
    let inserter_repo =
        ClickHouseInserterRepository::new(&ch_client, "processed_envelopes", inserter_config)?;

    // Create raw envelope consumer with Tower middleware
    let raw_service = RawEnvelopeConsumerService::new(raw_envelope_service);
    let raw_service_stack = ServiceBuilder::new()
        .layer(NatsConsumeTracingLayer::new(NatsConsumeTracingConfig::new(
            "raw_envelope_consume",
        )))
        .layer(NatsConsumeLoggingLayer::new())
        .service(raw_service);

    let raw_consumer_client = nats_client.create_consumer_client();
    let raw_consumer = TowerConsumer::new(
        raw_consumer_client,
        "raw_envelopes",
        "test-raw-consumer",
        "raw_envelopes.>",
        30, // batch size
        5,  // max wait secs
        raw_service_stack,
    )
    .await?;

    // Create processed envelope consumer with Tower middleware
    let processed_service = ProcessedEnvelopeConsumerService::new(inserter_repo.clone());
    let processed_service_stack = ServiceBuilder::new()
        .layer(NatsConsumeTracingLayer::new(NatsConsumeTracingConfig::new(
            "processed_envelope_consume",
        )))
        .layer(NatsConsumeLoggingLayer::new())
        .service(processed_service);

    let processed_consumer_client = nats_client.create_consumer_client();
    let processed_consumer = TowerConsumer::new(
        processed_consumer_client,
        "processed_envelopes",
        "test-processed-consumer",
        "processed_envelopes.>",
        30, // batch size
        5,  // max wait secs
        processed_service_stack,
    )
    .await?;

    // Create inserter commit handle for background commits
    let commit_handle = InserterCommitHandle::new(inserter_repo);

    // Spawn consumers and commit loop in background
    let raw_handle = tokio::spawn({
        let token = cancel_token.clone();
        async move { raw_consumer.run(token).await }
    });

    let processed_handle = tokio::spawn({
        let token = cancel_token.clone();
        async move { processed_consumer.run(token).await }
    });

    let commit_handle_task = tokio::spawn({
        let token = cancel_token.clone();
        async move { commit_handle.run(token, Duration::from_millis(250)).await }
    });

    // Wait for processing to complete
    sleep(Duration::from_secs(10)).await;

    // Cancel consumers and commit loop
    cancel_token.cancel();

    // Wait for clean shutdown
    let _ = tokio::time::timeout(Duration::from_secs(5), raw_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), processed_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), commit_handle_task).await;

    Ok(())
}

/// Verify that all processed messages were correctly stored in ClickHouse
async fn verify_clickhouse_data(
    clickhouse_http_url: &str,
    device: &Device,
    expected_count: usize,
) -> Result<()> {
    use clickhouse::Client;

    // Just query the data field as a string
    #[derive(Debug, clickhouse::Row, serde::Deserialize)]
    struct EnvelopeData {
        data: String, // JSON string
    }

    let client = Client::default()
        .with_url(clickhouse_http_url)
        .with_database("default");

    // Query for count
    let count_query = format!(
        "SELECT COUNT(*) as count FROM processed_envelopes WHERE end_device_id = '{}'",
        device.device_id
    );

    let count: u64 = client.query(&count_query).fetch_one().await?;

    assert_eq!(
        count as usize, expected_count,
        "Expected {} rows, found {}",
        expected_count, count
    );

    // Query for sample row to validate transformation
    // IMPORTANT: Cast JSON type to String for reading
    let sample_query = format!(
        "SELECT CAST(data AS String) as data FROM processed_envelopes
         WHERE end_device_id = '{}'
         LIMIT 1",
        device.device_id
    );

    let row: EnvelopeData = client.query(&sample_query).fetch_one().await?;

    // Parse and validate JSON data
    let data: serde_json::Value = serde_json::from_str(&row.data)?;

    // Verify CEL transformation worked correctly
    assert!(
        data.get("sensor_data").is_some(),
        "Missing sensor_data field"
    );
    assert_eq!(
        data.get("device_type").and_then(|v| v.as_str()),
        Some("environmental_sensor")
    );

    // Verify renamed fields exist
    assert!(
        data.get("reading_temp_celsius").is_some(),
        "Missing temperature field"
    );
    assert!(
        data.get("reading_humidity_percent").is_some(),
        "Missing humidity field"
    );
    assert!(
        data.get("reading_pressure_hpa").is_some(),
        "Missing pressure field"
    );

    // Verify values match expected Cayenne decoding
    let temp = data
        .get("reading_temp_celsius")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!(
        (temp - 25.5).abs() < 0.01,
        "Temperature value incorrect: {}",
        temp
    );

    let humidity = data
        .get("reading_humidity_percent")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!(
        (humidity - 65.0).abs() < 0.01,
        "Humidity value incorrect: {}",
        humidity
    );

    let pressure = data
        .get("reading_pressure_hpa")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!(
        (pressure - 1013.3).abs() < 0.01,
        "Pressure value incorrect: {}",
        pressure
    );

    Ok(())
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn test_end_to_end_raw_to_processed_pipeline() -> Result<()> {
    // Phase 1 & 2: Start containers and run migrations
    let (
        _pg_container,
        _ch_container,
        _nats_container,
        postgres_url,
        clickhouse_http_url,
        clickhouse_native_url,
        nats_url,
    ) = start_containers().await?;

    run_migrations(&postgres_url, &clickhouse_native_url).await?;

    println!("✅ Phase 2 completed: Containers started and migrations run");
    println!("   - PostgreSQL: {}", postgres_url);
    println!("   - ClickHouse HTTP: {}", clickhouse_http_url);
    println!("   - NATS: {}", nats_url);

    // Phase 3: Initialize services and create device
    let (device_service, raw_envelope_service, nats_client, pg_client, definition_repo) =
        initialize_services(&postgres_url, &nats_url).await?;

    let device = create_test_device(&device_service, &definition_repo, &pg_client).await?;

    println!("✅ Phase 3 completed: Services initialized and device created");
    println!("   - Device ID: {}", device.device_id);
    println!("   - Organization: {}", device.organization_id);
    println!("   - Device Name: {}", device.name);

    // Phase 4: Produce 100 raw messages
    produce_raw_messages(&nats_client, &device, 100).await?;

    println!("✅ Phase 4 completed: 100 raw messages produced");
    println!("   - Messages sent to NATS raw_envelopes stream");

    // Phase 5: Run consumers to process messages
    run_consumers(
        raw_envelope_service,
        &clickhouse_http_url,
        nats_client.clone(),
    )
    .await?;

    println!("✅ Phase 5 completed: Consumers processed all messages");
    println!("   - Raw envelopes converted to processed envelopes");
    println!("   - Processed envelopes written to ClickHouse");

    // Phase 6: Verify results in ClickHouse
    verify_clickhouse_data(&clickhouse_http_url, &device, 100).await?;

    println!("✅ Phase 6 completed: Verification successful");
    println!("   - All 100 records found in ClickHouse");
    println!("   - CEL transformations validated");
    println!("   - Sensor data correctly decoded");
    println!();
    println!("🎉 End-to-end test completed successfully!");
    println!("   - 100 raw messages produced");
    println!("   - All messages processed through pipeline");
    println!("   - CEL transformations applied correctly");
    println!("   - All data persisted to ClickHouse");

    Ok(())
}

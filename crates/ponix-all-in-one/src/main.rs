mod config;

use ponix_clickhouse::{ClickHouseClient, ClickHouseEnvelopeRepository};
use ponix_domain::{DeviceService, ProcessedEnvelopeService};
use ponix_grpc::{run_grpc_server, GrpcServerConfig};
use ponix_nats::{
    create_domain_processor, run_demo_producer, DemoProducerConfig,
    NatsClient, NatsConsumer, ProcessedEnvelopeProducer,
};
use ponix_postgres::{PostgresClient, PostgresDeviceRepository};
use ponix_runner::Runner;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing
    let config = match config::ServiceConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    info!("Starting ponix-all-in-one service");
    debug!("Configuration: {:?}", config);

    let startup_timeout = Duration::from_secs(config.startup_timeout_secs);
    debug!(
        "Using startup timeout of {:?} for all initialization",
        startup_timeout
    );

    // PHASE 1: Run PostgreSQL migrations
    info!("Running PostgreSQL migrations...");
    let postgres_dsn = format!(
        "postgres://{}:{}@{}:{}/{}?sslmode=disable",
        config.postgres_username,
        config.postgres_password,
        config.postgres_host,
        config.postgres_port,
        config.postgres_database
    );
    let postgres_migration_runner = ponix_postgres::MigrationRunner::new(
        config.postgres_goose_binary_path.clone(),
        config.postgres_migrations_dir.clone(),
        "postgres".to_string(),
        postgres_dsn,
    );

    if let Err(e) = postgres_migration_runner.run_migrations().await {
        error!("Failed to run PostgreSQL migrations: {}", e);
        std::process::exit(1);
    }

    // Initialize PostgreSQL client for device repository
    info!("Connecting to PostgreSQL...");
    let postgres_client = match PostgresClient::new(
        &config.postgres_host,
        config.postgres_port,
        &config.postgres_database,
        &config.postgres_username,
        &config.postgres_password,
        5, // max connections
    ) {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to create PostgreSQL client: {}", e);
            std::process::exit(1);
        }
    };
    info!("PostgreSQL connection established");

    // Initialize device repository and domain service
    let device_repository = PostgresDeviceRepository::new(postgres_client);
    let device_service = Arc::new(DeviceService::new(Arc::new(device_repository)));
    info!("Device service initialized");

    // PHASE 2: Run ClickHouse migrations
    info!("Running ClickHouse migrations...");
    let clickhouse_dsn = format!(
        "clickhouse://{}:{}@{}/{}?allow_experimental_json_type=1",
        config.clickhouse_username,
        config.clickhouse_password,
        config.clickhouse_native_url,
        config.clickhouse_database
    );
    let clickhouse_migration_runner = ponix_clickhouse::MigrationRunner::new(
        config.clickhouse_goose_binary_path.clone(),
        config.clickhouse_migrations_dir.clone(),
        "clickhouse".to_string(),
        clickhouse_dsn,
    );

    if let Err(e) = clickhouse_migration_runner.run_migrations().await {
        error!("Failed to run ClickHouse migrations: {}", e);
        std::process::exit(1);
    }

    // PHASE 3: Initialize ClickHouse client
    info!("Connecting to ClickHouse...");
    let clickhouse_client = ClickHouseClient::new(
        &config.clickhouse_url,
        &config.clickhouse_database,
        &config.clickhouse_username,
        &config.clickhouse_password,
    );

    if let Err(e) = clickhouse_client.ping().await {
        error!("Failed to ping ClickHouse: {}", e);
        std::process::exit(1);
    }
    info!("ClickHouse connection established");

    // Initialize ProcessedEnvelope domain layer
    info!("Initializing processed envelope domain service");

    // Create repository (infrastructure layer)
    let envelope_repository = ClickHouseEnvelopeRepository::new(
        clickhouse_client.clone(),
        "processed_envelopes".to_string(),
    );

    // Create domain service with repository
    let envelope_service = Arc::new(ProcessedEnvelopeService::new(Arc::new(envelope_repository)));
    info!("Processed envelope service initialized");

    // PHASE 4: Connect to NATS
    let nats_client = match NatsClient::connect(&config.nats_url, startup_timeout).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to NATS: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = nats_client.ensure_stream(&config.nats_stream).await {
        error!("Failed to ensure stream exists: {}", e);
        std::process::exit(1);
    }

    // PHASE 5: Create processor using domain service
    let processor = create_domain_processor(envelope_service.clone());

    // Create consumer using trait-based API
    let consumer_client = nats_client.create_consumer_client();
    let consumer = match NatsConsumer::new(
        consumer_client,
        &config.nats_stream,
        "ponix-all-in-one",
        &config.nats_subject,
        config.nats_batch_size,
        config.nats_batch_wait_secs,
        processor,
    )
    .await
    {
        Ok(consumer) => consumer,
        Err(e) => {
            error!("Failed to create consumer: {}", e);
            std::process::exit(1);
        }
    };

    // Create producer using trait-based API
    let publisher_client = nats_client.create_publisher_client();
    let producer = ProcessedEnvelopeProducer::new(publisher_client, config.nats_stream.clone());

    // Create gRPC server configuration
    let grpc_config = GrpcServerConfig {
        host: config.grpc_host.clone(),
        port: config.grpc_port,
    };

    // Create demo producer configuration
    let demo_config = DemoProducerConfig {
        interval: Duration::from_secs(config.interval_secs),
        organization_id: "example-org".to_string(),
        log_message: config.message.clone(),
    };

    // Create runner with producer, consumer, and gRPC server processes
    let runner = Runner::new()
        .with_app_process(move |ctx| Box::pin(async move {
            run_demo_producer(ctx, demo_config, producer).await
        }))
        .with_app_process(move |ctx| Box::pin(async move { consumer.run(ctx).await }))
        .with_app_process({
            let service = device_service.clone();
            move |ctx| Box::pin(async move { run_grpc_server(grpc_config, service, ctx).await })
        })
        .with_closer(move || {
            Box::pin(async move {
                info!("Running cleanup tasks...");
                nats_client.close().await;
                info!("Cleanup complete");
                Ok(())
            })
        })
        .with_closer_timeout(Duration::from_secs(10));

    // Run the service (this will handle the exit)
    runner.run().await;
}

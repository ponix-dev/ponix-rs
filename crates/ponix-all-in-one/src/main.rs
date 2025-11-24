mod config;

use ponix_clickhouse::{ClickHouseClient, ClickHouseEnvelopeRepository};
use ponix_domain::{
    DeviceService, GatewayService, OrganizationService, ProcessedEnvelopeService,
    RawEnvelopeService,
};
use ponix_grpc::{run_grpc_server, GrpcServerConfig};
use ponix_nats::{
    create_domain_processor, create_raw_envelope_domain_processor, NatsClient, NatsConsumer,
    ProcessedEnvelopeProducer,
};
use ponix_payload::CelPayloadConverter;
use ponix_postgres::{
    CdcConfig, CdcProcess, PostgresClient, PostgresDeviceRepository, PostgresGatewayRepository,
    PostgresOrganizationRepository,
};
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

    // Initialize repositories
    let device_repository = Arc::new(PostgresDeviceRepository::new(postgres_client.clone()));
    let organization_repository =
        Arc::new(PostgresOrganizationRepository::new(postgres_client.clone()));
    let gateway_repository = Arc::new(PostgresGatewayRepository::new(postgres_client));

    // Initialize domain services
    let device_service = Arc::new(DeviceService::new(
        device_repository.clone(),
        organization_repository.clone(),
    ));
    info!("Device service initialized");

    let organization_service = Arc::new(OrganizationService::new(organization_repository.clone()));
    info!("Organization service initialized");

    let gateway_service = Arc::new(GatewayService::new(
        gateway_repository,
        organization_repository.clone(),
    ));
    info!("Gateway service initialized");

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
        Ok(client) => Arc::new(client),
        Err(e) => {
            error!("Failed to connect to NATS: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = nats_client
        .ensure_stream(&config.processed_envelopes_stream)
        .await
    {
        error!("Failed to ensure stream exists: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = nats_client.ensure_stream(&config.nats_raw_stream).await {
        error!("Failed to ensure raw envelope stream exists: {}", e);
        std::process::exit(1);
    }

    // PHASE 5: Create processor using domain service
    let processor = create_domain_processor(envelope_service.clone());

    // Create consumer using trait-based API
    let consumer_client = nats_client.create_consumer_client();
    let consumer = match NatsConsumer::new(
        consumer_client,
        &config.processed_envelopes_stream,
        "ponix-all-in-one",
        &config.processed_envelopes_subject,
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

    // Initialize RawEnvelope infrastructure
    info!("Initializing raw envelope infrastructure");

    // Create shared JetStream publisher for ProcessedEnvelope production
    let publisher_client = nats_client.create_publisher_client();

    // Create payload converter (CEL)
    let payload_converter = Arc::new(CelPayloadConverter::new());

    // Create ProcessedEnvelope producer for RawEnvelopeService
    let processed_envelope_producer = Arc::new(ProcessedEnvelopeProducer::new(
        publisher_client,
        config.processed_envelopes_stream.clone(),
    ));

    // Create RawEnvelopeService
    let raw_envelope_service = Arc::new(RawEnvelopeService::new(
        device_repository.clone(),
        organization_repository.clone(),
        payload_converter,
        processed_envelope_producer,
    ));

    // Create RawEnvelope consumer processor
    let raw_processor = create_raw_envelope_domain_processor(raw_envelope_service.clone());

    // Create RawEnvelope consumer
    let raw_consumer_client = nats_client.create_consumer_client();
    let raw_consumer = match NatsConsumer::new(
        raw_consumer_client,
        &config.nats_raw_stream,
        "ponix-all-in-one-raw",
        &config.nats_raw_subject,
        config.nats_batch_size,
        config.nats_batch_wait_secs,
        raw_processor,
    )
    .await
    {
        Ok(consumer) => consumer,
        Err(e) => {
            error!("Failed to create raw envelope consumer: {}", e);
            std::process::exit(1);
        }
    };

    info!("Raw envelope infrastructure initialized");

    // PHASE 6: CDC Setup
    info!("Setting up CDC for gateway changes");

    // Load CDC configuration
    let cdc_config = match CdcConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load CDC configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Create gRPC server configuration
    let grpc_config = GrpcServerConfig {
        host: config.grpc_host.clone(),
        port: config.grpc_port,
    };

    // Create runner with consumer processes, CDC, and gRPC server
    let runner = Runner::new()
        .with_app_process(move |ctx| Box::pin(async move { consumer.run(ctx).await }))
        .with_app_process(move |ctx| Box::pin(async move { raw_consumer.run(ctx).await }))
        .with_app_process({
            let cdc_config = cdc_config.clone();
            let nats_for_cdc = Arc::clone(&nats_client);
            move |ctx| {
                let cdc_process = CdcProcess::new(cdc_config, nats_for_cdc, ctx);
                Box::pin(async move { cdc_process.run().await })
            }
        })
        .with_app_process({
            let device_svc = device_service.clone();
            let org_svc = organization_service.clone();
            let gateway_svc = gateway_service.clone();
            move |ctx| {
                Box::pin(async move {
                    run_grpc_server(grpc_config, device_svc, org_svc, gateway_svc, ctx).await
                })
            }
        })
        .with_closer({
            let nats_for_close = Arc::clone(&nats_client);
            move || {
                Box::pin(async move {
                    info!("Running cleanup tasks...");
                    // Try to unwrap Arc to get ownership for close()
                    if let Ok(client) = Arc::try_unwrap(nats_for_close) {
                        client.close().await;
                    } else {
                        info!("Could not unwrap NATS client for close (still in use)");
                    }
                    info!("Cleanup complete");
                    Ok(())
                })
            }
        })
        .with_closer_timeout(Duration::from_secs(10));

    // Run the service (this will handle the exit)
    runner.run().await;
}

mod config;

use analytics_worker::analytics_worker::{AnalyticsWorker, AnalyticsWorkerConfig};
use cdc_worker::cdc_worker::{CdcWorker, CdcWorkerConfig};
use cdc_worker::domain::{CdcConfig, EntityConfig, GatewayConverter};
use common::clickhouse::ClickHouseClient;
use common::grpc::{CorsConfig, GrpcLoggingConfig, GrpcServerConfig, GrpcTracingConfig};
use common::nats::NatsClient;
use common::postgres::{
    PostgresClient, PostgresDeviceRepository, PostgresGatewayRepository,
    PostgresOrganizationRepository, PostgresUserRepository,
};
use common::telemetry::{init_telemetry, shutdown_telemetry, TelemetryConfig, TelemetryProviders};
use config::ServiceConfig;
use gateway_orchestrator::gateway_orchestrator::{GatewayOrchestrator, GatewayOrchestratorConfig};
use goose::MigrationRunner;
use ponix_api::domain::{DeviceService, GatewayService, JwtConfig, OrganizationService, UserService};
use ponix_api::ponix_api::PonixApi;
use ponix_runner::Runner;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

#[tokio::main]
async fn main() {
    // Initialize configuration and tracing
    let config = match ServiceConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize telemetry (tracing + OpenTelemetry for traces and logs)
    let telemetry_providers: Option<TelemetryProviders> = match init_telemetry(&TelemetryConfig {
        service_name: config.otel_service_name.clone(),
        otel_endpoint: config.otel_endpoint.clone(),
        otel_enabled: config.otel_enabled,
        log_level: config.log_level.clone(),
    }) {
        Ok(provider) => provider,
        Err(e) => {
            eprintln!("Failed to initialize telemetry: {}", e);
            std::process::exit(1);
        }
    };

    info!(
        otel_enabled = config.otel_enabled,
        otel_endpoint = %config.otel_endpoint,
        "Starting ponix-all-in-one service"
    );
    debug!("Configuration: {:?}", config);

    // Initialize shared dependencies
    let (postgres_repos, clickhouse_client, nats_client) =
        match initialize_shared_dependencies(&config).await {
            Ok(deps) => deps,
            Err(e) => {
                error!("Failed to initialize shared dependencies: {}", e);
                std::process::exit(1);
            }
        };

    // Initialize domain services
    let device_service = Arc::new(DeviceService::new(
        postgres_repos.device.clone(),
        postgres_repos.organization.clone(),
    ));
    let organization_service = Arc::new(OrganizationService::new(
        postgres_repos.organization.clone(),
    ));
    let gateway_service = Arc::new(GatewayService::new(
        postgres_repos.gateway.clone(),
        postgres_repos.organization.clone(),
    ));
    let jwt_config = JwtConfig {
        secret: config.jwt_secret.clone(),
        expiration_hours: config.jwt_expiration_hours,
    };
    let user_service = Arc::new(UserService::new(postgres_repos.user.clone(), jwt_config));

    // Parse ignored paths from config (comma-separated)
    let ignored_paths: Vec<String> = config
        .grpc_ignored_paths
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Build gRPC server config with optional gRPC-Web support
    let grpc_config = {
        let mut cfg = GrpcServerConfig {
            host: config.grpc_host.clone(),
            port: config.grpc_port,
            logging_config: GrpcLoggingConfig::new(ignored_paths.clone()),
            tracing_config: GrpcTracingConfig::new(ignored_paths),
            enable_grpc_web: config.grpc_web_enabled,
            cors_config: None,
        };
        if config.grpc_web_enabled {
            cfg.cors_config = Some(CorsConfig::from_comma_separated(
                &config.grpc_cors_allowed_origins,
            ));
        }
        cfg
    };

    // Initialize application modules
    let ponix_api = PonixApi::new(
        device_service.clone(),
        organization_service,
        gateway_service,
        user_service,
        grpc_config,
    );

    // Create orchestrator shutdown token - owned by main for lifecycle coordination
    let orchestrator_shutdown_token = tokio_util::sync::CancellationToken::new();

    let gateway_orchestrator = match GatewayOrchestrator::new(
        postgres_repos.gateway.clone(),
        nats_client.clone(),
        orchestrator_shutdown_token.clone(),
        GatewayOrchestratorConfig {
            gateway_stream: config.nats_gateway_stream.clone(),
            gateway_consumer_name: config.gateway_consumer_name.clone(),
            gateway_filter_subject: config.gateway_filter_subject.clone(),
            raw_envelopes_stream: config.nats_raw_stream.clone(),
        },
    )
    .await
    {
        Ok(api) => api,
        Err(e) => {
            error!("Failed to initialize gateway API: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize CDC Worker
    let gateway_entity_config = EntityConfig {
        entity_name: config.cdc_gateway_entity_name.clone(),
        table_name: config.cdc_gateway_table_name.clone(),
        converter: Box::new(GatewayConverter::new()),
    };
    let cdc_worker = CdcWorker::new(
        nats_client.clone(),
        CdcWorkerConfig {
            cdc_config: build_cdc_config(&config),
            entity_configs: vec![gateway_entity_config],
        },
    );

    let analytics_ingester = match AnalyticsWorker::new(
        postgres_repos.device.clone(),
        postgres_repos.organization.clone(),
        clickhouse_client,
        nats_client.clone(),
        AnalyticsWorkerConfig {
            processed_envelopes_stream: config.processed_envelopes_stream.clone(),
            processed_envelopes_subject: config.processed_envelopes_subject.clone(),
            raw_stream: config.nats_raw_stream.clone(),
            raw_subject: config.nats_raw_subject.clone(),
            nats_batch_size: config.nats_batch_size,
            nats_batch_wait_secs: config.nats_batch_wait_secs,
            // ClickHouse inserter settings - using defaults for now
            clickhouse_inserter_max_entries: 10_000,
            clickhouse_inserter_period_secs: 1,
        },
    )
    .await
    {
        Ok(ingester) => ingester,
        Err(e) => {
            error!("Failed to initialize analytics ingester: {}", e);
            std::process::exit(1);
        }
    };

    // Build runner with all processes
    let mut runner = Runner::new();

    // Add Ponix API process
    runner = runner.with_named_process("ponix_api", ponix_api.into_runner_process());

    // Add Gateway API process (handles NATS CDC events → orchestrator)
    runner = runner.with_named_process(
        "gateway_orchestrator",
        gateway_orchestrator.into_runner_process(),
    );

    // Add CDC Worker process (handles Postgres → NATS CDC events)
    runner = runner.with_named_process("cdc_worker", cdc_worker.into_runner_process());

    // Add Analytics Ingester processes
    let analytics_processes = analytics_ingester.into_runner_processes();
    for (i, process) in analytics_processes.into_iter().enumerate() {
        runner = runner.with_named_process(format!("analytics_worker_{}", i), process);
    }

    // Add cleanup handlers
    runner = runner
        .with_closer({
            let nats_for_close = Arc::clone(&nats_client);
            move || {
                Box::pin(async move {
                    info!("Running cleanup tasks...");
                    // TODO: move this and all other closers in to individual modules
                    orchestrator_shutdown_token.cancel();
                    if let Ok(client) = Arc::try_unwrap(nats_for_close) {
                        client.close().await;
                    }

                    // Shutdown telemetry and flush pending traces and logs
                    shutdown_telemetry(telemetry_providers);

                    info!("Cleanup complete");
                    Ok(())
                })
            }
        })
        .with_closer_timeout(Duration::from_secs(10));

    // Run the service
    runner.run().await;
}

struct PostgresRepositories {
    device: Arc<PostgresDeviceRepository>,
    organization: Arc<PostgresOrganizationRepository>,
    gateway: Arc<PostgresGatewayRepository>,
    user: Arc<PostgresUserRepository>,
}

async fn initialize_shared_dependencies(
    config: &ServiceConfig,
) -> anyhow::Result<(PostgresRepositories, ClickHouseClient, Arc<NatsClient>)> {
    // PostgreSQL initialization
    info!("Initializing PostgreSQL...");
    run_postgres_migrations(config).await?;
    let postgres_client = create_postgres_client(config)?;
    let postgres_repos = PostgresRepositories {
        device: Arc::new(PostgresDeviceRepository::new(postgres_client.clone())),
        organization: Arc::new(PostgresOrganizationRepository::new(postgres_client.clone())),
        gateway: Arc::new(PostgresGatewayRepository::new(postgres_client.clone())),
        user: Arc::new(PostgresUserRepository::new(postgres_client)),
    };

    // ClickHouse initialization
    info!("Initializing ClickHouse...");
    run_clickhouse_migrations(config).await?;
    let clickhouse_client = create_clickhouse_client(config).await?;

    // NATS initialization
    info!("Initializing NATS...");
    let nats_client = Arc::new(
        NatsClient::connect(
            &config.nats_url,
            Duration::from_secs(config.startup_timeout_secs),
        )
        .await?,
    );
    ensure_nats_streams(&nats_client, config).await?;

    Ok((postgres_repos, clickhouse_client, nats_client))
}

async fn run_postgres_migrations(config: &ServiceConfig) -> anyhow::Result<()> {
    let postgres_dsn = format!(
        "postgres://{}:{}@{}:{}/{}?sslmode=disable",
        config.postgres_username,
        config.postgres_password,
        config.postgres_host,
        config.postgres_port,
        config.postgres_database
    );
    let runner = MigrationRunner::new(
        config.postgres_goose_binary_path.clone(),
        config.postgres_migrations_dir.clone(),
        "postgres".to_string(),
        postgres_dsn,
    );
    runner.run_migrations().await
}

async fn run_clickhouse_migrations(config: &ServiceConfig) -> anyhow::Result<()> {
    let clickhouse_dsn = format!(
        "clickhouse://{}:{}@{}/{}?allow_experimental_json_type=1",
        config.clickhouse_username,
        config.clickhouse_password,
        config.clickhouse_native_url,
        config.clickhouse_database
    );
    let runner = MigrationRunner::new(
        config.clickhouse_goose_binary_path.clone(),
        config.clickhouse_migrations_dir.clone(),
        "clickhouse".to_string(),
        clickhouse_dsn,
    );
    runner.run_migrations().await
}

fn create_postgres_client(config: &ServiceConfig) -> anyhow::Result<PostgresClient> {
    PostgresClient::new(
        &config.postgres_host,
        config.postgres_port,
        &config.postgres_database,
        &config.postgres_username,
        &config.postgres_password,
        5, // max connections
    )
}

async fn create_clickhouse_client(config: &ServiceConfig) -> anyhow::Result<ClickHouseClient> {
    let client = ClickHouseClient::new(
        &config.clickhouse_url,
        &config.clickhouse_database,
        &config.clickhouse_username,
        &config.clickhouse_password,
    );
    client.ping().await?;
    Ok(client)
}

async fn ensure_nats_streams(client: &NatsClient, config: &ServiceConfig) -> anyhow::Result<()> {
    client
        .ensure_stream(&config.processed_envelopes_stream)
        .await?;
    client.ensure_stream(&config.nats_raw_stream).await?;
    client.ensure_stream(&config.nats_gateway_stream).await?;
    Ok(())
}

fn build_cdc_config(config: &ServiceConfig) -> CdcConfig {
    CdcConfig {
        pg_host: config.postgres_host.clone(),
        pg_port: config.postgres_port,
        pg_database: config.postgres_database.clone(),
        pg_user: config.postgres_username.clone(),
        pg_password: config.postgres_password.clone(),
        publication_name: config.cdc_publication_name.clone(),
        slot_name: config.cdc_slot_name.clone(),
        batch_size: config.cdc_batch_size,
        batch_timeout_ms: config.cdc_batch_timeout_ms,
        retry_delay_ms: config.cdc_retry_delay_ms,
        max_retry_attempts: config.cdc_max_retry_attempts,
    }
}

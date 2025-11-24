# Refactor ponix-all-in-one into Logical Service Modules Implementation Plan

## Overview

Refactor the `ponix-all-in-one` service to organize code into three distinct logical application modules while keeping everything in a single binary. This will enable clear separation of concerns within the monolith and make future service splitting trivial.

## Current State Analysis

The current `ponix-all-in-one` service is a monolithic binary that initializes all dependencies sequentially in `main.rs` (385 lines) and runs 5 concurrent processes via the Runner pattern. All initialization logic is interleaved in a single main function, making it difficult to understand boundaries between different logical services.

### Key Discoveries:
- All dependencies are initialized in sequence in main.rs:52-310
- Five concurrent processes run via Runner pattern in main.rs:336-358
- Shared dependencies (PostgreSQL, NATS, ClickHouse) are passed via Arc cloning
- Clean separation already exists at the domain layer with well-defined services

## Desired End State

After this refactoring, the codebase will have:
- Three distinct application modules (ponix_api, gateway_api, analytics_ingester) with clear setup functions
- Shared dependencies initialized once and passed to each module
- Each module returns its own runner processes
- Main.rs reduced to ~100 lines focused on dependency initialization and runner orchestration
- No functional changes - purely structural reorganization

### Verification:
- The service should start and run exactly as before
- All 5 processes should continue running concurrently
- gRPC API should work on port 50051
- Gateway orchestration should load existing gateways
- Envelope processing should continue working

## What We're NOT Doing

- Not creating separate binaries or crates
- Not changing any business logic or functionality
- Not modifying the domain layer or infrastructure layers
- Not changing configuration structure
- Not adding new features or capabilities
- Not splitting into actual microservices (that's a future step)

## Implementation Approach

We'll extract initialization logic into three application modules, each with a setup function that takes shared dependencies and returns runner processes. The main function will handle shared dependency initialization and runner orchestration only.

## Phase 1: Create Application Module Structure

### Overview
Create the three application modules with their setup functions and module exports.

### Changes Required:

#### 1. Create ponix_api Module
**File**: `crates/ponix-all-in-one/src/ponix_api.rs`
**Changes**: Create new file with module structure

```rust
use ponix_domain::{DeviceService, GatewayService, OrganizationService};
use ponix_grpc::{run_grpc_server, GrpcServerConfig};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct PonixApiConfig {
    pub grpc_host: String,
    pub grpc_port: u16,
}

pub struct PonixApi {
    device_service: Arc<DeviceService>,
    organization_service: Arc<OrganizationService>,
    gateway_service: Arc<GatewayService>,
    config: PonixApiConfig,
}

impl PonixApi {
    pub fn new(
        device_service: Arc<DeviceService>,
        organization_service: Arc<OrganizationService>,
        gateway_service: Arc<GatewayService>,
        config: PonixApiConfig,
    ) -> Self {
        info!("Initializing Ponix API module");
        Self {
            device_service,
            organization_service,
            gateway_service,
            config,
        }
    }

    pub fn into_runner_process(
        self,
    ) -> impl FnOnce(CancellationToken) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
    {
        move |ctx| {
            let grpc_config = GrpcServerConfig {
                host: self.config.grpc_host,
                port: self.config.grpc_port,
            };
            Box::pin(async move {
                run_grpc_server(
                    grpc_config,
                    self.device_service,
                    self.organization_service,
                    self.gateway_service,
                    ctx,
                )
                .await
            })
        }
    }
}
```

#### 2. Create gateway_api Module
**File**: `crates/ponix-all-in-one/src/gateway_api.rs`
**Changes**: Create new file with gateway orchestration logic

```rust
use ponix_domain::{
    GatewayOrchestrator, GatewayOrchestratorConfig, InMemoryGatewayProcessStore,
};
use ponix_nats::{GatewayCdcConsumer, NatsClient};
use ponix_postgres::{CdcConfig, CdcProcess, EntityConfig, GatewayConverter, PostgresGatewayRepository};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct GatewayApiConfig {
    pub cdc_config: CdcConfig,
    pub gateway_stream: String,
    pub gateway_consumer_name: String,
    pub gateway_filter_subject: String,
    pub gateway_entity_name: String,
    pub gateway_table_name: String,
}

pub struct GatewayApi {
    orchestrator: Arc<GatewayOrchestrator>,
    orchestrator_shutdown_token: CancellationToken,
    cdc_consumer: GatewayCdcConsumer,
    cdc_config: CdcConfig,
    entity_configs: Vec<EntityConfig>,
    nats_client: Arc<NatsClient>,
}

impl GatewayApi {
    pub async fn new(
        gateway_repository: Arc<PostgresGatewayRepository>,
        nats_client: Arc<NatsClient>,
        config: GatewayApiConfig,
    ) -> anyhow::Result<Self> {
        info!("Initializing Gateway API module");

        // Initialize orchestrator
        let orchestrator_config = GatewayOrchestratorConfig::default();
        let process_store = Arc::new(InMemoryGatewayProcessStore::new());
        let orchestrator_shutdown_token = CancellationToken::new();
        let orchestrator = Arc::new(GatewayOrchestrator::new(
            gateway_repository,
            process_store,
            orchestrator_config,
            orchestrator_shutdown_token.clone(),
        ));

        // Start orchestrator to load existing gateways
        orchestrator.start().await?;
        info!("Gateway orchestrator started");

        // Setup CDC consumer
        let cdc_consumer = GatewayCdcConsumer::new(
            Arc::clone(&nats_client),
            config.gateway_stream.clone(),
            config.gateway_consumer_name.clone(),
            config.gateway_filter_subject.clone(),
            Arc::clone(&orchestrator),
        );

        // Setup entity configs for CDC
        let gateway_entity_config = EntityConfig {
            entity_name: config.gateway_entity_name.clone(),
            table_name: config.gateway_table_name.clone(),
            converter: Box::new(GatewayConverter::new()),
        };
        let entity_configs = vec![gateway_entity_config];

        Ok(Self {
            orchestrator,
            orchestrator_shutdown_token,
            cdc_consumer,
            cdc_config: config.cdc_config,
            entity_configs,
            nats_client,
        })
    }

    pub fn into_runner_processes(
        self,
    ) -> Vec<
        Box<
            dyn FnOnce(CancellationToken) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
                + Send,
        >,
    > {
        vec![
            // CDC Consumer process
            Box::new({
                let cdc_consumer = self.cdc_consumer;
                move |ctx| Box::pin(async move { cdc_consumer.run(ctx).await })
            }),
            // CDC Process
            Box::new({
                let cdc_config = self.cdc_config;
                let nats_client = self.nats_client;
                let entity_configs = self.entity_configs;
                move |ctx| {
                    let cdc_process = CdcProcess::new(cdc_config, nats_client, entity_configs, ctx);
                    Box::pin(async move { cdc_process.run().await })
                }
            }),
        ]
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.orchestrator_shutdown_token.clone()
    }
}
```

#### 3. Create analytics_ingester Module
**File**: `crates/ponix-all-in-one/src/analytics_ingester.rs`
**Changes**: Create new file with envelope processing logic

```rust
use ponix_clickhouse::{ClickHouseClient, ClickHouseEnvelopeRepository};
use ponix_domain::{ProcessedEnvelopeService, RawEnvelopeService};
use ponix_nats::{
    create_domain_processor, create_raw_envelope_domain_processor, NatsClient, NatsConsumer,
    ProcessedEnvelopeProducer,
};
use ponix_payload::CelPayloadConverter;
use ponix_postgres::{PostgresDeviceRepository, PostgresOrganizationRepository};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub struct AnalyticsIngesterConfig {
    pub processed_envelopes_stream: String,
    pub processed_envelopes_subject: String,
    pub raw_stream: String,
    pub raw_subject: String,
    pub nats_batch_size: usize,
    pub nats_batch_wait_secs: u64,
}

pub struct AnalyticsIngester {
    processed_consumer: NatsConsumer,
    raw_consumer: NatsConsumer,
}

impl AnalyticsIngester {
    pub async fn new(
        device_repository: Arc<PostgresDeviceRepository>,
        organization_repository: Arc<PostgresOrganizationRepository>,
        clickhouse_client: ClickHouseClient,
        nats_client: Arc<NatsClient>,
        config: AnalyticsIngesterConfig,
    ) -> anyhow::Result<Self> {
        info!("Initializing Analytics Ingester module");

        // Initialize ProcessedEnvelope infrastructure
        let envelope_repository = ClickHouseEnvelopeRepository::new(
            clickhouse_client,
            "processed_envelopes".to_string(),
        );
        let envelope_service = Arc::new(ProcessedEnvelopeService::new(Arc::new(envelope_repository)));

        // Create processed envelope consumer
        let processor = create_domain_processor(envelope_service.clone());
        let consumer_client = nats_client.create_consumer_client();
        let processed_consumer = NatsConsumer::new(
            consumer_client,
            &config.processed_envelopes_stream,
            "ponix-all-in-one",
            &config.processed_envelopes_subject,
            config.nats_batch_size,
            config.nats_batch_wait_secs,
            processor,
        )
        .await?;

        // Initialize RawEnvelope infrastructure
        let publisher_client = nats_client.create_publisher_client();
        let payload_converter = Arc::new(CelPayloadConverter::new());
        let processed_envelope_producer = Arc::new(ProcessedEnvelopeProducer::new(
            publisher_client,
            config.processed_envelopes_stream.clone(),
        ));

        let raw_envelope_service = Arc::new(RawEnvelopeService::new(
            device_repository,
            organization_repository,
            payload_converter,
            processed_envelope_producer,
        ));

        // Create raw envelope consumer
        let raw_processor = create_raw_envelope_domain_processor(raw_envelope_service);
        let raw_consumer_client = nats_client.create_consumer_client();
        let raw_consumer = NatsConsumer::new(
            raw_consumer_client,
            &config.raw_stream,
            "ponix-all-in-one-raw",
            &config.raw_subject,
            config.nats_batch_size,
            config.nats_batch_wait_secs,
            raw_processor,
        )
        .await?;

        info!("Analytics Ingester initialized");

        Ok(Self {
            processed_consumer,
            raw_consumer,
        })
    }

    pub fn into_runner_processes(
        self,
    ) -> Vec<
        Box<
            dyn FnOnce(CancellationToken) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
                + Send,
        >,
    > {
        vec![
            // Processed envelope consumer
            Box::new({
                let consumer = self.processed_consumer;
                move |ctx| Box::pin(async move { consumer.run(ctx).await })
            }),
            // Raw envelope consumer
            Box::new({
                let consumer = self.raw_consumer;
                move |ctx| Box::pin(async move { consumer.run(ctx).await })
            }),
        ]
    }
}
```

#### 4. Update lib.rs
**File**: `crates/ponix-all-in-one/src/lib.rs`
**Changes**: Create new file to export modules

```rust
pub mod analytics_ingester;
pub mod config;
pub mod gateway_api;
pub mod ponix_api;
```

### Success Criteria:

#### Automated Verification:
- [x] Code compiles successfully: `cargo build -p ponix-all-in-one`
- [x] No clippy warnings: `cargo clippy -p ponix-all-in-one`

#### Manual Verification:
- [x] Module files are created with proper structure
- [x] Each module has clear boundaries and responsibilities

---

## Phase 2: Refactor main.rs to Use Application Modules

### Overview
Refactor main.rs to initialize shared dependencies once and delegate to application modules for service-specific setup.

### Changes Required:

#### 1. Update main.rs imports
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Replace current imports and add module imports

```rust
mod analytics_ingester;
mod config;
mod gateway_api;
mod ponix_api;

use analytics_ingester::{AnalyticsIngester, AnalyticsIngesterConfig};
use config::ServiceConfig;
use gateway_api::{GatewayApi, GatewayApiConfig};
use ponix_api::{PonixApi, PonixApiConfig};

use ponix_clickhouse::ClickHouseClient;
use ponix_domain::{DeviceService, GatewayService, OrganizationService};
use ponix_nats::NatsClient;
use ponix_postgres::{
    CdcConfig, PostgresClient, PostgresDeviceRepository, PostgresGatewayRepository,
    PostgresOrganizationRepository,
};
use ponix_runner::Runner;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
```

#### 2. Refactor main function
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Simplify main to focus on shared dependency initialization and module setup

```rust
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

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    info!("Starting ponix-all-in-one service");
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

    // Initialize application modules
    let ponix_api = PonixApi::new(
        device_service.clone(),
        organization_service,
        gateway_service,
        PonixApiConfig {
            grpc_host: config.grpc_host.clone(),
            grpc_port: config.grpc_port,
        },
    );

    let gateway_api = match GatewayApi::new(
        postgres_repos.gateway.clone(),
        nats_client.clone(),
        GatewayApiConfig {
            cdc_config: build_cdc_config(&config),
            gateway_stream: config.nats_gateway_stream.clone(),
            gateway_consumer_name: config.gateway_consumer_name.clone(),
            gateway_filter_subject: config.gateway_filter_subject.clone(),
            gateway_entity_name: config.cdc_gateway_entity_name.clone(),
            gateway_table_name: config.cdc_gateway_table_name.clone(),
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

    let analytics_ingester = match AnalyticsIngester::new(
        postgres_repos.device.clone(),
        postgres_repos.organization.clone(),
        clickhouse_client,
        nats_client.clone(),
        AnalyticsIngesterConfig {
            processed_envelopes_stream: config.processed_envelopes_stream.clone(),
            processed_envelopes_subject: config.processed_envelopes_subject.clone(),
            raw_stream: config.nats_raw_stream.clone(),
            raw_subject: config.nats_raw_subject.clone(),
            nats_batch_size: config.nats_batch_size,
            nats_batch_wait_secs: config.nats_batch_wait_secs,
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
    runner = runner.with_app_process(ponix_api.into_runner_process());

    // Add Gateway API processes
    for process in gateway_api.into_runner_processes() {
        runner = runner.with_app_process(process);
    }

    // Add Analytics Ingester processes
    for process in analytics_ingester.into_runner_processes() {
        runner = runner.with_app_process(process);
    }

    // Add cleanup handlers
    runner = runner
        .with_closer({
            let nats_for_close = Arc::clone(&nats_client);
            let orchestrator_token = gateway_api.shutdown_token();
            move || {
                Box::pin(async move {
                    info!("Running cleanup tasks...");
                    orchestrator_token.cancel();
                    if let Ok(client) = Arc::try_unwrap(nats_for_close) {
                        client.close().await;
                    }
                    info!("Cleanup complete");
                    Ok(())
                })
            }
        })
        .with_closer_timeout(Duration::from_secs(10));

    // Run the service
    runner.run().await;
}
```

#### 3. Add helper functions
**File**: `crates/ponix-all-in-one/src/main.rs`
**Changes**: Add helper functions for shared dependency initialization

```rust
struct PostgresRepositories {
    device: Arc<PostgresDeviceRepository>,
    organization: Arc<PostgresOrganizationRepository>,
    gateway: Arc<PostgresGatewayRepository>,
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
        gateway: Arc::new(PostgresGatewayRepository::new(postgres_client)),
    };

    // ClickHouse initialization
    info!("Initializing ClickHouse...");
    run_clickhouse_migrations(config).await?;
    let clickhouse_client = create_clickhouse_client(config).await?;

    // NATS initialization
    info!("Initializing NATS...");
    let nats_client = Arc::new(
        NatsClient::connect(&config.nats_url, Duration::from_secs(config.startup_timeout_secs))
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
    let runner = ponix_postgres::MigrationRunner::new(
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
    let runner = ponix_clickhouse::MigrationRunner::new(
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
    client.ensure_stream(&config.processed_envelopes_stream).await?;
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
```

### Success Criteria:

#### Automated Verification:
- [x] Service compiles: `cargo build -p ponix-all-in-one`
- [x] All tests pass: `cargo test -p ponix-all-in-one`
- [x] No clippy warnings: `cargo clippy -p ponix-all-in-one`
- [ ] Service starts successfully: `cargo run -p ponix-all-in-one`

#### Manual Verification:
- [ ] gRPC API responds on port 50051
- [ ] Gateway orchestrator loads existing gateways
- [ ] Envelope processing continues working
- [ ] All 5 processes run concurrently
- [ ] Graceful shutdown works correctly

---

## Phase 3: Update Cargo.toml and Clean Up

### Overview
Update the Cargo.toml to properly declare the library modules and clean up any unused code.

### Changes Required:

#### 1. Update Cargo.toml
**File**: `crates/ponix-all-in-one/Cargo.toml`
**Changes**: Add lib section

```toml
[lib]
name = "ponix_all_in_one"
path = "src/lib.rs"

[[bin]]
name = "ponix-all-in-one"
path = "src/main.rs"
```

### Success Criteria:

#### Automated Verification:
- [x] Full build succeeds: `cargo build --release`
- [x] All workspace tests pass: `cargo test --workspace`
- [ ] Integration tests pass: `cargo test --workspace --features integration-tests`

#### Manual Verification:
- [ ] Service behavior unchanged from before refactoring
- [ ] Code is more modular and easier to understand
- [ ] Each module has clear responsibilities

---

## Testing Strategy

### Unit Tests:
- Test each application module's initialization independently
- Mock shared dependencies to test module boundaries
- Verify process creation returns expected closures

### Integration Tests:
- Full end-to-end test with all modules running
- Verify all processes start and communicate correctly
- Test graceful shutdown of all modules

### Manual Testing Steps:
1. Start the service with `docker-compose -f docker/docker-compose.deps.yaml up -d && cargo run -p ponix-all-in-one`
2. Create a device via gRPC: `grpcurl -plaintext -d '{"organization_id": "org-001", "name": "Test Device"}' localhost:50051 ponix.end_device.v1.EndDeviceService/CreateEndDevice`
3. Verify gateway orchestrator loads gateways from database
4. Send a raw envelope and verify it gets processed to ClickHouse
5. Shutdown with Ctrl+C and verify graceful cleanup

## Performance Considerations

This refactoring should have no performance impact as it's purely structural. All the same processes run with the same concurrency model. The only difference is better code organization.

## Migration Notes

No migration needed as this is a refactoring with no functional changes. The service can be deployed as a drop-in replacement.

## References

- Original issue: #51 on GitHub
- Current implementation: [main.rs:1-385](crates/ponix-all-in-one/src/main.rs#L1-L385)
- Runner pattern: [runner/src/lib.rs](crates/runner/src/lib.rs)
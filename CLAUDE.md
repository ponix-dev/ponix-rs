# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Ponix RS is a Rust-based IoT data ingestion platform with gRPC device management, NATS JetStream event processing, PostgreSQL Change Data Capture (CDC), and multi-database persistence (PostgreSQL + ClickHouse). It's a Cargo workspace with modular crates designed for future microservice extraction, currently deployed as a monolith.

## Development Commands

### Building
```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build specific crate
cargo build -p ponix_all_in_one
```

### Testing
```bash
# Unit tests only (fast, no Docker required)
mise run test:unit
# OR: cargo test --workspace --lib --bins

# Integration tests (requires Docker for testcontainers)
mise run test:integration
# OR: cargo test --workspace --features integration-tests -- --test-threads=1

# All tests
mise run test:all
# OR: cargo test --workspace --features integration-tests

# Watch mode for unit tests
mise run test:watch

# Watch mode for integration tests
mise run test:integration:watch
```

### Running Locally
```bash
# Start infrastructure (NATS + ClickHouse + PostgreSQL)
docker-compose -f docker/docker-compose.deps.yaml up -d

# Run with Tilt (recommended for development)
tilt up

# Or with Docker Compose
docker-compose -f docker/docker-compose.service.yaml up --build
```

### Database Access
```bash
# Connect to ClickHouse
docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix

# Query envelopes
SELECT * FROM ponix.processed_envelopes LIMIT 10;

# Connect to PostgreSQL
docker exec -it ponix-postgres psql -U ponix -d ponix

# Query devices
SELECT * FROM devices;
```

### gRPC API Testing
```bash
# Install grpcurl
brew install grpcurl

# Create a device
grpcurl -plaintext -d '{"organization_id": "org-001", "name": "Sensor Alpha"}' \
  localhost:50051 ponix.end_device.v1.EndDeviceService/CreateEndDevice

# Get a device
grpcurl -plaintext -d '{"device_id": "cm3rnbgek0000j5d37r63jnmn"}' \
  localhost:50051 ponix.end_device.v1.EndDeviceService/GetEndDevice

# List devices
grpcurl -plaintext -d '{"organization_id": "org-001"}' \
  localhost:50051 ponix.end_device.v1.EndDeviceService/ListEndDevices
```

## Architecture

### Workspace Structure
This is a multi-crate Cargo workspace with modular workers designed for future microservice extraction:

- **`runner`**: Core concurrency framework for managing long-running processes
  - Orchestrates multiple concurrent processes with graceful shutdown
  - Handles SIGTERM/SIGINT signals for graceful termination
  - Manages cleanup functions (closers) with configurable timeouts (default 10s)
  - Uses `CancellationToken` for coordinated shutdown across processes
  - All app processes run concurrently; if any fails, all are cancelled

- **`common`**: Shared infrastructure and domain types
  - **`domain/`**: Core domain entities and repository traits
    - Entities: `Device`, `Organization`, `Gateway`, `RawEnvelope`, `ProcessedEnvelope`
    - Repository traits: `DeviceRepository`, `OrganizationRepository`, `GatewayRepository`
    - Producer traits: `ProcessedEnvelopeProducer`, `RawEnvelopeProducer`
  - **`postgres/`**: PostgreSQL integration
    - `PostgresClient`: Connection pooling (deadpool-postgres, max 5 connections)
    - Repository implementations for devices, organizations, gateways
  - **`nats/`**: NATS JetStream integration
    - `NatsClient`, `NatsConsumer` with batch processing
    - `ProcessingResult` for per-message Ack/Nak decisions
  - **`clickhouse/`**: ClickHouse client and configuration
  - **`proto/`**: Bidirectional domain ↔ protobuf conversions
  - **`grpc/`**: Domain error → gRPC Status mapping
  - Feature flags: `testing` (mockall mocks), `integration-tests`

- **`ponix_api`**: gRPC API layer and domain services
  - **`domain/`**: Business logic services
    - `DeviceService`: Device CRUD with organization validation
    - `OrganizationService`: Organization management
    - `GatewayService`: Gateway management
  - **`grpc/`**: Tonic gRPC handlers and server
  - `PonixApi`: Aggregates services, converts to `Runner` process

- **`gateway_orchestrator`**: Gateway CDC orchestration
  - `GatewayOrchestrator`: Manages gateway configuration state
  - `GatewayOrchestrationService`: In-memory state machine for gateways
  - `GatewayCdcConsumer`: Consumes gateway CDC events from NATS
  - Loads existing gateways from PostgreSQL on startup

- **`cdc_worker`**: PostgreSQL Change Data Capture worker
  - `CdcWorker`: Captures PostgreSQL changes via logical decoding
  - Uses `etl` and `etl-postgres` (Supabase) for change capture
  - Publishes changes to NATS as protobuf messages
  - Configurable batch size, timeout, and retry logic

- **`analytics_worker`**: Envelope processing and storage
  - `AnalyticsWorker`: Coordinates envelope consumers
  - `ProcessedEnvelopeService`: Stores processed envelopes in ClickHouse
  - `RawEnvelopeService`: Converts raw → processed envelopes
    - Device lookup, organization validation
    - CEL expression execution on binary payloads
    - Republishes to processed envelopes stream
  - `CelPayloadConverter`: CEL-based payload transformation
  - `ClickHouseEnvelopeRepository`: Batch writes to ClickHouse

- **`goose`**: Migration runner wrapper
  - `MigrationRunner`: Spawns goose binary as subprocess
  - Supports PostgreSQL, ClickHouse, MySQL, SQLite

- **`ponix_all_in_one`**: Main service binary (monolith)
  - Orchestrates all modules via `Runner`
  - Runs migrations on startup
  - Initializes all infrastructure clients
  - Registers all workers as concurrent app processes

### Key Architectural Patterns

#### Domain-Driven Design (DDD)
The system follows DDD principles with clear layer separation:
- **Domain Layer** (`common/src/domain`): Business logic, domain types, repository traits
- **Infrastructure Layer** (`common/src/postgres`, `common/src/nats`, `common/src/clickhouse`): Trait implementations
- **Service Layer** (`ponix_api/src/domain`, `analytics_worker`): Business orchestration
- **API Layer** (`ponix_api/src/grpc`): Protocol translation

Key patterns:
- **Repository Pattern**: Traits in domain, implementations in infrastructure
- **Dependency Inversion**: Domain defines interfaces, infrastructure implements
- **Two-Type Pattern**: `CreateDeviceInput` (external) vs `CreateDeviceInputWithId` (internal)
- **ID Generation**: Domain service generates unique IDs using `xid`
- **Error Mapping**: Domain errors mapped to gRPC Status codes

#### Runner Pattern for Process Orchestration
All workers implement a standard process pattern:

```rust
impl MyWorker {
    pub fn into_runner_process(self) -> AppProcess {
        Box::new(|cancellation_token| {
            Box::pin(async move {
                tokio::select! {
                    _ = cancellation_token.cancelled() => Ok(())
                    result = self.run() => result
                }
            })
        })
    }
}
```

In `ponix_all_in_one`:
```rust
let runner = Runner::new()
    .with_app_process(ponix_api.into_runner_process())
    .with_app_process(gateway_orchestrator.into_runner_process())
    .with_app_process(cdc_worker.into_runner_process())
    .with_app_process(analytics_worker.into_runner_processes()) // Returns multiple
    .with_closer(cleanup_fn);
runner.run().await;
```

#### Raw Envelope Conversion Flow
```
RawEnvelope (binary payload)
    ↓
RawEnvelopeService.process_raw_envelope()
    ↓
├─> DeviceRepository: Fetch device with CEL expression
├─> Validate organization exists
├─> CelPayloadConverter: Execute CEL on binary data
├─> Validate output is JSON object
└─> ProcessedEnvelopeProducer: Publish to NATS
    ↓
ProcessedEnvelopeService (consumer)
    ↓
ClickHouseEnvelopeRepository: Batch insert
```

#### CDC Stream Architecture
```
PostgreSQL (wal_level=logical)
    ↓ (publication + replication slot)
CdcWorker (etl-postgres)
    ↓ extracts changes
NATS JetStream (gateways stream)
    ↓ CDC events as protobuf
GatewayOrchestrator (CDC consumer)
    ↓ updates in-memory state
GatewayOrchestrationService
```

### Rust Module Conventions

This project follows specific module organization patterns:

#### Module Path Syntax
Use the modern module path syntax (Rust 2018+) instead of `mod.rs`:
```
# Preferred
src/
├── lib.rs
├── domain.rs        # Contains mod declarations and re-exports
└── domain/
    ├── foo.rs
    └── bar.rs

# Avoid
src/
├── lib.rs
└── domain/
    ├── mod.rs       # Don't use mod.rs
    ├── foo.rs
    └── bar.rs
```

#### Module Visibility Pattern
Keep submodules private and re-export public items at the parent level:
```rust
// In domain.rs (or any module aggregator)
mod foo;           // Private module declaration
mod bar;

pub use foo::*;    // Re-export public items
pub use bar::*;
```

This pattern:
- Keeps internal module structure private
- Exposes items at the parent module level (e.g., `crate::domain::FooStruct`)
- Allows refactoring internal structure without breaking external imports

#### Crate-Level Exports
In `lib.rs`, use `pub mod` to expose modules with full paths:
```rust
// In lib.rs
pub mod domain;
pub mod grpc;
pub mod nats;
```

External consumers use namespaced imports: `my_crate::domain::FooStruct`

### Protobuf Dependencies

This project uses the Buf Schema Registry (BSR) for protobuf types:
- Protobuf types vendored via BSR at build time
- Registry: `buf` (configured in Cargo auth)
- Packages: `ponix-proto-prost` (common), `ponix-proto-tonic` (ponix_api)
- **Authentication required**: Must run `cargo login --registry buf "Bearer $BSR_TOKEN"` once per machine
- BSR_TOKEN should be set in `.env` and loaded via `.mise.toml`

### Service Lifecycle Phases

The `ponix_all_in_one` service initializes in phases:
1. **Configuration**: Load `ServiceConfig` from environment variables
2. **PostgreSQL**: Run migrations → create client → initialize repositories
3. **ClickHouse**: Run migrations → create client
4. **NATS**: Connect → ensure streams exist
5. **Domain Services**: Create DeviceService, OrganizationService, GatewayService
6. **Workers**: Initialize PonixApi, GatewayOrchestrator, CdcWorker, AnalyticsWorker
7. **Runner**: Register all as app processes, register closers
8. **Run**: Start all processes concurrently
9. **Shutdown**: On signal, cancel processes and run closers

## Common Development Tasks

### Adding a New Crate
1. Create directory under `crates/`
2. Add to workspace members in root `Cargo.toml`
3. Use workspace dependencies: `tokio.workspace = true`

### Adding Database Migrations

**PostgreSQL migrations:**
1. Create new SQL file in `crates/init_process/migrations/postgres/`
2. Use naming convention: `{timestamp}_{description}.sql`
3. Migrations run automatically on service startup via `MigrationRunner`
4. Requires `goose` binary: `brew install goose`

**ClickHouse migrations:**
1. Create new SQL file in `crates/init_process/migrations/clickhouse/`
2. Use naming convention: `{timestamp}_{description}.sql`
3. Migrations run automatically on service startup via `MigrationRunner`
4. Requires `goose` binary: `brew install goose`

### Adding a New Worker
1. Create new crate under `crates/`
2. Implement `into_runner_process()` method returning `AppProcess`
3. Add to `ponix_all_in_one` initialization
4. Register with `Runner.with_app_process()`

### Working with Integration Tests
- Integration tests require Docker (uses testcontainers)
- Must use `--features integration-tests` flag
- Run with `--test-threads=1` to avoid container conflicts
- Tests automatically start required containers
- See `crates/common/Cargo.toml` for feature flag pattern

### Environment Configuration
Service configuration loaded from environment variables with `PONIX_` prefix:

```bash
# Logging
PONIX_LOG_LEVEL=info

# NATS
PONIX_NATS_URL=nats://localhost:4222
PONIX_PROCESSED_ENVELOPES_STREAM=processed_envelopes
PONIX_NATS_RAW_STREAM=raw_envelopes
PONIX_NATS_GATEWAY_STREAM=gateways
PONIX_NATS_BATCH_SIZE=30
PONIX_NATS_BATCH_WAIT_SECS=5

# ClickHouse
PONIX_CLICKHOUSE_URL=http://localhost:8123
PONIX_CLICKHOUSE_NATIVE_URL=localhost:9000
PONIX_CLICKHOUSE_DATABASE=ponix
PONIX_CLICKHOUSE_USERNAME=ponix
PONIX_CLICKHOUSE_PASSWORD=ponix
PONIX_CLICKHOUSE_MIGRATIONS_DIR=./migrations/clickhouse
PONIX_CLICKHOUSE_GOOSE_BINARY_PATH=/usr/local/bin/goose

# PostgreSQL
PONIX_POSTGRES_HOST=localhost
PONIX_POSTGRES_PORT=5432
PONIX_POSTGRES_DATABASE=ponix
PONIX_POSTGRES_USERNAME=ponix
PONIX_POSTGRES_PASSWORD=ponix
PONIX_POSTGRES_MIGRATIONS_DIR=./migrations/postgres
PONIX_POSTGRES_GOOSE_BINARY_PATH=/usr/local/bin/goose

# gRPC
PONIX_GRPC_HOST=0.0.0.0
PONIX_GRPC_PORT=50051

# CDC
PONIX_CDC_PUBLICATION_NAME=ponix_cdc_publication
PONIX_CDC_SLOT_NAME=ponix_cdc_slot
PONIX_CDC_BATCH_SIZE=100
PONIX_CDC_BATCH_TIMEOUT_MS=5000
PONIX_CDC_RETRY_DELAY_MS=10000
PONIX_CDC_MAX_RETRY_ATTEMPTS=5
```

See `crates/ponix_all_in_one/src/config.rs` for full config schema.

### Working with the gRPC API
- Service definitions use Buf Schema Registry for protobuf types
- Import paths for tonic-generated code have nested `tonic` module:
  ```rust
  use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceService;
  ```
- Domain service generates device IDs using `xid` crate (not at gRPC layer)
- Error mapping: `DomainError::DeviceAlreadyExists` → `Status::already_exists`

## Tilt Development

The project uses Tilt for local development:
- Watches for file changes and rebuilds Docker images
- Manages Docker Compose services
- Requires BSR_TOKEN environment variable for protobuf authentication
- Uses secret mounting to pass BSR token to Docker build
- Access: `tilt up` then open Tilt UI in browser

## Dependency Graph

```
ponix_all_in_one
    ├── runner
    ├── common
    ├── ponix_api ────────→ common
    ├── gateway_orchestrator → common
    ├── cdc_worker ───────→ common
    ├── analytics_worker ──→ common
    └── goose

common (no workspace deps)
    └── External: async-nats, tokio-postgres, clickhouse, ponix-proto-prost

runner (no workspace deps)
    └── External: tokio, tokio-util, tracing
```

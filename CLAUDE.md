# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Ponix RS is a Rust-based IoT data ingestion platform with gRPC device management, NATS JetStream event processing, and multi-database persistence (PostgreSQL + ClickHouse). It's a Cargo workspace with multiple crates implementing domain-driven design principles for device management and event processing.

## Development Commands

### Building
```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build specific crate
cargo build -p ponix-all-in-one
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
This is a multi-crate Cargo workspace with the following crates:

- **`runner`**: Core concurrency framework for managing long-running processes
  - Orchestrates multiple concurrent processes with graceful shutdown
  - Handles SIGTERM/SIGINT signals for graceful termination
  - Manages cleanup functions (closers) with configurable timeouts
  - Uses `CancellationToken` for coordinated shutdown across processes
  - All app processes run concurrently; if any fails, all are cancelled

- **`ponix-domain`**: Domain layer (business logic)
  - Core domain types: `Device`, `RawEnvelope`, `ProcessedEnvelope`, and input types
  - Business logic services:
    - `DeviceService`: Device management with input validation and ID generation
    - `EnvelopeService`: Raw → Processed envelope conversion with CEL transformations
  - Repository traits: `DeviceRepository`, `ProcessedEnvelopeProducer`
  - Converter traits: `PayloadConverter` for CEL-based payload transformation
  - Domain errors: `DomainError` enum including `PayloadConversionError`, `MissingCelExpression`
  - Uses `xid` crate for generating unique device IDs
  - Comprehensive unit and integration tests with Mockall mocks

- **`ponix-grpc`**: gRPC API layer
  - Service handlers implementing Tonic-generated traits
  - Type conversions between protobuf and domain types
  - Error mapping: Domain errors → gRPC Status codes
  - Feature flag `device` (enabled by default) for device management endpoints
  - Uses Buf Schema Registry for protobuf definitions
  - Graceful shutdown support via CancellationToken

- **`ponix-postgres`**: PostgreSQL repository implementation
  - Implements `DeviceRepository` trait from domain layer
  - Type conversions between database models and domain types
  - PostgreSQL-specific error handling (e.g., error code 23505 for unique violations)
  - Migration runner using goose
  - SQL migrations stored in `crates/ponix-postgres/migrations/`
  - Integration tests with Testcontainers

- **`ponix-nats`**: NATS JetStream client library
  - Generic batch consumer with fine-grained Ack/Nak control per message
  - Stream management and JetStream context access
  - Feature flag `processed-envelope` for ProcessedEnvelope-specific functionality:
    - `ProcessedEnvelopeProducer`: Implements domain trait for publishing envelopes
    - Domain ↔ Protobuf conversions for seamless type translation
    - `run_demo_producer()`: Reusable demo producer for testing
  - `ProcessingResult` type allows individual message Ack/Nak decisions
  - Supports both generic message processing and protobuf-based workflows

- **`ponix-payload`**: CEL-based payload conversion
  - `CelPayloadConverter`: Implements `PayloadConverter` trait from domain layer
  - Executes CEL expressions on binary payloads to produce JSON
  - Built-in Cayenne LPP decoder and base64 encoding functions
  - Comprehensive test coverage for CEL transformations

- **`ponix-clickhouse`**: ClickHouse client and migrations
  - Async batch writer for ProcessedEnvelope messages
  - Migration runner using goose (external binary dependency)
  - SQL migrations stored in `crates/ponix-clickhouse/migrations/`
  - Feature flag `integration-tests` for testcontainer-based tests

- **`ponix-all-in-one`**: Main service binary
  - Combines gRPC server, producer, and consumer in a single service
  - Demonstrates the runner pattern with multiple concurrent processes
  - gRPC server for device management on port 50051
  - Producer publishes sample ProcessedEnvelope messages to NATS
  - Consumer reads from NATS and writes to ClickHouse in batches
  - Configuration loaded from environment variables

### Key Architectural Patterns

#### Domain-Driven Design (DDD)
The system follows DDD principles with clear layer separation:
- **Domain Layer** (`ponix-domain`): Business logic, domain types, and repository/converter traits
- **Infrastructure Layer** (`ponix-postgres`, `ponix-nats`, `ponix-clickhouse`): Trait implementations
- **Payload Layer** (`ponix-payload`): CEL-based payload transformation logic
- **API Layer** (`ponix-grpc`): Protocol translation between gRPC and domain types

Key patterns:
- **Repository Pattern**: `DeviceRepository` and `ProcessedEnvelopeProducer` traits with infrastructure implementations
- **Strategy Pattern**: `PayloadConverter` trait allows different payload conversion strategies
- **Dependency Inversion**: Domain layer defines interfaces, infrastructure implements them
- **Service Orchestration**: `EnvelopeService` coordinates device lookup, conversion, and publishing
- **ID Generation**: Domain service generates unique IDs using `xid` (not at API layer)
- **Two-Type Pattern**: `CreateDeviceInput` (external) vs `CreateDeviceInputWithId` (internal)
- **Error Mapping**: Domain errors (`DomainError`) mapped to gRPC Status codes

#### Raw Envelope Conversion Flow
```
RawEnvelope (binary payload)
    ↓
EnvelopeService.process_raw_envelope()
    ↓
├─> DeviceRepository: Fetch device with CEL expression
├─> Validate CEL expression exists
├─> PayloadConverter: Execute CEL transformation on binary data
├─> Validate output is a JSON object
└─> ProcessedEnvelopeProducer: Publish to NATS
    ↓
ProcessedEnvelope (structured JSON) → ClickHouse
```

This flow demonstrates:
- **Separation of Concerns**: Each step has a single responsibility
- **Testability**: Each component can be mocked independently
- **Error Handling**: Clear error variants for each failure scenario

#### Runner Pattern
The `ponix-runner` crate implements a process lifecycle management pattern:
- App processes are registered with `.with_app_process()` and run concurrently
- Closers are registered with `.with_closer()` for cleanup tasks
- Runner handles signal propagation via `CancellationToken`
- Cleanup always runs regardless of process outcome
- See [lib.rs](crates/runner/src/lib.rs) for the full implementation

#### NATS Consumer Pattern
The `ponix-nats` consumer uses a batch processing model:
- Consumers pull batches of messages (configurable size and wait time)
- Each message can be individually Ack'd or Nak'd via `ProcessingResult`
- Processors are `Box<dyn Fn(&[Message]) -> BoxFuture<Result<ProcessingResult>>>`
- This allows fine-grained error handling per message in a batch

#### ClickHouse Batch Writing
The ClickHouse integration uses an async batch writer:
- Messages are collected in batches before insertion
- Uses ClickHouse's native `inserter` feature for efficient bulk inserts
- Protobuf Struct types are serialized to JSON for storage
- See [main.rs:145-148](crates/ponix-all-in-one/src/main.rs#L145-L148) for processor integration

### Protobuf Dependencies

This project uses the Buf Schema Registry (BSR) for protobuf types:
- Protobuf types are vendored via BSR at build time
- Registry: `buf` (configured in Cargo auth)
- Package: `ponix_ponix_community_neoeinstein-prost`
- **Authentication required**: Must run `cargo login --registry buf "Bearer $BSR_TOKEN"` once per machine
- BSR_TOKEN should be set in `.env` and loaded via `.mise.toml`

### Service Lifecycle Phases

The `ponix-all-in-one` service initializes in phases:
1. **PostgreSQL Migrations**: Run PostgreSQL schema migrations via goose
2. **PostgreSQL Client**: Initialize PostgreSQL client and device repository
3. **Domain Service**: Create domain service with repository dependency
4. **ClickHouse Migrations**: Run ClickHouse schema migrations via goose
5. **ClickHouse Client**: Connect and ping ClickHouse
6. **NATS**: Connect to NATS and ensure stream exists
7. **Processes**: Start three concurrent processes via Runner:
   - Producer (publishes sample data to NATS)
   - Consumer (reads from NATS, writes to ClickHouse)
   - gRPC Server (device management API)
8. **Shutdown**: On signal, cancel processes and run closers (cleanup)

See [main.rs](crates/ponix-all-in-one/src/main.rs) for the full initialization flow.

## Common Development Tasks

### Adding a New Crate
1. Create directory under `crates/`
2. Add to workspace members in root `Cargo.toml`
3. Use workspace dependencies: `tokio.workspace = true`

### Adding Database Migrations

**PostgreSQL migrations:**
1. Create new SQL file in `crates/ponix-postgres/migrations/`
2. Use naming convention: `{timestamp}_{description}.sql`
3. Migrations run automatically on service startup via `MigrationRunner`
4. Requires `goose` binary: `brew install goose`

**ClickHouse migrations:**
1. Create new SQL file in `crates/ponix-clickhouse/migrations/`
2. Use naming convention: `{timestamp}_{description}.sql`
3. Migrations run automatically on service startup via `MigrationRunner`
4. Requires `goose` binary: `brew install goose`

### Working with Integration Tests
- Integration tests require Docker (uses testcontainers)
- Must use `--features integration-tests` flag
- Run with `--test-threads=1` to avoid container conflicts
- Tests automatically start NATS/ClickHouse containers
- See `crates/ponix-clickhouse/Cargo.toml` for feature flag pattern

### Environment Configuration
- Service configuration loaded from environment variables with `PONIX_` prefix
- Use `.env` file (loaded via `.mise.toml`)
- Required vars: `NATS_URL`, `CLICKHOUSE_URL`, `POSTGRES_HOST`, `BSR_TOKEN`
- gRPC configuration: `PONIX_GRPC_HOST` (default: `0.0.0.0`), `PONIX_GRPC_PORT` (default: `50051`)
- See `crates/ponix-all-in-one/src/config.rs` for full config schema

### Working with the gRPC API
- Service definitions use Buf Schema Registry for protobuf types
- Import paths for tonic-generated code have nested `tonic` module:
  ```rust
  use ponix_proto_tonic::end_device::v1::tonic::end_device_service_server::EndDeviceService;
  ```
- Domain service generates device IDs using `xid` crate (not at gRPC layer)
- Error mapping: `DomainError::DeviceAlreadyExists` → `Status::already_exists`
- Feature flag `device` is enabled by default for rust-analyzer support

## Tilt Development

The project uses Tilt for local development:
- Watches for file changes and rebuilds Docker images
- Manages Docker Compose services
- Requires BSR_TOKEN environment variable for protobuf authentication
- Uses secret mounting to pass BSR token to Docker build
- Access: `tilt up` then open Tilt UI in browser

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Ponix RS is a Rust-based event processing service demonstrating NATS JetStream and ClickHouse integration with graceful lifecycle management. It's a Cargo workspace with multiple crates that work together to publish, consume, and store ProcessedEnvelope messages.

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
# Start infrastructure (NATS + ClickHouse)
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

- **`ponix-nats`**: NATS JetStream client library
  - Generic batch consumer with fine-grained Ack/Nak control per message
  - Stream management and JetStream context access
  - Feature flag `processed-envelope` for ProcessedEnvelope-specific functionality
  - `ProcessingResult` type allows individual message Ack/Nak decisions
  - Supports both generic message processing and protobuf-based workflows

- **`ponix-clickhouse`**: ClickHouse client and migrations
  - Async batch writer for ProcessedEnvelope messages
  - Migration runner using goose (external binary dependency)
  - SQL migrations stored in `crates/ponix-clickhouse/migrations/`
  - Feature flag `integration-tests` for testcontainer-based tests

- **`ponix-all-in-one`**: Main service binary
  - Combines producer and consumer in a single service
  - Demonstrates the runner pattern with multiple concurrent processes
  - Producer publishes sample ProcessedEnvelope messages to NATS
  - Consumer reads from NATS and writes to ClickHouse in batches
  - Configuration loaded from environment variables

### Key Architectural Patterns

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
- See [main.rs:90-94](crates/ponix-all-in-one/src/main.rs#L90-L94) for processor integration

### Protobuf Dependencies

This project uses the Buf Schema Registry (BSR) for protobuf types:
- Protobuf types are vendored via BSR at build time
- Registry: `buf` (configured in Cargo auth)
- Package: `ponix_ponix_community_neoeinstein-prost`
- **Authentication required**: Must run `cargo login --registry buf "Bearer $BSR_TOKEN"` once per machine
- BSR_TOKEN should be set in `.env` and loaded via `.mise.toml`

### Service Lifecycle Phases

The `ponix-all-in-one` service initializes in phases:
1. **Migrations**: Run ClickHouse schema migrations via goose
2. **ClickHouse**: Connect and ping ClickHouse
3. **NATS**: Connect to NATS and ensure stream exists
4. **Processes**: Start producer and consumer concurrently via Runner
5. **Shutdown**: On signal, cancel processes and run closers (cleanup)

See [main.rs](crates/ponix-all-in-one/src/main.rs) for the full initialization flow.

## Common Development Tasks

### Adding a New Crate
1. Create directory under `crates/`
2. Add to workspace members in root `Cargo.toml`
3. Use workspace dependencies: `tokio.workspace = true`

### Adding Database Migrations
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
- Service configuration loaded from environment variables
- Use `.env` file (loaded via `.mise.toml`)
- Required vars: `NATS_URL`, `CLICKHOUSE_URL`, `BSR_TOKEN`
- See `crates/ponix-all-in-one/src/config.rs` for full config schema

## Tilt Development

The project uses Tilt for local development:
- Watches for file changes and rebuilds Docker images
- Manages Docker Compose services
- Requires BSR_TOKEN environment variable for protobuf authentication
- Uses secret mounting to pass BSR token to Docker build
- Access: `tilt up` then open Tilt UI in browser

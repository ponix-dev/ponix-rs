# ponix-rs

A Rust-based IoT data ingestion platform

## Overview

ponix-rs is an IoT data ingestion platform designed to handle device telemetry at scale. The `ponix-all-in-one` service provides:
- **Device Management**: Register and track IoT devices with PostgreSQL
- **Event Ingestion**: Process ProcessedEnvelope messages via NATS JetStream
- **Analytics Storage**: Batch write telemetry data to ClickHouse for time-series analysis
- **Graceful Lifecycle**: Coordinated startup/shutdown with proper resource cleanup

**Key Features:**
- Multi-architecture Docker support (ARM64/AMD64)
- Multi-database support (ClickHouse for events, PostgreSQL for devices)
- Generic migration framework supporting multiple database types
- Environment-based configuration
- Integration test suite with testcontainers
- Automated database migrations via goose

## Project Structure

This is a Cargo workspace containing multiple crates:

```
ponix-rs/
├── Cargo.toml              # Workspace configuration
├── docker/                 # Docker Compose configurations
│   ├── docker-compose.deps.yaml    # Infrastructure (NATS, ClickHouse, PostgreSQL)
│   └── docker-compose.service.yaml # Application service
└── crates/
    ├── runner/             # Concurrent application runner
    ├── goose/              # Generic database migration framework
    ├── ponix-nats/         # NATS JetStream client
    ├── ponix-clickhouse/   # ClickHouse client and migrations
    ├── ponix-postgres/     # PostgreSQL client and device store
    └── ponix-all-in-one/   # Main service binary
```

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Docker and Docker Compose
- [mise](https://mise.jdx.dev/) (optional, for task management)
- Buf Schema Registry token (for protobuf types)

### BSR Authentication

This project uses protobuf types from the Buf Schema Registry:

1. Ensure BSR_TOKEN is configured in `.mise.toml`
2. Follow the [BSR Cargo Registry setup guide](https://buf.build/docs/bsr/generated-sdks/cargo/)
3. Authenticate once per machine:
   ```bash
   cargo login --registry buf "Bearer $BSR_TOKEN"
   ```

### Running Locally

**Start infrastructure services:**
```bash
docker-compose -f docker/docker-compose.deps.yaml up -d
```

**Build and run the service:**
```bash
# With Docker Compose
docker-compose -f docker/docker-compose.service.yaml up --build

# Or with Tilt (recommended for development)
tilt up
```

**Available services:**
- NATS: `localhost:4222` (monitoring: `localhost:8222`)
- ClickHouse: HTTP `localhost:8123`, native TCP `localhost:9000`
- PostgreSQL: `localhost:5432` (database: `ponix`, user: `ponix`, password: `ponix`)

### Running Tests

**Using mise (recommended):**
```bash
# Unit tests only (fast, no Docker required)
mise run test:unit

# Integration tests (requires Docker)
mise run test:integration

# All tests
mise run test:all
```

**Using cargo:**
```bash
# Unit tests
cargo test --workspace --lib --bins

# Integration tests
cargo test --workspace --features integration-tests -- --test-threads=1

# All tests
cargo test --workspace --features integration-tests
```

### Viewing Data

**ClickHouse (event storage):**
```bash
# Connect to ClickHouse
docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix

# Query envelopes
SELECT * FROM ponix.processed_envelopes LIMIT 10;

# View JSON data
SELECT end_device_id, data FROM ponix.processed_envelopes;
```

**PostgreSQL (device storage):**
```bash
# Connect to PostgreSQL
docker exec -it ponix-postgres psql -U ponix -d ponix

# Query devices
SELECT * FROM devices;

# List devices by organization
SELECT * FROM devices WHERE organization_id = 'org-001';

# View device details with timestamps
SELECT device_id, device_name, organization_id, created_at, updated_at
FROM devices ORDER BY created_at DESC;
```

## Development

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release
```

### Adding a New Crate

1. Create a new directory under `crates/`
2. Add the crate path to `Cargo.toml` workspace members
3. Create the crate with `cargo init --lib` or `cargo init --bin`

### Workspace Dependencies

Common dependencies are defined in the workspace `Cargo.toml`:

```toml
[dependencies]
tokio.workspace = true
```

## License

MIT

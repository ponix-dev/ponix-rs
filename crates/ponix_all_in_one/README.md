# ponix-all-in-one

The monolith binary that orchestrates all Ponix platform modules via the `Runner` concurrency framework.

## Running Locally

### Using Tilt (Recommended)
```bash
# Start the service with live reload
tilt up

# View logs at http://localhost:10350
```

### Using Docker Compose
```bash
# Start infrastructure
docker-compose -f docker/docker-compose.deps.yaml up -d

# Start service
docker-compose -f docker/docker-compose.service.yaml up --build
```

### Using Cargo
```bash
# From repository root (requires infrastructure running)
cargo run --bin ponix-all-in-one
```

## What It Runs

The service starts the following concurrent processes:

| Process | Description | Port |
|---------|-------------|------|
| `ponix_api` | gRPC API server (data streams, documents, orgs, gateways) | 50051 |
| `collaboration_server` | WebSocket server for real-time Yrs document collaboration | 50052 |
| `document_snapshotter` | NATS consumer that persists Yrs document state to PostgreSQL | - |
| `compaction_worker` | Periodic flush of dirty documents + idle eviction | - |
| `gateway_orchestrator` | Gateway CDC consumer + in-memory state machine | - |
| `cdc_worker` | PostgreSQL logical replication → NATS (gateways, documents) | - |
| `analytics_worker` | Raw → processed envelope pipeline (NATS → ClickHouse) | - |

## Configuration

See the root `CLAUDE.md` for full environment variable reference, or `src/config.rs` for the config schema.

## Architecture

The service initializes in phases:
1. Load config from environment
2. Run PostgreSQL + ClickHouse migrations
3. Initialize infrastructure clients (PostgreSQL, NATS, ClickHouse)
4. Create domain services and workers
5. Register all as `Runner` app processes
6. Run concurrently with graceful shutdown on SIGTERM/SIGINT

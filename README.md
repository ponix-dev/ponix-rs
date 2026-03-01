# Ponix

A data ingestion platform that transforms raw event data into structured, queryable insights.

## What is Ponix?

Ponix ingests event data from any source — IoT devices, webhooks, APIs, services — and transforms it into structured JSON using configurable expressions. Processed data is stored in [ClickHouse](https://clickhouse.com/) for fast analytical queries across millions of events.

### How it works

1. **Create a data stream** — define a named stream with a transformation expression
2. **Send events** — raw payloads are routed to the matching data stream
3. **Automatic transformation** — expressions convert raw data into structured JSON
4. **Query with SQL** — processed events land in ClickHouse, ready for analysis

### Key capabilities

- **Configurable transformations** — define how each data stream converts raw payloads to structured JSON
- **Analytical storage** — ClickHouse provides fast SQL queries across large volumes of event data
- **Real-time processing** — events are transformed and stored as they arrive
- **Multi-tenant** — organize data streams by organization and workspace
- **gRPC API** — manage data streams, organizations, and gateways programmatically

## Getting Started

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- [Rust](https://rustup.rs/) (for local development)
- (Optional) [Tilt](https://tilt.dev/) for live-reloading development
- (Optional) [mise](https://mise.jdx.dev/) for task management

### Running Ponix

```bash
# Start infrastructure
docker-compose -f docker/docker-compose.deps.yaml up -d

# Start Ponix
docker-compose -f docker/docker-compose.service.yaml up --build

# Or use Tilt for development (watches for changes and rebuilds)
tilt up
```

Once running, the gRPC API is available at `localhost:50051`.

### Try it out

```bash
# Install grpcurl
brew install grpcurl

# Create a data stream
grpcurl -plaintext -d '{
  "organization_id": "my-org",
  "name": "Temperature Sensor",
  "payload_conversion": "cayenne_lpp_decode(input)"
}' localhost:50051 ponix.data_stream.v1.DataStreamService/CreateDataStream
```

Ponix matches incoming events to their data stream, runs the transformation, and stores the result in ClickHouse:

```bash
# Query processed events
docker exec -it ponix-clickhouse clickhouse-client -u ponix --password ponix \
  -q "SELECT data_stream_id, received_at, data FROM ponix.processed_envelopes ORDER BY received_at DESC LIMIT 10"
```

### Running tests

```bash
# Unit tests (no Docker required)
cargo test --workspace --lib --bins

# Integration tests (requires Docker)
cargo test --workspace --features integration-tests -- --test-threads=1
```

## Supported Gateways

Gateways connect external data sources to Ponix. Currently supported:

- **MQTT** — subscribe to MQTT topics and ingest messages as events

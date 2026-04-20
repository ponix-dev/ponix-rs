---
title: CDC Worker
description: Captures PostgreSQL row-level changes via logical replication and publishes them as protobuf messages to NATS JetStream.
crate: cdc_worker
category: module
related-files:
  - crates/cdc_worker/src/cdc_worker.rs
  - crates/cdc_worker/src/domain/process.rs
  - crates/cdc_worker/src/domain/cdc_config.rs
  - crates/cdc_worker/src/domain/cdc_converter.rs
  - crates/cdc_worker/src/domain/entity_config.rs
  - crates/cdc_worker/src/domain/gateway_converter.rs
  - crates/cdc_worker/src/domain/organization_converter.rs
  - crates/cdc_worker/src/domain/end_device_converter.rs
  - crates/cdc_worker/src/domain/end_device_definition_converter.rs
  - crates/cdc_worker/src/domain/user_converter.rs
  - crates/cdc_worker/src/nats/nats_sink.rs
last-updated: 2026-03-18
---

# CDC Worker

The CDC Worker captures row-level changes from PostgreSQL using logical replication and publishes them as protobuf-encoded messages to NATS JetStream. This enables the rest of the system to react to database mutations in near real-time without coupling consumers directly to PostgreSQL, forming the backbone of the event-driven architecture that powers gateway orchestration and future downstream pipelines.

## Overview

The CDC Worker sits between PostgreSQL's Write-Ahead Log (WAL) and NATS JetStream. It uses the `etl` and `etl-postgres` libraries (from Supabase) to consume a PostgreSQL logical replication stream, then routes each change through entity-specific converters that transform raw table rows into typed protobuf messages. These messages are published to NATS subjects following the pattern `{entity_name}.{operation}` (e.g., `gateways.create`, `end_devices.update`, `organizations.delete`).

Key responsibilities:
- Consume PostgreSQL logical replication events via a named publication and replication slot
- Map table-level WAL events to domain entities using a configurable entity registry
- Convert row data from JSON into protobuf messages, applying entity-specific logic (field filtering, enum mapping, security redaction)
- Publish protobuf-encoded messages to NATS JetStream with per-event tracing
- Handle batching, retries, and graceful shutdown

## Key Concepts

- **`CdcWorker`** -- Top-level struct that holds configuration and produces a `Runner` app process. It is the entry point wired into `ponix_all_in_one`.

- **`CdcProcess`** -- The runtime process that configures and starts the ETL pipeline. It connects to PostgreSQL's logical replication slot, wires up the `NatsSink` as the pipeline destination, and runs until cancelled.

- **`CdcConverter` trait** -- The core abstraction for transforming CDC operations. Each entity implements `convert_insert`, `convert_update`, and `convert_delete`, receiving raw JSON column data and returning protobuf-encoded `Bytes`.

- **`EntityConfig`** -- Binds a PostgreSQL table name to an entity name and a `CdcConverter` implementation. This is how the system knows that changes to the `gateways` table should be routed through `GatewayConverter` and published to `gateways.*` subjects.

- **`NatsSink`** -- Implements the ETL `Destination` trait. It receives batches of replication events, resolves table IDs to entity configs via Relation events, converts rows to JSON using table schemas, delegates to the appropriate converter, and publishes to NATS.

- **`CdcConfig`** -- PostgreSQL connection details plus CDC tuning parameters (publication name, slot name, batch size, timeouts, retry policy).

## How It Works

### Logical Replication Setup

PostgreSQL must be configured with `wal_level=logical`. The CDC Worker connects using a named publication (`ponix_cdc_publication` by default) and replication slot (`ponix_cdc_slot`). The publication determines which tables emit WAL events. The replication slot tracks the consumer's position in the WAL so events are not lost across restarts.

### Event Processing Flow

The ETL library delivers events in batches to `NatsSink::write_events()`:

1. **Relation events** -- Received when PostgreSQL sends table schema metadata. The sink stores the schema (column names and types) and maps the table ID to the matching `EntityConfig` by table name. This mapping is essential because subsequent Insert/Update/Delete events reference tables by numeric ID, not name.

2. **Insert/Update/Delete events** -- The sink looks up the entity config by table ID, converts the row data to JSON using the stored table schema, then delegates to the converter's corresponding method. The converter produces protobuf-encoded bytes, which are published to a NATS subject like `{entity_name}.{operation}`.

3. **Begin/Commit/Truncate events** -- Silently ignored. The sink does not maintain transactional boundaries in NATS.

### Entity Converter Design

Each converter follows the same pattern: extract fields from JSON, map database values to protobuf enum variants, parse timestamps, build and encode the protobuf message. Notable behaviors:

- **UserConverter** excludes `password_hash` from CDC events for security.

- **GatewayConverter** extracts `broker_url` from the `gateway_config` JSONB column and maps it into the protobuf `Config` oneof as `EmqxGatewayConfig`.

- **OrganizationConverter** derives status enums from the presence of `deleted_at` (active vs. inactive).

### Error Handling

Individual event processing failures are logged and skipped -- the batch continues processing remaining events. This prevents a single malformed row from blocking the entire CDC pipeline.

## Configuration

| Environment Variable | Default | Description |
|---|---|---|
| `PONIX_CDC_PUBLICATION_NAME` | `ponix_cdc_publication` | PostgreSQL publication name |
| `PONIX_CDC_SLOT_NAME` | `ponix_cdc_slot` | PostgreSQL replication slot name |
| `PONIX_CDC_BATCH_SIZE` | `100` | Maximum events per batch |
| `PONIX_CDC_BATCH_TIMEOUT_MS` | `5000` | Maximum wait time to fill a batch |
| `PONIX_CDC_RETRY_DELAY_MS` | `10000` | Delay between retry attempts |
| `PONIX_CDC_MAX_RETRY_ATTEMPTS` | `5` | Maximum retry attempts per table error |

## Related Documentation

- [Common](common.md) — Proto converters and NATS publisher infrastructure used by NatsSink
- [Envelope Processing Pipeline](../data-flows/envelope-pipeline.md) — Separate data pipeline for IoT telemetry (not CDC-based)

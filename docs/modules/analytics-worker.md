---
title: Analytics Worker
description: Consumes IoT sensor payloads from NATS, transforms them via CEL expressions with contract-based routing, and stores structured results in ClickHouse.
crate: analytics_worker
category: module
related-files:
  - crates/analytics_worker/src/analytics_worker.rs
  - crates/analytics_worker/src/domain/raw_envelope_service.rs
  - crates/analytics_worker/src/domain/cel_converter.rs
  - crates/analytics_worker/src/domain/payload_converter.rs
  - crates/analytics_worker/src/domain/processed_envelope_service.rs
  - crates/analytics_worker/src/clickhouse/inserter_repository.rs
  - crates/analytics_worker/src/clickhouse/envelope_repository.rs
  - crates/analytics_worker/src/nats/raw_envelope_service.rs
  - crates/analytics_worker/src/nats/processed_envelope_service.rs
  - crates/analytics_worker/src/nats/processed_envelope_producer.rs
last-updated: 2026-03-18
---

# Analytics Worker

The analytics worker is the data processing backbone of Ponix. It transforms raw binary IoT sensor payloads into structured JSON and persists them in ClickHouse for time-series analytics. The crate exists to decouple payload ingestion from storage, using NATS JetStream as the intermediary so that each stage can scale and fail independently.

## Overview

The analytics worker runs three concurrent processes registered with the Runner framework:

1. **Raw envelope consumer** -- reads binary sensor payloads from the `raw_envelopes` NATS stream, validates ownership, applies contract-based CEL transformations, and republishes structured results to the `processed_envelopes` stream.
2. **Processed envelope consumer** -- reads structured envelopes from the `processed_envelopes` stream and buffers them into a ClickHouse inserter for batch writes.
3. **Inserter commit loop** -- periodically flushes the ClickHouse inserter buffer to disk, ensuring data reaches storage even under low throughput.

This two-stage pipeline (raw to processed to ClickHouse) means the transformation logic never blocks storage writes, and each consumer can Ack/Nak messages independently based on its own success criteria.

## Key Concepts

- **`RawEnvelopeService`** -- Domain service orchestrating the full raw-to-processed conversion: data stream lookup, organization validation, contract matching, CEL transformation, schema validation, and publishing. This is the core business logic of the crate.
- **`PayloadConverter` trait** -- Abstraction over CEL expression execution with two operations: `evaluate_match` (does this contract apply?) and `transform` (convert the binary payload to JSON). Mockable for unit tests.
- **`CelPayloadConverter`** -- Production implementation of `PayloadConverter` backed by `CelEnvironment`. Supports pre-compiled CEL expressions and the `cayenne_lpp_decode` built-in function for LoRaWAN sensor data.
- **`PayloadContract`** -- A data stream's contract defines a match expression, a transform expression, and a JSON Schema. Contracts are evaluated in order; the first match wins and claims ownership of the payload.
- **`ClickHouseInserterRepository`** -- Wraps ClickHouse's native `Inserter<T>` behind an `Arc<Mutex<...>>` for shared, buffered writes. Rows are cheap to buffer; actual flushes happen on a configurable time/count threshold.
- **`InserterCommitHandle`** -- Runs as a dedicated Runner process that calls `commit()` on the inserter at a regular interval, ensuring time-based flushes even when the row-count threshold has not been reached.
- **`TowerConsumer`** -- Both NATS consumers are Tower services with logging and tracing middleware layers, providing per-message observability with OpenTelemetry span propagation.

## How It Works

### Raw Envelope Processing Pipeline

When a gateway publishes a binary sensor payload, it arrives on the `raw_envelopes.*` NATS subject as a protobuf-encoded `RawEnvelope`.

**Step 1: Message consumption.** The `RawEnvelopeConsumerService` (a Tower `Service<ConsumeRequest>`) decodes the protobuf and converts it to the domain `RawEnvelope` type. Decode or conversion failures result in a Nak so NATS can redeliver.

**Step 2: Data stream lookup.** `RawEnvelopeService` fetches the `DataStreamWithDefinition` from PostgreSQL, which includes the data stream's payload contracts. If the data stream is not found, the envelope is rejected with `DataStreamNotFound`.

**Step 3: Organization validation.** The service checks that the owning organization exists and has not been soft-deleted. Envelopes from deleted or missing organizations are rejected. This prevents stale data streams from producing analytics data after an organization is deactivated.

**Step 4: Structural validation.** The `DataStreamWithDefinition` is validated with garde to ensure invariants hold (e.g., at least one contract must be defined).

**Step 5: Contract matching and transformation.** Contracts are evaluated in definition order. For each contract:

1. The **match expression** is evaluated as a boolean CEL expression against the binary payload (exposed as `input`). This allows routing different payload formats to different transformations -- for example, matching on payload size or header bytes.
2. If matched, the **transform expression** is executed to convert the binary payload into a JSON value. Built-in functions like `cayenne_lpp_decode(input)` handle protocol-specific decoding.
3. The result is validated against the contract's **JSON Schema**. If validation fails, the envelope is silently dropped (Ack without publish) rather than trying the next contract. This is intentional: a matching contract claims ownership, so a schema failure indicates malformed data from the expected source, not a routing mistake.

If no contract matches, the envelope is silently acknowledged and dropped. This is a normal condition -- not all payloads from a gateway may be relevant to a particular data stream.

Both match and transform expressions use pre-compiled CEL bytecode stored alongside the source expression. The `CelPayloadConverter` deserializes the compiled `CheckedExpr` and executes it, falling back to the source string only for error reporting. This avoids re-parsing CEL on every message.

**Step 6: Envelope construction.** The JSON output (which must be a JSON object, not a scalar or array) is combined with metadata (organization ID, data stream ID, received/processed timestamps) into a `ProcessedEnvelope`.

**Step 7: Publishing.** The `ProcessedEnvelopeProducer` serializes the domain envelope to protobuf and publishes it to `processed_envelopes.{data_stream_id}` via a Tower-layered NATS publisher with tracing and logging middleware. The subject-per-data-stream pattern enables downstream consumers to filter by data stream if needed.

### Processed Envelope Storage Pipeline

The `ProcessedEnvelopeService` (confusingly named the same as the domain service but living in `nats/`) is a Tower service consuming from the `processed_envelopes` stream. It decodes the protobuf, converts the domain type, and writes it to the `ClickHouseInserterRepository` buffer. This write is fast -- it just appends to an in-memory buffer.

The `InserterCommitHandle` runs a separate tokio loop that calls `commit()` at the configured interval (default 1 second). ClickHouse's native inserter also auto-flushes when the buffer reaches `max_entries` (default 10,000 rows). On graceful shutdown (cancellation token), the commit loop performs one final flush.

The `ProcessedEnvelopeRow` maps the domain type to ClickHouse's schema. Notably, the `data` field (a `serde_json::Map` in the domain) is serialized to a JSON string for storage in ClickHouse's `JSON` column type. The `received_at` and `processed_at` timestamps use ClickHouse's custom chrono serde format, which is required when JSON columns coexist with DateTime columns in the same row struct.

### Error Handling Philosophy

The pipeline distinguishes between retryable and non-retryable failures:

- **Nak (redeliver)**: Protobuf decode errors, NATS publish failures, ClickHouse buffer errors. These may succeed on retry.
- **Ack (drop silently)**: No contract matched, schema validation failed. These are expected conditions that will never succeed on retry.
- **Err (propagate)**: Data stream not found, organization deleted/missing, CEL execution failure. These are domain errors that surface for observability but still result in message acknowledgment to prevent infinite redelivery.

## Configuration

| Environment Variable | Default | Description |
|---|---|---|
| `PONIX_PROCESSED_ENVELOPES_STREAM` | `processed_envelopes` | NATS JetStream stream name for processed envelopes |
| `PONIX_NATS_RAW_STREAM` | `raw_envelopes` | NATS JetStream stream name for raw envelopes |
| `PONIX_NATS_BATCH_SIZE` | `30` | Number of messages to fetch per NATS batch |
| `PONIX_NATS_BATCH_WAIT_SECS` | `5` | Max seconds to wait for a full batch |
| `PONIX_CLICKHOUSE_URL` | `http://localhost:8123` | ClickHouse HTTP endpoint |
| `PONIX_CLICKHOUSE_NATIVE_URL` | `localhost:9000` | ClickHouse native protocol endpoint |
| `PONIX_CLICKHOUSE_DATABASE` | `ponix` | ClickHouse database name |
| `PONIX_CLICKHOUSE_USERNAME` | `ponix` | ClickHouse authentication user |
| `PONIX_CLICKHOUSE_PASSWORD` | `ponix` | ClickHouse authentication password |

## Related Documentation

- [Runner](runner.md) — Process lifecycle and graceful shutdown
- [Envelope Processing Pipeline](../data-flows/envelope-pipeline.md) — End-to-end data flow view

---
title: Envelope Processing Pipeline
description: End-to-end data flow for ingesting raw IoT payloads, transforming them via CEL expressions, and storing structured results in ClickHouse.
category: data-flow
related-files:
  - crates/common/src/domain/envelope.rs
  - crates/common/src/domain/end_device.rs
  - crates/common/src/domain/end_device_definition.rs
  - crates/analytics_worker/src/analytics_worker.rs
  - crates/analytics_worker/src/domain/raw_envelope_service.rs
  - crates/analytics_worker/src/domain/processed_envelope_service.rs
  - crates/analytics_worker/src/domain/cel_converter.rs
  - crates/analytics_worker/src/domain/payload_converter.rs
  - crates/analytics_worker/src/nats/raw_envelope_service.rs
  - crates/analytics_worker/src/nats/processed_envelope_service.rs
  - crates/analytics_worker/src/nats/processed_envelope_producer.rs
  - crates/analytics_worker/src/clickhouse/envelope_repository.rs
  - crates/analytics_worker/src/clickhouse/inserter_repository.rs
last-updated: 2026-03-18
---

# Envelope Processing Pipeline

The envelope processing pipeline transforms raw binary IoT payloads into structured JSON and persists them for analytics. It is the core data ingestion path of Ponix, bridging the gap between opaque device telemetry (e.g., Cayenne LPP-encoded sensor readings) and queryable, schema-validated records in ClickHouse. The pipeline is fully asynchronous, uses NATS JetStream for durable message passing between stages, and applies user-defined CEL expressions for payload transformation.

## Overview

A raw envelope enters the system as a binary payload associated with an end device. The pipeline consumes it from a NATS JetStream stream, looks up the end device's payload contracts (match expression, transform expression, JSON schema), executes CEL-based matching and transformation against the binary data, validates the output against a JSON Schema, and publishes the resulting structured JSON as a processed envelope to a second NATS stream. A separate consumer reads processed envelopes and writes them to ClickHouse via a buffered inserter for time-series analytics. The two-stream design decouples transformation from storage, allowing each stage to scale and fail independently.

## Key Concepts

- **`RawEnvelope`** -- The ingestion input. Contains `organization_id`, `end_device_id`, `received_at` timestamp, and a `payload: Vec<u8>` (opaque binary data from the IoT device).

- **`ProcessedEnvelope`** -- The pipeline output. Replaces the binary payload with `data: serde_json::Map<String, serde_json::Value>` (a flat JSON object) and adds a `processed_at` timestamp.

- **`EndDeviceWithDefinition`** -- An end device joined with its definition. Carries `contracts: Vec<PayloadContract>`, the ordered list of processing rules.

- **`PayloadContract`** -- A single processing rule consisting of a match expression (CEL boolean), a transform expression (CEL binary→JSON), and a JSON Schema for validation. Contracts are pre-compiled at definition time to avoid repeated parsing at runtime.

- **`PayloadConverter` trait** -- Abstraction with `evaluate_match()` and `transform()` methods. Implemented by `CelPayloadConverter`. Mockable for unit testing.

## How It Works

```
                         NATS JetStream
                    ┌─────────────────────┐
                    │  raw_envelopes.*    │
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │  RawEnvelopeConsumer │  Decode protobuf → domain RawEnvelope
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │  RawEnvelopeService  │
                    │                     │
                    │  1. Fetch EndDevice  │──→ PostgreSQL
                    │  2. Validate org    │──→ PostgreSQL
                    │  3. For each contract:
                    │     match(input)    │    CEL bool expression
                    │     transform(input)│    CEL: binary → JSON
                    │     validate(schema)│    JSON Schema check
                    │  4. Build Processed │
                    │     Envelope       │
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │  ProcessedEnvelope   │  Publish to
                    │  Producer           │  processed_envelopes.{end_device_id}
                    └─────────┬───────────┘
                              │
                         NATS JetStream
                    ┌─────────▼───────────┐
                    │ processed_envelopes │
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │  ProcessedEnvelope   │  Decode protobuf → buffer to
                    │  Consumer           │  ClickHouse inserter
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │  InserterCommitLoop  │  Periodic flush (1s or 10K rows)
                    └─────────┬───────────┘
                              │
                    ┌─────────▼───────────┐
                    │  ClickHouse         │  processed_envelopes table
                    └─────────────────────┘
```

### Contract Matching (First Match Wins)

Contracts are evaluated in definition order. For each contract:
1. **Match**: Evaluate the CEL boolean expression against the binary payload (exposed as `input`). If `false`, skip to the next contract.
2. **Transform**: Execute the CEL transform expression. Built-in functions like `cayenne_lpp_decode(input)` handle protocol-specific decoding.
3. **Validate**: Check the transformed JSON against the contract's JSON Schema. If validation fails, the message is silently dropped -- a matched contract "claims ownership," so subsequent contracts are not tried.

If no contract matches at all, the message is silently acknowledged. This is normal in IoT (e.g., device heartbeats).

### Error Handling

| Scenario | NATS Response | Rationale |
|----------|--------------|-----------|
| Protobuf decode failure | NAK | May succeed on retry |
| End device not found | NAK | May appear after migration |
| Organization deleted/missing | NAK | Transient state |
| No contract matches | ACK | Normal condition |
| Schema validation fails | ACK | Permanent, would fail forever |
| CEL execution error | NAK | May be transient |
| NATS publish failure | NAK | Network transient |
| ClickHouse buffer error | NAK | May succeed on retry |

### Process Structure

The `AnalyticsWorker` registers three concurrent Runner processes:

| Process | Role |
|---------|------|
| Raw envelope consumer | Pulls from `raw_envelopes`, transforms, publishes to `processed_envelopes` |
| Processed envelope consumer | Pulls from `processed_envelopes`, buffers to ClickHouse inserter |
| Inserter commit loop | Periodically flushes the ClickHouse inserter buffer |

## Related Documentation

- [Analytics Worker](../modules/analytics-worker.md) — Crate internals and detailed implementation
- [Common](../modules/common.md) — Domain types, producer traits, and NATS infrastructure

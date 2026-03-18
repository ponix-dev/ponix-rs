# Documentation Index

This is the master index for all project documentation. Use `/doc-search` to find docs by crate, topic, or source file.

## Architecture

Cross-cutting concepts spanning multiple crates.

| Document | Description |
|----------|-------------|
| *(none yet — use `/doc-format` to create)* | |

## Modules

One file per crate documenting internals, key types, and design decisions.

| Document | Description |
|----------|-------------|
| [All-in-One Service Binary](modules/ponix-all-in-one.md) | Monolith binary that orchestrates all workers, infrastructure clients, and domain services |
| [Analytics Worker](modules/analytics-worker.md) | Consumes IoT sensor payloads from NATS, transforms them via CEL expressions, and stores structured results in ClickHouse |
| [CDC Worker](modules/cdc-worker.md) | Captures PostgreSQL row-level changes via logical replication and publishes them as protobuf messages to NATS JetStream |
| [Collaboration Server](modules/collaboration-server.md) | Real-time collaborative document editing via Yrs CRDTs, WebSocket transport, and NATS-based cross-instance relay |
| [Common Crate](modules/common.md) | Shared domain types, repository traits, and infrastructure clients used by all Ponix workspace crates |
| [Gateway Orchestrator](modules/gateway-orchestrator.md) | Reactive gateway lifecycle manager that deploys MQTT subscriber processes from CDC events |
| [Goose Migration Runner](modules/goose.md) | Thin Rust wrapper around the goose binary for running database migrations |
| [Ponix API](modules/ponix-api.md) | gRPC API layer and domain services for the Ponix platform |
| [Runner](modules/runner.md) | Concurrent process runner with coordinated shutdown and cleanup for long-running services |

## Data Flows

End-to-end paths through the system.

| Document | Description |
|----------|-------------|
| [Documentation Lifecycle](data-flows/documentation-lifecycle.md) | How project documentation is created, discovered, and kept in sync with code changes through Claude Code skills |
| [Envelope Processing Pipeline](data-flows/envelope-pipeline.md) | End-to-end data flow for ingesting raw IoT payloads, transforming them via CEL expressions, and storing structured results in ClickHouse |

## Operations

Procedural "how to" guides.

| Document | Description |
|----------|-------------|
| *(none yet — use `/doc-format` to create)* | |

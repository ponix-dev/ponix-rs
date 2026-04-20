---
title: Gateway Orchestrator
description: Reactive gateway lifecycle manager that consumes CDC events to deploy, reconfigure, and teardown per-gateway MQTT subscriber processes.
crate: gateway_orchestrator
category: module
related-files:
  - crates/gateway_orchestrator/src/gateway_orchestrator.rs
  - crates/gateway_orchestrator/src/domain/gateway_orchestration_service.rs
  - crates/gateway_orchestrator/src/domain/gateway_deployer.rs
  - crates/gateway_orchestrator/src/domain/deployment_handle.rs
  - crates/gateway_orchestrator/src/domain/gateway_runner.rs
  - crates/gateway_orchestrator/src/domain/gateway_runner_factory.rs
  - crates/gateway_orchestrator/src/domain/in_process_deployer.rs
  - crates/gateway_orchestrator/src/domain/in_memory_deployment_handle_store.rs
  - crates/gateway_orchestrator/src/nats/gateway_cdc_consumer.rs
  - crates/gateway_orchestrator/src/nats/gateway_cdc_service.rs
  - crates/gateway_orchestrator/src/nats/raw_envelope_producer.rs
  - crates/gateway_orchestrator/src/mqtt/emqx_gateway_runner.rs
  - crates/gateway_orchestrator/src/mqtt/subscriber.rs
  - crates/gateway_orchestrator/src/mqtt/topic.rs
last-updated: 2026-03-18
---

# Gateway Orchestrator

The gateway orchestrator is a reactive process manager that maintains a 1:1 mapping between gateway database records and long-running MQTT subscriber processes. It watches for PostgreSQL changes via CDC events on NATS JetStream and automatically starts, restarts, or stops gateway subscriber processes in response. This eliminates the need for imperative "deploy gateway" API calls -- gateways are managed entirely through database state.

## Overview

The orchestrator serves as the bridge between the CRUD layer (where users create/update/delete gateways via the gRPC API) and the runtime layer (where MQTT subscriber processes connect to external brokers and ingest IoT data). Its key responsibilities are:

- **Startup hydration**: On boot, load all non-deleted gateways from PostgreSQL and start a subscriber process for each one, ensuring the system recovers its full runtime state after restarts.
- **CDC event handling**: Consume `gateway.{create|update|delete}` events from NATS JetStream and translate them into process lifecycle actions (start, restart, stop).
- **Config change detection**: Hash gateway configurations and compare against running process state to avoid unnecessary restarts when non-config fields are updated.
- **Soft delete awareness**: Treat an update with `deleted_at` set as a stop signal, so soft deletes flow through the same CDC update path as hard deletes.
- **Graceful shutdown**: Cancel all running gateway processes with a 5-second timeout when the service shuts down.

## Key Concepts

- **`GatewayOrchestrationService`** -- The central state machine that maps CDC event types (created, updated, deleted) to process lifecycle operations. Holds references to the deployer, handle store, runner factory, and raw envelope producer.

- **`GatewayRunner` (trait)** -- Defines how to connect to a specific gateway type's message broker. Each implementation (e.g., `EmqxGatewayRunner`) knows the protocol details: connecting, subscribing, converting messages to `RawEnvelope`s, and handling reconnection.

- **`GatewayDeployer` (trait)** -- Abstracts *where* a gateway process runs, separating deployment strategy from protocol logic. Currently only `InProcessDeployer` exists (spawns a tokio task), but the trait is designed for future Docker or Kubernetes deployers.

- **`DeploymentHandle` (trait)** -- A handle to a running gateway process. Provides `cancel()` and `wait_for_stop()` for lifecycle management, plus `config_hash()` for change detection.

- **`DeploymentHandleStore` (trait)** -- Registry of active deployment handles, keyed by gateway ID. The `InMemoryDeploymentHandleStore` uses a `RwLock<HashMap>` for concurrent access.

- **`GatewayRunnerFactory`** -- Registry pattern that maps `GatewayConfig` variants to runner constructors. Adding a new gateway type requires only registering a new constructor.

## How It Works

### Initialization

1. A `RawEnvelopeProducer` is created with a NATS publisher, shared across all gateway processes so incoming MQTT messages flow into the envelope processing pipeline.
2. A `GatewayRunnerFactory` is created and the `EmqxGatewayRunner` constructor is registered.
3. The `GatewayOrchestrationService` calls `launch_gateways()`, querying all non-deleted gateways from PostgreSQL and starting a process for each one. Individual failures are logged but don't prevent other gateways from starting.
4. A `GatewayCdcConsumer` subscribes to the gateway CDC stream on NATS JetStream.

### CDC Event Processing

When a CDC event arrives from NATS:

- **Created**: Checks for duplicate handles, resolves the runner via the factory, delegates to the deployer, and stores the returned handle.
- **Updated**: First checks for soft delete (`deleted_at` set) and stops the process if so. Otherwise, computes a config hash and compares against the stored handle's hash. If changed, the old process is stopped and a new one is started. If unchanged, no action is taken.
- **Deleted**: Removes the handle from the store, cancels the process, and waits for it to stop.

### MQTT Subscriber Lifecycle

Each gateway process runs an MQTT subscriber loop:

1. Connects to the MQTT 5 broker using the URL from `EmqxGatewayConfig`.
2. Subscribes to a shared subscription topic `$share/{gateway_id}/{org_id}/+`, enabling load balancing across multiple instances.
3. Incoming MQTT messages are parsed to extract `org_id` and `end_device_id` from the topic, then published as `RawEnvelope`s to NATS via the `RawEnvelopeProducer`.
4. On connection failure, retries with configurable delay (default 10s) up to max attempts (default 3).

### Config Change Detection

The `hash_gateway_config()` function produces a deterministic hash of configuration fields that affect runtime behavior. When a CDC update arrives, the new config hash is compared against the stored hash. This means metadata-only updates (like renaming a gateway) don't trigger a reconnection to the MQTT broker.

## Related Documentation

- [CDC Worker](cdc-worker.md) — Upstream producer of gateway CDC events from PostgreSQL WAL
- [Analytics Worker](analytics-worker.md) — Downstream consumer of `RawEnvelope`s produced by gateway subscribers
- [Envelope Processing Pipeline](../data-flows/envelope-pipeline.md) — End-to-end flow from raw envelope to ClickHouse
- [Common](common.md) — `Gateway`, `GatewayConfig`, `RawEnvelope`, and repository trait definitions
